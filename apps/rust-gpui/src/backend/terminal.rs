//! Rust-owned interactive PTYs. Renderer channels may detach without killing a shell.
use crate::ssh::SshTarget;
use base64::{engine::general_purpose::STANDARD, Engine};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
};
#[derive(Clone)]
pub struct EventSink(Arc<dyn Fn(TerminalEvent) -> Result<(), ()> + Send + Sync>);

impl EventSink {
    pub fn new(send: impl Fn(TerminalEvent) -> Result<(), ()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(send))
    }

    fn send(&self, event: TerminalEvent) -> Result<(), ()> {
        (self.0)(event)
    }
}

const MAX_SESSIONS: usize = 16;
const BUFFER_BYTES: usize = 1024 * 1024;
const OUTPUT_WINDOW: usize = 256 * 1024;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Started { reused: bool, generation: String },
    Output { data: String },
    Exit { code: u32 },
    Error { message: String },
}

#[derive(Clone, Serialize)]
pub struct TerminalInfo {
    pub id: String,
    pub pid: Option<u32>,
    pub cwd: String,
    pub shell: String,
    pub remote: bool,
    pub exit_code: Option<u32>,
}

struct OutputState {
    buffer: VecDeque<u8>,
    channel: Option<EventSink>,
    exit_code: Option<u32>,
    generation: String,
    in_flight: usize,
}

struct Session {
    info: TerminalInfo,
    stopped: AtomicBool,
    target: Option<SshTarget>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    output: Mutex<OutputState>,
    flow: Condvar,
}

impl Session {
    fn emit(&self, event: TerminalEvent) {
        let mut state = self.output.lock().unwrap();
        if let TerminalEvent::Exit { code } = event {
            state.exit_code = Some(code);
        }
        if state
            .channel
            .as_ref()
            .is_some_and(|channel| channel.send(event).is_err())
        {
            state.channel = None;
        }
    }

    fn info(&self) -> TerminalInfo {
        let mut info = self.info.clone();
        info.exit_code = self.output.lock().unwrap().exit_code;
        if !info.remote && info.exit_code.is_none() {
            if let Some(pid) = info.pid {
                if let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) {
                    info.cwd = cwd.to_string_lossy().into_owned();
                }
            }
        }
        info
    }

    fn stop(self: &Arc<Self>) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.flow.notify_all();
        let alive = self.output.lock().unwrap().exit_code.is_none();
        if alive {
            // Ctrl+C's foreground job may have its own process group. Hang up
            // that group as well as the shell, then reap the child on its waiter.
            let master = self.master.lock().unwrap();
            if let Some(fd) = master.as_raw_fd() {
                let foreground = unsafe { libc::tcgetpgrp(fd) };
                if foreground > 1 {
                    unsafe {
                        libc::kill(-foreground, libc::SIGHUP);
                    }
                }
            }
            let _ = self.killer.lock().unwrap().kill();
        }
        let stopping = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if stopping.output.lock().unwrap().exit_code.is_none() {
                if let Some(pid) = stopping.info.pid {
                    unsafe {
                        libc::kill(pid as i32, libc::SIGKILL);
                    }
                }
            }
        });
        self.writer.lock().unwrap().take();
        self.output.lock().unwrap().channel = None;
    }
}

#[derive(Clone, Default)]
pub struct TerminalManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Session>>>>,
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.sessions) == 1 {
            self.stop_all();
        }
    }
}

impl TerminalManager {
    pub fn start(
        &self,
        id: String,
        cwd: String,
        cols: u16,
        rows: u16,
        ssh: Option<SshTarget>,
        channel: EventSink,
    ) -> Result<TerminalInfo, String> {
        if id.is_empty() || id.len() > 160 {
            return Err("Invalid terminal identifier".into());
        }
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get(&id) {
            if session.target != ssh {
                return Err("Terminal target does not match this connection".into());
            }
            let mut output = session.output.lock().unwrap();
            output.generation = rand::random::<u64>().to_string();
            output.in_flight = output.buffer.len();
            channel
                .send(TerminalEvent::Started {
                    reused: true,
                    generation: output.generation.clone(),
                })
                .map_err(|_| "Terminal output channel closed".to_owned())?;
            if !output.buffer.is_empty() {
                channel
                    .send(TerminalEvent::Output {
                        data: STANDARD.encode(output.buffer.make_contiguous()),
                    })
                    .map_err(|_| "Terminal output channel closed".to_owned())?;
            }
            if let Some(code) = output.exit_code {
                let _ = channel.send(TerminalEvent::Exit { code });
            }
            output.channel = Some(channel);
            session.flow.notify_all();
            drop(output);
            return Ok(session.info());
        }
        if sessions.len() >= MAX_SESSIONS {
            return Err("Close a terminal before opening another (16 tabs maximum)".into());
        }
        let remote = ssh.is_some();
        let directory = if remote {
            dirs::home_dir().unwrap_or_default()
        } else {
            crate::workspace::expand_dir(&cwd)
        };
        if !directory.is_dir() {
            return Err(format!(
                "Terminal directory does not exist: {}",
                directory.display()
            ));
        }
        let (mut command, shell) = if let Some(target) = &ssh {
            let mut command = CommandBuilder::new("ssh");
            command.args(target.args(true, &cwd, None)?);
            (command, "SSH".to_owned())
        } else {
            let shell = std::env::var("SHELL")
                .ok()
                .filter(|value| std::path::Path::new(value).is_file())
                .unwrap_or_else(|| "/bin/bash".into());
            let mut command = CommandBuilder::new(&shell);
            command.arg("-l");
            let name = std::path::Path::new(&shell)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (command, name)
        };
        command.cwd(&directory);
        for (key, _) in std::env::vars().filter(|(key, _)| {
            key.starts_with("npm_config_")
                || key.starts_with("npm_package_")
                || ["NO_COLOR", "FORCE_COLOR", "COLORFGBG"].contains(&key.as_str())
        }) {
            command.env_remove(key);
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "Hermes");
        command.env("HERMES_DESKTOP_TERMINAL", "1");
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: rows.clamp(2, 1000),
                cols: cols.clamp(2, 1000),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| format!("Could not start terminal: {e}"))?;
        drop(pair.slave);
        let info = TerminalInfo {
            id: id.clone(),
            pid: child.process_id(),
            cwd: if remote {
                cwd
            } else {
                directory.to_string_lossy().into_owned()
            },
            shell,
            remote,
            exit_code: None,
        };
        let session = Arc::new(Session {
            info: info.clone(),
            stopped: AtomicBool::new(false),
            target: ssh,
            master: Mutex::new(pair.master),
            writer: Mutex::new(Some(writer)),
            killer: Mutex::new(child.clone_killer()),
            output: Mutex::new(OutputState {
                buffer: VecDeque::with_capacity(BUFFER_BYTES),
                channel: Some(channel),
                exit_code: None,
                generation: rand::random::<u64>().to_string(),
                in_flight: 0,
            }),
            flow: Condvar::new(),
        });
        let generation = session.output.lock().unwrap().generation.clone();
        session.emit(TerminalEvent::Started {
            reused: false,
            generation,
        });
        sessions.insert(id, session.clone());
        let reader_fd = session.master.lock().unwrap().as_raw_fd();
        let reading = session.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                {
                    let mut state = reading.output.lock().unwrap();
                    while state.channel.is_some()
                        && state.in_flight >= OUTPUT_WINDOW
                        && !reading.stopped.load(Ordering::Relaxed)
                    {
                        state = reading
                            .flow
                            .wait_timeout(state, std::time::Duration::from_millis(250))
                            .unwrap()
                            .0;
                    }
                }
                if reading.stopped.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(fd) = reader_fd {
                    let mut poll = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let ready = unsafe { libc::poll(&mut poll, 1, 1000) };
                    if ready <= 0 {
                        continue;
                    }
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        let mut state = reading.output.lock().unwrap();
                        let excess = (state.buffer.len() + size).saturating_sub(BUFFER_BYTES);
                        state.buffer.drain(..excess);
                        state.buffer.extend(&buffer[..size]);
                        if let Some(channel) = &state.channel {
                            if channel
                                .send(TerminalEvent::Output {
                                    data: STANDARD.encode(&buffer[..size]),
                                })
                                .is_err()
                            {
                                state.channel = None;
                                state.in_flight = 0;
                            } else {
                                state.in_flight += size;
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break, // Linux PTYs report EIO after the slave exits.
                }
            }
        });
        std::thread::spawn(move || match child.wait() {
            Ok(status) => session.emit(TerminalEvent::Exit {
                code: status.exit_code(),
            }),
            Err(error) => session.emit(TerminalEvent::Error {
                message: format!("Terminal exited: {error}"),
            }),
        });
        Ok(info)
    }

    pub fn write(&self, id: &str, data: &[u8]) -> Result<(), String> {
        if data.len() > 1024 * 1024 {
            return Err("Paste is too large (1 MB maximum)".into());
        }
        let session = self
            .sessions
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or("Terminal is closed")?;
        let mut writer = session.writer.lock().unwrap();
        let writer = writer.as_mut().ok_or("Terminal is closed")?;
        writer
            .write_all(data)
            .and_then(|_| writer.flush())
            .map_err(|e| e.to_string())
    }

    pub fn acknowledge(&self, id: &str, generation: &str, bytes: usize) {
        if let Some(session) = self.sessions.lock().unwrap().get(id) {
            let mut output = session.output.lock().unwrap();
            if output.generation == generation {
                output.in_flight = output.in_flight.saturating_sub(bytes);
                session.flow.notify_all();
            }
        }
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(id).ok_or("Terminal is closed")?;
        let result = session
            .master
            .lock()
            .unwrap()
            .resize(PtySize {
                rows: rows.clamp(2, 1000),
                cols: cols.clamp(2, 1000),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string());
        result
    }

    pub fn info(&self, id: &str) -> Result<TerminalInfo, String> {
        self.sessions
            .lock()
            .unwrap()
            .get(id)
            .map(|s| s.info())
            .ok_or_else(|| "Terminal is closed".into())
    }
    pub fn close(&self, id: &str) {
        let session = { self.sessions.lock().unwrap().remove(id) };
        if let Some(session) = session { session.stop(); }
    }
    pub fn stop_all(&self) {
        let sessions: Vec<_> = self.sessions.lock().unwrap().drain().map(|(_, session)| session).collect();
        for session in sessions { session.stop(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn channel() -> (
        EventSink,
        Arc<Mutex<String>>,
        Arc<Mutex<Vec<serde_json::Value>>>,
    ) {
        let text = Arc::new(Mutex::new(String::new()));
        let events = Arc::new(Mutex::new(Vec::new()));
        let output = text.clone();
        let frames = events.clone();
        let channel = EventSink::new(move |body| {
            {
                let value: serde_json::Value = serde_json::to_value(body).unwrap();
                if let Some(data) = value.get("data").and_then(|value| value.as_str()) {
                    output
                        .lock()
                        .unwrap()
                        .push_str(&String::from_utf8_lossy(&STANDARD.decode(data).unwrap()));
                }
                frames.lock().unwrap().push(value);
            }
            Ok(())
        });
        (channel, text, events)
    }
    fn wait_for(output: &Arc<Mutex<String>>, expected: &str) {
        let start = Instant::now();
        loop {
            if output.lock().unwrap().contains(expected) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "PTY did not produce expected marker: {expected}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn real_pty_preserves_shell_state_resizes_interrupts_and_reattaches() {
        let manager = TerminalManager::default();
        let (on_event, output, _) = channel();
        let info = manager
            .start(
                "pty-integration".into(),
                "/tmp".into(),
                80,
                24,
                None,
                on_event,
            )
            .unwrap();
        manager
            .write(
                &info.id,
                b"printf '\\n__TTY__'; test -t 0 && printf 'yes\\n'; stty size\r",
            )
            .unwrap();
        wait_for(&output, "__TTY__yes");
        wait_for(&output, "24 80");
        manager.write(&info.id, b"cd /; export HERMES_UI_PTY_CHECK=persistent; printf '\\n__STATE__%s:%s\\n' \"$PWD\" \"$HERMES_UI_PTY_CHECK\"\r").unwrap();
        wait_for(&output, "__STATE__/:persistent");
        assert_eq!(manager.info(&info.id).unwrap().cwd, "/");
        manager.resize(&info.id, 101, 37).unwrap();
        manager.write(&info.id, b"stty size\r").unwrap();
        wait_for(&output, "37 101");
        // Attach a new renderer channel; same process, bounded output replay.
        let (on_event, replay, events) = channel();
        let reused = manager
            .start(info.id.clone(), "/tmp".into(), 80, 24, None, on_event)
            .unwrap();
        assert_eq!(reused.pid, info.pid);
        wait_for(&replay, "__STATE__/:persistent");
        assert_eq!(events.lock().unwrap()[0]["reused"], true);
        manager.write(&info.id, b"sleep 30\r").unwrap();
        std::thread::sleep(Duration::from_millis(100));
        manager.write(&info.id, b"\x03").unwrap();
        manager
            .write(&info.id, b"printf '\\n__INTERRUPTED__\\n'\r")
            .unwrap();
        wait_for(&replay, "\r\n__INTERRUPTED__\r\n");
        manager.close(&info.id);
        assert!(manager.info(&info.id).is_err());
        assert!(manager.write(&info.id, b"unused").is_err());
        let deadline = Instant::now() + Duration::from_secs(3);
        while std::path::Path::new(&format!("/proc/{}", info.pid.unwrap())).exists() {
            assert!(Instant::now() < deadline, "PTY child was not reaped");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod ssh_integration {
    use super::*;
    use std::{
        fs,
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };

    struct Fixture {
        dir: std::path::PathBuf,
        server: Child,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.server.kill();
            let _ = self.server.wait();
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    #[test]
    fn real_ssh_pty_and_git_use_only_the_configured_target() {
        let dir =
            std::env::temp_dir().join(format!("hermes-ssh-fixture-{}", rand::random::<u64>()));
        fs::create_dir(&dir).unwrap();
        for name in ["host_key", "client_key"] {
            assert!(Command::new("ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(dir.join(name))
                .status()
                .unwrap()
                .success());
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        fs::copy(dir.join("client_key.pub"), dir.join("authorized_keys")).unwrap();
        let host_key = fs::read_to_string(dir.join("host_key.pub")).unwrap();
        fs::write(
            dir.join("known_hosts"),
            format!("[127.0.0.1]:{port} {host_key}"),
        )
        .unwrap();
        let username = String::from_utf8(Command::new("id").arg("-un").output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_owned();
        fs::write(dir.join("sshd_config"), format!("Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nAllowUsers {username}\nLogLevel ERROR\n", dir.join("host_key").display(), dir.join("sshd.pid").display(), dir.join("authorized_keys").display())).unwrap();
        fs::write(dir.join("ssh_config"), format!("Host hermes-fixture\n  HostName 127.0.0.1\n  Port {port}\n  User {username}\n  IdentityFile {}\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  IdentitiesOnly yes\n", dir.join("client_key").display(), dir.join("known_hosts").display())).unwrap();
        let log = fs::File::create(dir.join("sshd.log")).unwrap();
        let server = Command::new("/usr/bin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(dir.join("sshd_config"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut fixture = Fixture {
            dir: dir.clone(),
            server,
        };
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            if fixture.server.try_wait().unwrap().is_some() {
                panic!(
                    "Isolated sshd exited: {}",
                    fs::read_to_string(dir.join("sshd.log")).unwrap()
                );
            }
            assert!(Instant::now() < deadline, "Isolated sshd did not listen");
            std::thread::sleep(Duration::from_millis(20));
        }
        let repo = dir.join("project's ü $(must-stay-literal)");
        fs::create_dir(&repo).unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(&repo)
            .status()
            .unwrap()
            .success());
        fs::write(repo.join("untracked file.txt"), "fixture").unwrap();
        let ssh = SshTarget {
            host: "hermes-fixture".into(),
            user: None,
            port: None,
            identity_file: None,
            config_file: Some(dir.join("ssh_config").to_string_lossy().into_owned()),
        };
        let manager = TerminalManager::default();
        let text = Arc::new(Mutex::new(String::new()));
        let output = text.clone();
        let channel = EventSink::new(move |body| {
            {
                let event: serde_json::Value = serde_json::to_value(body).unwrap();
                if let Some(data) = event["data"].as_str() {
                    output
                        .lock()
                        .unwrap()
                        .push_str(&String::from_utf8_lossy(&STANDARD.decode(data).unwrap()));
                }
            }
            Ok(())
        });
        let info = manager
            .start(
                "ssh-integration".into(),
                repo.to_string_lossy().into_owned(),
                90,
                30,
                Some(ssh.clone()),
                channel,
            )
            .unwrap();
        assert!(info.remote);
        let wait = |marker: &str| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !text.lock().unwrap().contains(marker) {
                assert!(
                    Instant::now() < deadline,
                    "SSH PTY missing marker {marker}: {}",
                    text.lock().unwrap()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        // Wait until SSH has authenticated and the remote shell consumes input.
        manager
            .write(
                &info.id,
                b"printf '\\n__REMOTE_CWD__%s\\n' \"$PWD\"; stty size\r",
            )
            .unwrap();
        wait(&format!("__REMOTE_CWD__{}", repo.display()));
        wait("30 90");
        manager.resize(&info.id, 110, 40).unwrap();
        manager.write(&info.id, b"stty size\r").unwrap();
        wait("40 110");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let status = runtime
            .block_on(crate::workspace::git_workspace_status(
                &repo.to_string_lossy(),
                Some(&ssh),
                crate::workspace::GitDiffScope::default(),
                None,
            ))
            .unwrap();
        assert!(status.is_repo);
        assert!(status
            .changed_files
            .iter()
            .any(|change| change.path == "untracked file.txt"));
        runtime.block_on(crate::workspace::git_workspace_action(
            &repo.to_string_lossy(),
            Some(&ssh),
            crate::workspace::GitWorkspaceAction::Stage { paths: vec!["untracked file.txt".into()] },
        )).unwrap();
        let staged = runtime.block_on(crate::workspace::git_workspace_status(
            &repo.to_string_lossy(),
            Some(&ssh),
            crate::workspace::GitDiffScope::Staged,
            None,
        )).unwrap();
        assert!(staged.diff.contains("untracked file.txt"));
        manager.close(&info.id);
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;
    use std::{
        sync::atomic::AtomicUsize,
        time::{Duration, Instant},
    };

    #[test]
    fn fast_output_pauses_at_the_window_and_resumes_after_matching_acknowledgements() {
        let manager = TerminalManager::default();
        let received = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(Mutex::new(String::new()));
        let done = Arc::new(AtomicBool::new(false));
        let count = received.clone();
        let stream = generation.clone();
        let completed = done.clone();
        let channel = EventSink::new(move |body| {
            {
                let event: serde_json::Value = serde_json::to_value(body).unwrap();
                if let Some(value) = event["generation"].as_str() {
                    *stream.lock().unwrap() = value.into();
                }
                if let Some(data) = event["data"].as_str() {
                    let bytes = STANDARD.decode(data).unwrap();
                    count.fetch_add(bytes.len(), Ordering::SeqCst);
                    if String::from_utf8_lossy(&bytes).contains("\r\n__FLOW_COMPLETE__\r\n") {
                        completed.store(true, Ordering::SeqCst);
                    }
                }
            }
            Ok(())
        });
        let info = manager
            .start("flow-control".into(), "/tmp".into(), 80, 24, None, channel)
            .unwrap();
        manager
            .write(
                &info.id,
                b"head -c 1500000 /dev/zero | tr '\\000' X; printf '\\n__FLOW_COMPLETE__\\n'\r",
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while received.load(Ordering::SeqCst) < OUTPUT_WINDOW {
            assert!(
                Instant::now() < deadline,
                "PTY did not fill the output window"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        let paused = received.load(Ordering::SeqCst);
        assert!(paused <= OUTPUT_WINDOW + 8192);
        // A stale renderer must not release the current renderer's window.
        manager.acknowledge(&info.id, "stale-generation", paused);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(received.load(Ordering::SeqCst), paused);
        let mut acknowledged = 0;
        while !done.load(Ordering::SeqCst) {
            let total = received.load(Ordering::SeqCst);
            manager.acknowledge(&info.id, &generation.lock().unwrap(), total - acknowledged);
            acknowledged = total;
            assert!(
                Instant::now() < deadline,
                "PTY did not resume after output acknowledgement"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(received.load(Ordering::SeqCst) >= 1_500_000);
        assert!(
            manager
                .sessions
                .lock()
                .unwrap()
                .get(&info.id)
                .unwrap()
                .output
                .lock()
                .unwrap()
                .buffer
                .len()
                <= BUFFER_BYTES
        );
        manager.close(&info.id);
    }
}
