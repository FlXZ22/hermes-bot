//! Private subprocess protocol for the Chromium desktop host.
use crate::{
    hermes_backend::{HermesSupervisor, NativeHttpRequest},
    profile_home, remote, save_local_memory_file, sqlite_store,
    terminal::{EventSink, TerminalManager},
    updater::{AppUpdater, UpdateSettingsPatch},
    workspace,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::Arc,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, BufReader},
    sync::mpsc,
    task::JoinSet,
};

const MAX_FRAME: usize = 48 * 1024 * 1024;
const MAX_CONCURRENT: usize = 32;

#[derive(Deserialize)]
struct Request {
    id: u32,
    command: String,
    #[serde(default)]
    args: Value,
}

struct Service {
    supervisor: HermesSupervisor,
    terminals: TerminalManager,
    login: remote::RemoteLogin,
    updater: AppUpdater,
}

fn args<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid native arguments: {error}"))
}

fn result<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

async fn blocking<T: Serialize + Send + 'static>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Value, String> {
    result(
        tokio::task::spawn_blocking(operation)
            .await
            .map_err(|error| error.to_string())??,
    )
}

impl Service {
    async fn invoke(
        self: Arc<Self>,
        request: Request,
        output: mpsc::Sender<Value>,
    ) -> Result<Value, String> {
        let value = request.args;
        match request.command.as_str() {
            "start_local_backend" => {
                self.supervisor.start_in_background().await;
                Ok(Value::Null)
            }
            "stop_local_backend" => {
                self.supervisor.stop().await;
                Ok(Value::Null)
            }
            "get_backend_info" => result(self.supervisor.info().await),
            "restart_hermes_backend" => result(self.supervisor.restart().await),
            "hermes_http_request" => {
                #[derive(Deserialize)]
                struct Args {
                    req: NativeHttpRequest,
                }
                let Args { req } = args(value)?;
                let url = reqwest::Url::parse(&req.url).map_err(|_| "Invalid gateway address")?;
                if !matches!(url.scheme(), "http" | "https")
                    || !url.username().is_empty()
                    || url.password().is_some()
                {
                    return Err("Gateway requests require an HTTP or HTTPS address without embedded credentials".into());
                }
                result(self.supervisor.proxy_http(req).await?)
            }
            "load_connection" => {
                blocking(|| remote::read_connection(&remote::connection_path()?)).await
            }
            "save_connection" => {
                #[derive(Deserialize)]
                struct Args {
                    connection: Value,
                }
                let Args { connection } = args(value)?;
                blocking(move || remote::save_connection(&remote::connection_path()?, &connection))
                    .await
            }
            "begin_remote_login" => {
                #[derive(Deserialize)]
                struct Args {
                    url: String,
                    provider: String,
                }
                let Args { url, provider } = args(value)?;
                result(self.login.begin(url, provider).await?)
            }
            "poll_remote_login" => result(self.login.poll().await?),
            "cancel_remote_login" => {
                self.login.cancel().await;
                Ok(Value::Null)
            }
            "refresh_remote_login" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Args {
                    url: String,
                    refresh_token: String,
                    provider: String,
                }
                let Args {
                    url,
                    refresh_token,
                    provider,
                } = args(value)?;
                remote::refresh(url, refresh_token, provider).await
            }
            "load_local_bootstrap" => {
                #[derive(Deserialize)]
                struct Args {
                    profile: Option<String>,
                }
                let Args { profile } = args(value)?;
                blocking(move || {
                    let home = profile_home(self.supervisor.hermes_home(), profile.as_deref())?;
                    Ok(sqlite_store::load_local_bootstrap(&home))
                })
                .await
            }
            "load_local_session_messages" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Args {
                    session_id: String,
                    limit: Option<usize>,
                    profile: Option<String>,
                }
                let Args {
                    session_id,
                    limit,
                    profile,
                } = args(value)?;
                blocking(move || {
                    let home = profile_home(self.supervisor.hermes_home(), profile.as_deref())?;
                    Ok(sqlite_store::load_local_session_messages(
                        &home,
                        &session_id,
                        limit.unwrap_or(500).min(500),
                    ))
                })
                .await
            }
            "load_board_state" => {
                blocking(move || {
                    Ok(sqlite_store::load_board_state(
                        self.supervisor.hermes_home(),
                    ))
                })
                .await
            }
            "save_board_state" => {
                #[derive(Deserialize)]
                struct Args {
                    state: Value,
                }
                let Args { state } = args(value)?;
                blocking(move || {
                    sqlite_store::save_board_state(self.supervisor.hermes_home(), &state)?;
                    Ok(true)
                })
                .await
            }
            "save_local_memory_file" => {
                #[derive(Deserialize)]
                struct Args {
                    kind: String,
                    content: String,
                    profile: Option<String>,
                }
                let Args {
                    kind,
                    content,
                    profile,
                } = args(value)?;
                blocking(move || save_local_memory_file(&self.supervisor, kind, content, profile))
                    .await
            }
            "list_workspace_files" | "git_workspace_status" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Args {
                    dir_path: String,
                    ssh: Option<crate::ssh::SshTarget>,
                }
                let Args { dir_path, ssh } = args(value)?;
                if request.command == "list_workspace_files" {
                    blocking(move || workspace::list_workspace_files(&dir_path)).await
                } else {
                    result(workspace::git_workspace_status(&dir_path, ssh.as_ref()).await?)
                }
            }
            "read_workspace_file" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Args {
                    file_path: String,
                }
                let Args { file_path } = args(value)?;
                blocking(move || workspace::read_workspace_file(&file_path)).await
            }
            "get_linux_system_stats" => blocking(|| Ok(workspace::get_linux_system_stats())).await,
            "terminal_start" => {
                #[derive(Deserialize)]
                struct Args {
                    id: String,
                    cwd: String,
                    cols: u16,
                    rows: u16,
                    ssh: Option<crate::ssh::SshTarget>,
                }
                let Args {
                    id,
                    cwd,
                    cols,
                    rows,
                    ssh,
                } = args(value)?;
                let terminal_id = id.clone();
                let subscription = request.id;
                let sink =
                    EventSink::new(move |event| {
                        output.blocking_send(json!({
                    "terminalId": terminal_id, "subscription": subscription, "event": event,
                })).map_err(|_| ())
                    });
                blocking(move || self.terminals.start(id, cwd, cols, rows, ssh, sink)).await
            }
            "terminal_write" => {
                #[derive(Deserialize)]
                struct Args {
                    id: String,
                    data: String,
                }
                let Args { id, data } = args(value)?;
                if data.len() > 1024 * 1024 {
                    return Err("Terminal input is too large".into());
                }
                blocking(move || self.terminals.write(&id, data.as_bytes())).await
            }
            "terminal_acknowledge" => {
                #[derive(Deserialize)]
                struct Args {
                    id: String,
                    generation: String,
                    bytes: usize,
                }
                let Args {
                    id,
                    generation,
                    bytes,
                } = args(value)?;
                self.terminals.acknowledge(&id, &generation, bytes);
                Ok(Value::Null)
            }
            "terminal_resize" => {
                #[derive(Deserialize)]
                struct Args {
                    id: String,
                    cols: u16,
                    rows: u16,
                }
                let Args { id, cols, rows } = args(value)?;
                blocking(move || self.terminals.resize(&id, cols, rows)).await
            }
            "terminal_info" | "terminal_close" => {
                #[derive(Deserialize)]
                struct Args {
                    id: String,
                }
                let Args { id } = args(value)?;
                if request.command == "terminal_info" {
                    blocking(move || self.terminals.info(&id)).await
                } else {
                    blocking(move || {
                        self.terminals.close(&id);
                        Ok(())
                    })
                    .await
                }
            }
            "get_update_status" => result(self.updater.status()),
            "check_for_updates" => result(self.updater.check_for_updates().await?),
            "set_update_settings" => {
                #[derive(Deserialize)]
                struct Args {
                    patch: UpdateSettingsPatch,
                }
                let Args { patch } = args(value)?;
                result(self.updater.update_settings(patch)?)
            }
            "apply_app_update" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Args {
                    mode: String,
                    #[serde(default)]
                    force_native_rebuild: bool,
                }
                let Args {
                    mode,
                    force_native_rebuild,
                } = args(value)?;
                result(self.updater.apply_update(mode, force_native_rebuild)?)
            }
            _ => Err("Unknown native command".into()),
        }
    }
}

async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
    frame: &mut Vec<u8>,
) -> io::Result<bool> {
    frame.clear();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return if frame.is_empty() {
                Ok(false)
            } else {
                Err(io::ErrorKind::UnexpectedEof.into())
            };
        }
        let end = chunk.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(chunk.len(), |index| index + 1);
        if frame.len() + count > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Oversized native request",
            ));
        }
        frame.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(true);
        }
    }
}

pub async fn run() -> Result<(), String> {
    let service = Arc::new(Service {
        supervisor: HermesSupervisor::new(),
        terminals: TerminalManager::default(),
        login: remote::RemoteLogin::default(),
        updater: AppUpdater::new(),
    });
    let (send, mut receive) = mpsc::channel::<Value>(64);
    let writer = std::thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        while let Some(frame) = receive.blocking_recv() {
            if serde_json::to_writer(&mut stdout, &frame).is_err()
                || stdout.write_all(b"\n").is_err()
                || stdout.flush().is_err()
            {
                break;
            }
        }
    });
    let mut input = BufReader::new(tokio::io::stdin());
    let mut frame = Vec::new();
    let mut tasks = JoinSet::new();
    let finished = loop {
        match read_frame(&mut input, &mut frame).await {
            Ok(true) => {}
            Ok(false) => break Ok(()),
            Err(error) => break Err(error.to_string()),
        }
        let request: Request = match serde_json::from_slice(&frame) {
            Ok(request) => request,
            Err(_) => break Err("Invalid request from desktop host".into()),
        };
        while tasks.try_join_next().is_some() {}
        let limit = if request.command == "terminal_acknowledge" {
            MAX_CONCURRENT + 16
        } else {
            MAX_CONCURRENT
        };
        if tasks.len() >= limit {
            let _ = send
                .send(json!({"id": request.id, "error": "Native service is busy"}))
                .await;
            continue;
        }
        let service = service.clone();
        let output = send.clone();
        let id = request.id;
        tasks.spawn(async move {
            let frame = match service.invoke(request, output.clone()).await {
                Ok(result) => json!({ "id": id, "result": result }),
                Err(error) => json!({ "id": id, "error": error }),
            };
            let _ = output.send(frame).await;
        });
    };
    tasks.shutdown().await;
    service.login.cancel().await;
    service.terminals.stop_all();
    service.supervisor.stop().await;
    drop(service);
    drop(send);
    let _ = writer.join();
    finished
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn read_frame_handles_newlines_eof_and_unterminated_input() {
        let mut cursor = Cursor::new(b"{\"id\":1,\"command\":\"get_backend_info\"}\n".to_vec());
        let mut frame = Vec::new();
        assert!(read_frame(&mut cursor, &mut frame).await.unwrap());
        assert_eq!(frame, b"{\"id\":1,\"command\":\"get_backend_info\"}\n");
        assert!(!read_frame(&mut cursor, &mut frame).await.unwrap());

        let mut incomplete = Cursor::new(b"{\"id\":2".to_vec());
        let err = read_frame(&mut incomplete, &mut frame).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn service_rejects_unknown_commands_and_embedded_url_credentials() {
        let service = Arc::new(Service {
            supervisor: HermesSupervisor::new(),
            terminals: TerminalManager::default(),
            login: remote::RemoteLogin::default(),
            updater: AppUpdater::new(),
        });
        let (tx, _rx) = mpsc::channel(4);

        let unknown = service
            .clone()
            .invoke(
                Request {
                    id: 1,
                    command: "not_a_command".into(),
                    args: json!({}),
                },
                tx.clone(),
            )
            .await;
        assert_eq!(unknown.unwrap_err(), "Unknown native command");

        let bad_url = service
            .invoke(
                Request {
                    id: 2,
                    command: "hermes_http_request".into(),
                    args: json!({
                        "req": {
                            "url": "http://user:pass@127.0.0.1:9119/api/status",
                            "method": "GET"
                        }
                    }),
                },
                tx,
            )
            .await;
        assert!(bad_url
            .unwrap_err()
            .contains("without embedded credentials"));
    }
}
