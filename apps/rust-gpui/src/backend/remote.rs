//! Native browser sign-in and private, atomic connection storage.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, sync::Mutex, task::JoinHandle};

#[derive(Clone, Default)]
pub struct RemoteLogin {
    result: Arc<Mutex<Option<Result<Value, String>>>>,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn server_url(raw: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(raw).map_err(|_| "Enter a complete server URL, such as https://hermes.example.com".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("Use an http:// or https:// server URL without a password, query, or fragment".into());
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url)
}

fn auth_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none()).build().map_err(|e| e.to_string())
}

async fn token_post(url: String, body: Value) -> Result<Value, String> {
    let response = auth_client()?.post(url).json(&body).send().await
        .map_err(|_| "Could not reach the server to finish sign-in. Check its address and TLS certificate.".to_string())?;
    if !response.status().is_success() {
        return Err(if response.status().as_u16() == 401 { "Your sign-in has expired. Sign in again.".into() }
            else { format!("Server rejected sign-in (HTTP {}). Try signing in again.", response.status().as_u16()) });
    }
    let value: Value = response.json().await.map_err(|_| "Server returned an invalid sign-in response".to_string())?;
    if value.get("access_token").and_then(Value::as_str).filter(|v| !v.is_empty()).is_none() {
        return Err("Server did not return an access token".into());
    }
    Ok(value)
}

fn callback_code(target: &str, expected_state: &str) -> Result<Option<String>, String> {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| "Invalid sign-in callback".to_string())?;
    if url.path() != "/callback" { return Ok(None); }
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    if query.get("state").map(String::as_str) != Some(expected_state) { return Err("Sign-in state did not match. Please try again.".into()); }
    if query.contains_key("error") { return Err("Sign-in was declined. Please try again.".into()); }
    query.get("code").filter(|v| !v.is_empty()).cloned().map(Some).ok_or_else(|| "No sign-in code was returned".into())
}

impl RemoteLogin {
    pub async fn cancel(&self) {
        if let Some(task) = self.task.lock().await.take() { task.abort(); let _ = task.await; }
        *self.result.lock().await = None;
    }

    pub async fn begin(&self, raw_url: String, provider: String) -> Result<String, String> {
        self.begin_with_browser(raw_url, provider, true).await
    }

    async fn begin_with_browser(&self, raw_url: String, provider: String, open_browser: bool) -> Result<String, String> {
        self.cancel().await;
        let base = server_url(&raw_url)?.as_str().trim_end_matches('/').to_owned();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|_| "Could not open the local sign-in listener".to_string())?;
        let redirect = format!("http://127.0.0.1:{}/callback", listener.local_addr().map_err(|e| e.to_string())?.port());
        let verifier = random_secret();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = random_secret();
        let mut authorize = reqwest::Url::parse(&format!("{base}/auth/native/authorize")).map_err(|e| e.to_string())?;
        authorize.query_pairs_mut().extend_pairs([
            ("provider", provider.as_str()), ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"), ("redirect_uri", redirect.as_str()), ("state", state.as_str()),
        ]);
        let result = self.result.clone();
        let task = tokio::spawn(async move {
            let login = async {
                loop {
                    let (mut stream, _) = listener.accept().await.map_err(|_| "Sign-in listener closed".to_string())?;
                    let request = tokio::time::timeout(Duration::from_secs(3), async {
                        let mut request = Vec::new();
                        while request.len() < 8192 {
                            let mut bytes = [0u8; 1024];
                            let size = stream.read(&mut bytes).await.map_err(|e| e.to_string())?;
                            if size == 0 { break; }
                            request.extend_from_slice(&bytes[..size]);
                            if request.windows(4).any(|b| b == b"\r\n\r\n") { break; }
                        }
                        Ok::<_, String>(String::from_utf8_lossy(&request).into_owned())
                    }).await;
                    let Ok(Ok(request)) = request else { continue; };
                    let target = request.lines().next().unwrap_or("").strip_prefix("GET ").and_then(|line| line.split_whitespace().next()).unwrap_or("/");
                    let code = callback_code(target, &state);
                    let content = if matches!(code, Ok(Some(_))) { "Signed in to Hermes. You can close this tab and return to the app." } else { "Return to Hermes to continue signing in." };
                    let body = format!("<!doctype html><meta charset=utf-8><title>Hermes</title><body style='font:16px system-ui;text-align:center;padding:15vh 24px'><h1>Hermes</h1><p>{content}</p>");
                    let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await;
                    let _ = stream.shutdown().await;
                    if let Some(code) = code? {
                        return token_post(format!("{base}/auth/native/token"), json!({ "code": code, "code_verifier": verifier })).await;
                    }
                }
            };
            let value = tokio::time::timeout(Duration::from_secs(300), login).await
                .unwrap_or_else(|_| Err("Sign-in timed out. Please try again.".into()));
            *result.lock().await = Some(value);
        });
        *self.task.lock().await = Some(task);
        let url = authorize.to_string();
        if open_browser && tokio::process::Command::new("xdg-open").arg(&url)
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_err() {
            // The UI also offers this URL, so a missing browser handler is recoverable.
        }
        Ok(url)
    }

    pub async fn poll(&self) -> Result<Option<Value>, String> {
        match self.result.lock().await.take() { Some(value) => value.map(Some), None => Ok(None) }
    }
}

pub async fn refresh(raw_url: String, refresh_token: String, provider: String) -> Result<Value, String> {
    let base = server_url(&raw_url)?.as_str().trim_end_matches('/').to_owned();
    token_post(format!("{base}/auth/native/refresh"), json!({ "refresh_token": refresh_token, "provider": provider })).await
}

pub fn connection_path() -> Result<std::path::PathBuf, String> {
    Ok(dirs::config_dir().ok_or("Your configuration folder is unavailable")?.join("hermes-grok-bot/connection.json"))
}

pub fn read_connection(path: &Path) -> Result<Option<Value>, String> {
    if !path.exists() { return Ok(None); }
    let value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|_| "Saved connection could not be read".to_string())?;
    Ok(Some(value))
}

pub fn save_connection(path: &Path, value: &Value) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::{OpenOptionsExt, PermissionsExt}};
    server_url(value.get("url").and_then(Value::as_str).ok_or("Connection URL is missing")?)?;
    let parent = path.parent().ok_or("Configuration folder is unavailable")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".connection-{}.tmp", random_secret()));
    let outcome = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if outcome.is_err() { let _ = std::fs::remove_file(temp); }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_requires_state_and_code() {
        assert_eq!(callback_code("/favicon.ico", "state").unwrap(), None);
        assert_eq!(callback_code("/callback?code=one&state=state", "state").unwrap(), Some("one".into()));
        assert!(callback_code("/callback?code=one&state=other", "state").is_err());
        assert!(callback_code("/callback?state=state&error=denied", "state").is_err());
        assert!(callback_code("/callback?state=state", "state").is_err());
    }
    #[test]
    fn connection_storage_is_private_and_atomic() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("hermes-connection-test-{}", random_secret()));
        let path = dir.join("connection.json");
        let value = json!({"id":"remote", "url":"https://example.com/hermes", "token":"private-test"});
        save_connection(&path, &value).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(read_connection(&path).unwrap(), Some(value.clone()));
        assert!(save_connection(&path, &json!({"url":"file:///tmp/x"})).is_err());
        assert_eq!(read_connection(&path).unwrap(), Some(value));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn native_pkce_listener_redeems_code_and_can_be_cancelled() {
        let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", gateway.local_addr().unwrap());
        let login = RemoteLogin::default();
        let authorize = reqwest::Url::parse(&login.begin_with_browser(base.clone(), "basic".into(), false).await.unwrap()).unwrap();
        let query: std::collections::HashMap<_, _> = authorize.query_pairs().into_owned().collect();
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["provider"], "basic");
        let challenge = query["code_challenge"].clone();
        let exchange = tokio::spawn(async move {
            let (mut stream, _) = gateway.accept().await.unwrap();
            let mut data = Vec::new();
            loop {
                let mut bytes = [0; 2048];
                let n = stream.read(&mut bytes).await.unwrap();
                assert!(n > 0);
                data.extend_from_slice(&bytes[..n]);
                if let Some(header_end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&data[..header_end]);
                    assert!(headers.starts_with("POST /auth/native/token "));
                    let length: usize = headers.lines().find_map(|line| line.to_lowercase().strip_prefix("content-length: ").and_then(|v| v.parse().ok())).unwrap();
                    if data.len() < header_end + 4 + length { continue; }
                    let body: Value = serde_json::from_slice(&data[header_end+4..header_end+4+length]).unwrap();
                    assert_eq!(body["code"], "fixture-code");
                    assert_eq!(URL_SAFE_NO_PAD.encode(Sha256::digest(body["code_verifier"].as_str().unwrap().as_bytes())), challenge);
                    break;
                }
            }
            let body = r#"{"access_token":"fixture-access","refresh_token":"fixture-refresh","expires_at":2000000000,"provider":"basic"}"#;
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
        });
        let redirect = &query["redirect_uri"];
        let callback = format!("{redirect}?state={}&code=fixture-code", query["state"]);
        // A favicon request must not consume the pending sign-in.
        let origin = redirect.trim_end_matches("/callback");
        reqwest::get(format!("{origin}/favicon.ico")).await.unwrap();
        assert!(login.poll().await.unwrap().is_none());
        reqwest::get(callback).await.unwrap();
        exchange.await.unwrap();
        let tokens = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(value) = login.poll().await.unwrap() { break value; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        assert_eq!(tokens["access_token"], "fixture-access");
        login.cancel().await;
        let authorize = reqwest::Url::parse(&login.begin_with_browser(base, "basic".into(), false).await.unwrap()).unwrap();
        let redirect = authorize.query_pairs().find(|(key, _)| key == "redirect_uri").unwrap().1.into_owned();
        login.cancel().await;
        assert!(reqwest::get(redirect).await.is_err());
        assert!(login.poll().await.unwrap().is_none());
    }

}
