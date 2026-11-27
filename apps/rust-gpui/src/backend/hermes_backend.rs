use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, Notify};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    pub status: String, // "starting" | "ready" | "error" | "stopped"
    pub url: Option<String>,
    pub token: Option<String>,
    pub pid: Option<u32>,
    pub hermes_home: String,
    pub binary_path: Option<String>,
    pub boot_ms: Option<u64>,
    pub error: Option<String>,
    pub logs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct NativeHttpRequest {
    pub url: String,
    pub method: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NativeHttpResponse {
    pub status: u16,
    pub body: String,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
}

struct BackendInner {
    child: Option<Child>,
    status: String,
    url: Option<String>,
    token: Option<String>,
    pid: Option<u32>,
    binary_path: Option<String>,
    boot_ms: Option<u64>,
    error: Option<String>,
    logs: VecDeque<String>,
}

#[derive(Clone)]
pub struct HermesSupervisor {
    inner: Arc<Mutex<BackendInner>>,
    boot_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    status_notify: Arc<Notify>,
    http: reqwest::Client,
    hermes_home: PathBuf,
}

impl HermesSupervisor {
    pub fn new() -> Self {
        Self::with_home(resolve_hermes_home())
    }

    fn with_home(hermes_home: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(8)
            .build()
            .expect("initialize Hermes HTTP client");

        Self {
            boot_task: Arc::new(Mutex::new(None)),
            status_notify: Arc::new(Notify::new()),
            inner: Arc::new(Mutex::new(BackendInner {
                child: None,
                status: "stopped".into(),
                url: None,
                token: None,
                pid: None,
                binary_path: None,
                boot_ms: None,
                error: None,
                logs: VecDeque::with_capacity(200),
            })),
            http,
            hermes_home,
        }
    }

    pub async fn wait_until_ready(&self, timeout: Duration) -> Result<BackendInfo, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let notified = self.status_notify.notified();
            let info = self.info().await;
            if info.status == "ready" {
                return Ok(info);
            }
            if info.status == "error" {
                return Err(info.error.unwrap_or_else(|| {
                    "Hermes could not start. Install Hermes Agent, or connect to a server.".into()
                }));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(
                    "Hermes took too long to start. Check the runtime in Settings, or connect to a server."
                        .into(),
                );
            }
            let _ = tokio::time::timeout(remaining.min(Duration::from_secs(2)), notified).await;
        }
    }

    pub fn hermes_home(&self) -> &PathBuf {
        &self.hermes_home
    }

    async fn push_log(&self, line: String) {
        let mut g = self.inner.lock().await;
        if g.logs.len() >= 180 {
            g.logs.pop_front();
        }
        g.logs.push_back(line);
    }

    pub async fn info(&self) -> BackendInfo {
        let g = self.inner.lock().await;
        BackendInfo {
            status: g.status.clone(),
            url: g.url.clone(),
            token: g.token.clone(),
            pid: g.pid,
            hermes_home: self.hermes_home.to_string_lossy().to_string(),
            binary_path: g.binary_path.clone(),
            boot_ms: g.boot_ms,
            error: g.error.clone(),
            logs: g.logs.iter().cloned().collect(),
        }
    }

    pub async fn stop(&self) {
        // Join the cancelled boot before killing its child, so a late startup
        // cannot publish a new process or mark a stopped backend ready.
        let mut task = self.boot_task.lock().await;
        if let Some(handle) = task.take() {
            handle.abort();
            let _ = handle.await;
        }
        let mut g = self.inner.lock().await;
        if let Some(mut child) = g.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        g.status = "stopped".into();
        g.pid = None;
        g.url = None;
        g.token = None;
        drop(g);
        self.status_notify.notify_waiters();
    }

    pub async fn start_in_background(&self) {
        let mut task = self.boot_task.lock().await;
        if task.as_ref().is_some_and(|handle| !handle.is_finished()) {
            return;
        }
        // Publish startup synchronously before returning to the UI. A retry
        // must not observe the previous boot's error while the new task queues.
        {
            let mut inner = self.inner.lock().await;
            inner.status = "starting".into();
            inner.error = None;
            inner.boot_ms = None;
        }
        let sup = self.clone();
        *task = Some(tokio::spawn(async move {
            if let Err(err) = sup.boot_local().await {
                sup.push_log(format!("[rust-supervisor] boot error: {err}"))
                    .await;
                let mut g = sup.inner.lock().await;
                if let Some(mut child) = g.child.take() {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
                g.pid = None;
                g.status = "error".into();
                g.error = Some(err);
                drop(g);
                sup.status_notify.notify_waiters();
                return;
            }
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let mut g = sup.inner.lock().await;
                if let Some(child) = g.child.as_mut() {
                    if let Ok(Some(exit)) = child.try_wait() {
                        g.child = None;
                        g.pid = None;
                        g.status = "error".into();
                        g.error = Some(format!("Hermes backend exited with status {exit}"));
                        drop(g);
                        sup.status_notify.notify_waiters();
                        return;
                    }
                }
            }
        }));
    }

    pub async fn restart(&self) -> BackendInfo {
        self.stop().await;
        {
            let mut g = self.inner.lock().await;
            g.status = "starting".into();
            g.error = None;
            g.boot_ms = None;
        }
        self.start_in_background().await;
        tokio::time::sleep(Duration::from_millis(120)).await;
        self.info().await
    }

    async fn boot_local(&self) -> Result<(), String> {
        let started = Instant::now();
        let (program, base_args) = resolve_hermes_command(&self.hermes_home)
            .ok_or_else(|| "Could not locate Hermes Agent binary (`hermes` or `~/.hermes/hermes-agent/venv/bin/python`).".to_string())?;

        let port = pick_free_port()?;
        let token = generate_session_token();
        let url = format!("http://127.0.0.1:{port}");

        {
            let mut g = self.inner.lock().await;
            g.status = "starting".into();
            g.error = None;
            g.url = Some(url.clone());
            g.token = Some(token.clone());
            g.binary_path = Some(program.clone());
        }

        let mut args = base_args;
        args.extend([
            "serve".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            port.to_string(),
        ]);

        self.push_log(format!(
            "[rust-supervisor] spawning {} {}",
            program,
            args.join(" ")
        ))
        .await;

        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .env("HERMES_HOME", &self.hermes_home)
            .env("HERMES_DASHBOARD_SESSION_TOKEN", &token)
            .env("PYTHONUNBUFFERED", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn Hermes backend ({program}): {e}"))?;

        let pid = child.id();

        if let Some(stdout) = child.stdout.take() {
            let sup = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    sup.push_log(format!("[stdout] {line}")).await;
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let sup = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    sup.push_log(format!("[stderr] {line}")).await;
                }
            });
        }

        {
            let mut g = self.inner.lock().await;
            g.child = Some(child);
            g.pid = pid;
        }

        // Poll /api/status up to 25 seconds (start fast at 40ms, ramp to 120ms)
        let deadline = Instant::now() + Duration::from_secs(25);
        let status_url = format!("{url}/api/status");
        let mut ready = false;
        let mut attempt = 0u32;

        while Instant::now() < deadline {
            // Check if child already exited prematurely
            {
                let mut g = self.inner.lock().await;
                if let Some(child) = g.child.as_mut() {
                    if let Ok(Some(exit)) = child.try_wait() {
                        return Err(format!("Hermes backend exited early with status {exit}"));
                    }
                }
            }

            if let Ok(resp) = self
                .http
                .get(&status_url)
                .header("X-Hermes-Session-Token", &token)
                .timeout(Duration::from_millis(800))
                .send()
                .await
            {
                if resp.status().is_success() || resp.status().as_u16() == 401 {
                    ready = true;
                    break;
                }
            }

            let delay_ms = match attempt {
                0..=5 => 40,
                6..=15 => 80,
                _ => 120,
            };
            attempt = attempt.saturating_add(1);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        if !ready {
            return Err(
                "Timed out waiting for local Hermes backend (`hermes serve`) to become ready."
                    .into(),
            );
        }

        // Adopt served token from index HTML if server regenerated one (mirrors Hermes Desktop dashboard-token.ts)
        let final_token = match self.resolve_served_token(&url).await {
            Some(served) if !served.is_empty() => served,
            _ => token,
        };

        let elapsed = started.elapsed().as_millis() as u64;
        self.push_log(format!("[rust-supervisor] ready at {url} in {elapsed}ms"))
            .await;

        let mut g = self.inner.lock().await;
        g.status = "ready".into();
        g.url = Some(url);
        g.token = Some(final_token);
        g.boot_ms = Some(elapsed);
        g.error = None;
        drop(g);
        self.status_notify.notify_waiters();
        Ok(())
    }

    async fn resolve_served_token(&self, base_url: &str) -> Option<String> {
        static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
        let resp = self
            .http
            .get(format!("{}/", base_url.trim_end_matches('/')))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .ok()?;
        let html = resp.text().await.ok()?;
        let re = TOKEN_RE.get_or_init(|| {
            Regex::new(r#"window\.__HERMES_SESSION_TOKEN__\s*=\s*("(?:\\.|[^"\\])*")"#)
                .expect("valid session token regex")
        });
        let caps = re.captures(&html)?;
        let json_str = caps.get(1)?.as_str();
        serde_json::from_str::<String>(json_str).ok()
    }

    pub async fn proxy_http(&self, req: NativeHttpRequest) -> Result<NativeHttpResponse, String> {
        let url = reqwest::Url::parse(&req.url).map_err(|_| "Invalid gateway address")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Gateway requests require an HTTP or HTTPS address without embedded credentials".into());
        }
        let method = reqwest::Method::from_bytes(req.method.to_uppercase().as_bytes())
            .map_err(|e| format!("Invalid HTTP method: {e}"))?;

        let mut builder = self.http.request(method, url);
        if let Some(headers) = req.headers {
            for (k, v) in headers {
                builder = builder.header(k, v);
            }
        }
        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let mut resp = builder
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
        let mut bytes = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("Failed reading response body: {e}"))? {
            if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
                return Err("Hermes response exceeded the 32 MB limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(bytes).unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());

        Ok(NativeHttpResponse {
            status,
            body,
            content_type,
        })
    }
}

pub fn resolve_hermes_home() -> PathBuf {
    if let Ok(env_home) = std::env::var("HERMES_HOME") {
        let trimmed = env_home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".hermes")
}

fn resolve_hermes_command(hermes_home: &PathBuf) -> Option<(String, Vec<String>)> {
    // 1. Managed venv python inside ~/.hermes/hermes-agent/venv/bin/python (fastest, zero shim overhead)
    let venv_py = hermes_home.join("hermes-agent/venv/bin/python");
    if venv_py.is_file() {
        return Some((
            venv_py.to_string_lossy().to_string(),
            vec!["-m".into(), "hermes_cli.main".into()],
        ));
    }

    // 2. ~/.local/bin/hermes
    if let Some(home) = dirs::home_dir() {
        let local_bin = home.join(".local/bin/hermes");
        if local_bin.is_file() {
            return Some((local_bin.to_string_lossy().to_string(), vec![]));
        }
    }

    // 3. System PATH `hermes`
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("hermes");
            if candidate.is_file() {
                return Some((candidate.to_string_lossy().to_string(), vec![]));
            }
        }
    }

    None
}

fn pick_free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Failed to bind ephemeral localhost port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed reading local port: {e}"))?
        .port();
    Ok(port)
}

fn generate_session_token() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
