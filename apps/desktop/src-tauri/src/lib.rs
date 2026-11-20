mod hermes_backend;
mod sqlite_store;
mod workspace;
mod remote;
mod ssh;
mod terminal;

use hermes_backend::{BackendInfo, HermesSupervisor, NativeHttpRequest, NativeHttpResponse};
use serde_json::Value;
use sqlite_store::{LocalBootstrapPayload, LocalMessageRow, LocalSessionRow};
use std::fs;
use tauri::{Manager, RunEvent, State, Window};
use workspace::{GitWorkspaceStatus, LinuxSystemStats, ShellExecResult, WorkspaceFileEntry};

fn profile_home(
    root: &std::path::Path,
    profile: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    match profile.filter(|name| !name.is_empty() && *name != "default") {
        None => Ok(root.to_path_buf()),
        Some(name) => {
            let mut components = std::path::Path::new(name).components();
            if !matches!(components.next(), Some(std::path::Component::Normal(_)))
                || components.next().is_some()
            {
                return Err("Invalid Hermes profile name".into());
            }
            let home = root.join("profiles").join(name);
            if !home.is_dir() {
                return Err(format!("Hermes profile does not exist: {name}"));
            }
            Ok(home)
        }
    }
}

/// Verify the installed Hermes runtime through the same Rust path used by the
/// desktop app, without requiring a display or submitting an inference request.
pub fn diagnose_backend() -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let supervisor = HermesSupervisor::new();
        let local = sqlite_store::load_local_bootstrap(supervisor.hermes_home());
        supervisor.start_in_background().await;
        let result = async {
            for _ in 0..150 {
                let info = supervisor.info().await;
                if info.status == "error" { return Err(info.error.unwrap_or_else(|| "Backend failed".into())); }
                if info.status == "ready" {
                    let url = info.url.ok_or("Missing backend URL")?;
                    let mut headers = std::collections::HashMap::new();
                    headers.insert("X-Hermes-Session-Token".into(), info.token.ok_or("Missing session token")?);
                    let response = supervisor.proxy_http(NativeHttpRequest {
                        url: format!("{url}/api/model/info"), method: "GET".into(), headers: Some(headers), body: None,
                    }).await?;
                    if response.status != 200 { return Err(format!("Authenticated model API returned {}", response.status)); }
                    println!("{}", serde_json::json!({
                        "backend": "ready", "authenticated_http": true,
                        "boot_ms": info.boot_ms, "local_load_ms": local.elapsed_ms,
                        "sessions": local.sessions.len(), "skills": local.skills.len(), "models": local.models.len(),
                    }));
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            Err("Backend startup timed out".into())
        }.await;
        supervisor.stop().await;
        result
    })
}

#[tauri::command]
async fn terminal_start(manager: State<'_, terminal::TerminalManager>, id: String, cwd: String, cols: u16, rows: u16, ssh: Option<ssh::SshTarget>, on_event: tauri::ipc::Channel<terminal::TerminalEvent>) -> Result<terminal::TerminalInfo, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.start(id, cwd, cols, rows, ssh, on_event)).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn terminal_write(manager: State<'_, terminal::TerminalManager>, id: String, data: String) -> Result<(), String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.write(&id, data.as_bytes())).await.map_err(|e| e.to_string())?
}
#[tauri::command]
fn terminal_acknowledge(manager: State<'_, terminal::TerminalManager>, id: String, generation: String, bytes: usize) { manager.acknowledge(&id, &generation, bytes); }
#[tauri::command]
fn terminal_resize(manager: State<'_, terminal::TerminalManager>, id: String, cols: u16, rows: u16) -> Result<(), String> { manager.resize(&id, cols, rows) }
#[tauri::command]
fn terminal_info(manager: State<'_, terminal::TerminalManager>, id: String) -> Result<terminal::TerminalInfo, String> { manager.info(&id) }
#[tauri::command]
async fn terminal_close(manager: State<'_, terminal::TerminalManager>, id: String) -> Result<(), String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.close(&id)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn begin_remote_login(login: State<'_, remote::RemoteLogin>, url: String, provider: String) -> Result<String, String> {
    login.begin(url, provider).await
}
#[tauri::command]
async fn poll_remote_login(login: State<'_, remote::RemoteLogin>) -> Result<Option<Value>, String> { login.poll().await }
#[tauri::command]
async fn cancel_remote_login(login: State<'_, remote::RemoteLogin>) -> Result<(), String> { login.cancel().await; Ok(()) }
#[tauri::command]
async fn refresh_remote_login(url: String, refresh_token: String, provider: String) -> Result<Value, String> {
    remote::refresh(url, refresh_token, provider).await
}
#[tauri::command]
fn load_connection() -> Result<Option<Value>, String> { remote::read_connection(&remote::connection_path()?) }
#[tauri::command]
fn save_connection(connection: Value) -> Result<(), String> { remote::save_connection(&remote::connection_path()?, &connection) }
#[tauri::command]
async fn start_local_backend(sup: State<'_, HermesSupervisor>) -> Result<(), String> { sup.start_in_background().await; Ok(()) }
#[tauri::command]
async fn stop_local_backend(sup: State<'_, HermesSupervisor>) -> Result<(), String> { sup.stop().await; Ok(()) }

#[tauri::command]
async fn get_backend_info(sup: State<'_, HermesSupervisor>) -> Result<BackendInfo, String> {
    Ok(sup.info().await)
}

#[tauri::command]
async fn restart_hermes_backend(sup: State<'_, HermesSupervisor>) -> Result<BackendInfo, String> {
    Ok(sup.restart().await)
}

#[tauri::command]
async fn hermes_http_request(
    sup: State<'_, HermesSupervisor>,
    req: NativeHttpRequest,
) -> Result<NativeHttpResponse, String> {
    sup.proxy_http(req).await
}

#[tauri::command]
fn load_local_bootstrap(
    sup: State<'_, HermesSupervisor>,
    profile: Option<String>,
) -> Result<LocalBootstrapPayload, String> {
    Ok(sqlite_store::load_local_bootstrap(&profile_home(
        sup.hermes_home(),
        profile.as_deref(),
    )?))
}

#[tauri::command]
fn load_local_sessions(
    sup: State<'_, HermesSupervisor>,
    limit: Option<usize>,
) -> Result<Vec<LocalSessionRow>, String> {
    Ok(sqlite_store::load_local_sessions(
        sup.hermes_home(),
        limit.unwrap_or(120),
    ))
}

#[tauri::command]
fn load_local_session_messages(
    sup: State<'_, HermesSupervisor>,
    session_id: String,
    limit: Option<usize>,
    profile: Option<String>,
) -> Result<Vec<LocalMessageRow>, String> {
    Ok(sqlite_store::load_local_session_messages(
        &profile_home(sup.hermes_home(), profile.as_deref())?,
        &session_id,
        limit.unwrap_or(500),
    ))
}

#[tauri::command]
fn load_board_state(sup: State<'_, HermesSupervisor>) -> Result<Value, String> {
    Ok(sqlite_store::load_board_state(sup.hermes_home()))
}

#[tauri::command]
fn save_board_state(sup: State<'_, HermesSupervisor>, state: Value) -> Result<bool, String> {
    sqlite_store::save_board_state(sup.hermes_home(), &state)?;
    Ok(true)
}

#[tauri::command]
fn save_local_memory_file(
    sup: State<'_, HermesSupervisor>,
    kind: String,
    content: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let home = profile_home(sup.hermes_home(), profile.as_deref())?;
    let target = match kind.as_str() {
        "soul" => home.join("SOUL.md"),
        "memory" => home.join("memories/MEMORY.md"),
        "user" => home.join("memories/USER.md"),
        _ => return Err(format!("Unsupported memory file kind: {kind}")),
    };
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&target, content).map_err(|e| format!("Failed saving {}: {e}", target.display()))?;
    Ok(true)
}

#[tauri::command]
fn list_workspace_files(dir_path: String) -> Result<Vec<WorkspaceFileEntry>, String> {
    workspace::list_workspace_files(&dir_path)
}

#[tauri::command]
fn read_workspace_file(file_path: String) -> Result<String, String> {
    workspace::read_workspace_file(&file_path)
}

#[tauri::command]
async fn git_workspace_status(dir_path: String, ssh: Option<ssh::SshTarget>) -> Result<GitWorkspaceStatus, String> {
    workspace::git_workspace_status(&dir_path, ssh.as_ref()).await
}

#[tauri::command]
async fn run_shell_command(cwd: String, command: String) -> Result<ShellExecResult, String> {
    workspace::run_shell_command(&cwd, &command).await
}

#[tauri::command]
fn get_linux_system_stats() -> Result<LinuxSystemStats, String> {
    Ok(workspace::get_linux_system_stats())
}

#[tauri::command]
fn window_minimize(window: Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_toggle_maximize(window: Window) -> Result<(), String> {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn window_close(window: Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_start_drag(window: Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let supervisor = HermesSupervisor::new();
    let sup_for_exit = supervisor.clone();
    let terminals = terminal::TerminalManager::default();
    let terminals_for_exit = terminals.clone();

    let app = tauri::Builder::default()
        .manage(supervisor)
        .manage(terminals)
        .manage(remote::RemoteLogin::default())
        .setup(move |app| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_title("Hermes");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            terminal_start, terminal_write, terminal_acknowledge, terminal_resize, terminal_info, terminal_close,
            begin_remote_login, poll_remote_login, cancel_remote_login, refresh_remote_login,
            load_connection, save_connection, start_local_backend, stop_local_backend,
            get_backend_info,
            restart_hermes_backend,
            hermes_http_request,
            load_local_bootstrap,
            load_local_sessions,
            load_local_session_messages,
            load_board_state,
            save_board_state,
            save_local_memory_file,
            list_workspace_files,
            read_workspace_file,
            git_workspace_status,
            run_shell_command,
            get_linux_system_stats,
            window_minimize,
            window_toggle_maximize,
            window_close,
            window_start_drag,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Hermes Grok Bot Tauri application");

    app.run(move |_app_handle, event| {
        if let RunEvent::Exit = event {
            terminals_for_exit.stop_all();
            let sup = sup_for_exit.clone();
            tauri::async_runtime::block_on(async move {
                sup.stop().await;
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::profile_home;
    #[test]
    fn memory_and_history_resolve_inside_the_selected_profile() {
        let root =
            std::env::temp_dir().join(format!("hermes-ui-profiles-{}", rand::random::<u64>()));
        let named = root.join("profiles/research");
        std::fs::create_dir_all(&named).unwrap();
        assert_eq!(profile_home(&root, Some("default")).unwrap(), root);
        assert_eq!(profile_home(&root, Some("research")).unwrap(), named);
        for invalid in ["..", "../default", "/tmp", "missing"] {
            assert!(profile_home(&root, Some(invalid)).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
