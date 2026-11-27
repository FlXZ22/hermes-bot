use crate::hermes::types::*;
use crate::workspace::WorkspaceFileEntry;
use reqwest::Method;
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Clone)]
pub struct HermesClient {
    conn: Arc<RwLock<HermesConnection>>,
    profile: Arc<RwLock<Option<String>>>,
    http: reqwest::Client,
}

impl HermesClient {
    pub fn new(mut connection: HermesConnection, profile: Option<String>) -> Self {
        connection.url = connection.url.trim().trim_end_matches('/').to_string();
        let prof = profile.or_else(|| connection.default_profile.clone());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(8)
            .build()
            .expect("create reqwest client");
        Self {
            conn: Arc::new(RwLock::new(connection)),
            profile: Arc::new(RwLock::new(prof)),
            http,
        }
    }

    pub fn connection(&self) -> HermesConnection {
        self.conn.read().unwrap().clone()
    }

    #[allow(dead_code)]
    pub fn set_credentials(
        &self,
        token: Option<String>,
        refresh_token: Option<String>,
        expires_at: Option<f64>,
        auth_provider: Option<String>,
    ) {
        let mut c = self.conn.write().unwrap();
        c.token = token;
        if refresh_token.is_some() {
            c.refresh_token = refresh_token;
        }
        if expires_at.is_some() {
            c.expires_at = expires_at;
        }
        if auth_provider.is_some() {
            c.auth_provider = auth_provider;
        }
    }

    pub fn set_profile(&self, profile: Option<String>) {
        *self.profile.write().unwrap() = profile;
    }

    pub fn profile(&self) -> Option<String> {
        self.profile.read().unwrap().clone()
    }

    fn build_url(&self, api_path: &str, query: &[(&str, String)]) -> String {
        let conn = self.conn.read().unwrap();
        let base = conn.url.trim_end_matches('/');
        let clean = if api_path.starts_with('/') {
            api_path.to_string()
        } else {
            format!("/{api_path}")
        };
        let mut full = format!("{base}{clean}");
        let mut params: Vec<(String, String)> = query
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        let has_profile = params.iter().any(|(k, _)| k == "profile");
        if !has_profile && clean.starts_with("/api/") {
            if let Some(p) = self.profile.read().unwrap().as_ref() {
                if !p.is_empty() {
                    params.push(("profile".into(), p.clone()));
                }
            }
        }
        if !params.is_empty() {
            let mut ser = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in params {
                ser.append_pair(&k, &v);
            }
            full.push('?');
            full.push_str(&ser.finish());
        }
        full
    }

    pub async fn request_json(
        &self,
        method: Method,
        api_path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let url = self.build_url(api_path, query);
        let conn = self.conn.read().unwrap().clone();
        let mut req = self.http.request(method, &url).header("Accept", "application/json");
        if let Some(tok) = conn.token.as_deref().filter(|t| !t.is_empty()) {
            match conn.auth_type {
                HermesAuthType::SessionToken => {
                    req = req.header("X-Hermes-Session-Token", tok);
                }
                HermesAuthType::Bearer => {
                    req = req.header("Authorization", format!("Bearer {tok}"));
                }
                _ if conn.mode == HermesServerMode::Openai => {
                    req = req.header("Authorization", format!("Bearer {tok}"));
                }
                _ => {}
            }
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("Network error reaching Hermes: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()));
        if !status.is_success() {
            let msg = parsed
                .get("detail")
                .and_then(|d| {
                    d.as_str().map(String::from).or_else(|| Some(d.to_string()))
                })
                .or_else(|| parsed.get("error").and_then(Value::as_str).map(String::from))
                .unwrap_or_else(|| format!("Request failed (HTTP {})", status.as_u16()));
            return Err(msg);
        }
        Ok(parsed)
    }

    pub async fn test_connection(&self) -> Result<(HermesStatus, Vec<HermesProfile>), String> {
        let (status_val, profiles_val) = tokio::try_join!(
            self.request_json(Method::GET, "/api/status", &[], None),
            self.request_json(Method::GET, "/api/profiles", &[], None),
        )?;
        if !status_val.is_object() {
            return Err("This address did not return the Hermes dashboard API. Check the server URL.".into());
        }
        let status = serde_json::from_value(status_val).unwrap_or_default();
        let profiles = extract_vec(&profiles_val, &["profiles", "items"]);
        Ok((status, profiles))
    }

    pub async fn get_auth_providers(&self) -> Result<Vec<AuthProviderInfo>, String> {
        let val = self
            .request_json(Method::GET, "/api/auth/providers", &[], None)
            .await?;
        let arr = val
            .get("providers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    pub async fn mint_ws_ticket(&self) -> Option<String> {
        let res = self
            .request_json(Method::POST, "/api/auth/ws-ticket", &[], None)
            .await
            .ok()?;
        res.get("ticket").and_then(Value::as_str).map(String::from)
    }

    pub async fn get_status(&self) -> Result<HermesStatus, String> {
        let v = self.request_json(Method::GET, "/api/status", &[], None).await?;
        Ok(serde_json::from_value(v).unwrap_or_default())
    }

    pub async fn gateway_action(&self, action: &str) -> Result<Value, String> {
        self.request_json(Method::POST, &format!("/api/gateway/{action}"), &[], None)
            .await
    }

    pub async fn rescan_dashboard_plugins(&self) -> Result<Value, String> {
        self.request_json(Method::GET, "/api/dashboard/plugins/rescan", &[], None)
            .await
    }

    pub async fn list_profiles(&self) -> Result<Vec<HermesProfile>, String> {
        let v = self.request_json(Method::GET, "/api/profiles", &[], None).await?;
        Ok(extract_vec(&v, &["profiles", "items"]))
    }

    pub async fn set_active_profile(&self, name: &str) -> Result<Value, String> {
        self.request_json(
            Method::POST,
            "/api/profiles/active",
            &[],
            Some(json!({ "name": name })),
        )
        .await
    }

    pub async fn create_profile(
        &self,
        name: &str,
        description: Option<&str>,
    ) -> Result<Value, String> {
        let mut payload = json!({ "name": name });
        if let Some(d) = description.filter(|s| !s.is_empty()) {
            payload["description"] = json!(d);
        }
        self.request_json(Method::POST, "/api/profiles", &[], Some(payload))
            .await
    }

    pub async fn get_profile_soul(&self, name: &str) -> Result<String, String> {
        let enc = urlencoding_segment(name);
        let v = self
            .request_json(Method::GET, &format!("/api/profiles/{enc}/soul"), &[], None)
            .await?;
        Ok(v.as_str()
            .map(String::from)
            .or_else(|| v.get("content").and_then(Value::as_str).map(String::from))
            .or_else(|| v.get("soul").and_then(Value::as_str).map(String::from))
            .unwrap_or_default())
    }

    pub async fn save_profile_soul(&self, name: &str, content: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(name);
        self.request_json(
            Method::PUT,
            &format!("/api/profiles/{enc}/soul"),
            &[],
            Some(json!({ "content": content })),
        )
        .await
    }

    pub async fn list_sessions(&self, limit: usize) -> Result<Vec<HermesSession>, String> {
        let mut out = Vec::new();
        while out.len() < limit {
            let page_size = (limit - out.len()).min(100);
            let v = self
                .request_json(
                    Method::GET,
                    "/api/sessions",
                    &[
                        ("limit", page_size.to_string()),
                        ("offset", out.len().to_string()),
                        ("order", "recent".into()),
                        ("archived", "include".into()),
                    ],
                    None,
                )
                .await?;
            let page: Vec<HermesSession> = extract_vec(&v, &["sessions", "items"]);
            let count = page.len();
            out.extend(page);
            if count < page_size {
                break;
            }
        }
        Ok(out)
    }

    pub async fn get_session_messages(
        &self,
        session_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<HermesMessage>, String> {
        let enc = urlencoding_segment(session_id);
        let v = self
            .request_json(
                Method::GET,
                &format!("/api/sessions/{enc}/messages"),
                &[
                    ("limit", limit.to_string()),
                    ("offset", offset.to_string()),
                    ("order", "latest".into()),
                ],
                None,
            )
            .await?;
        let raw: Vec<Value> = extract_vec(&v, &["messages", "items"]);
        Ok(raw
            .into_iter()
            .enumerate()
            .map(|(idx, m)| {
                let id = m
                    .get("id")
                    .and_then(|x| x.as_str().map(String::from).or_else(|| Some(x.to_string())))
                    .unwrap_or_else(|| format!("{session_id}-{offset}-{idx}"));
                let role = match m.get("role").and_then(Value::as_str).unwrap_or("assistant") {
                    "user" => MessageRole::User,
                    "system" => MessageRole::System,
                    "tool" => MessageRole::Tool,
                    _ => MessageRole::Assistant,
                };
                let content = match m.get("content") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => other.to_string(),
                };
                let (thinking, content) = if role == MessageRole::Assistant {
                    let fields = thinking_from_fields(&m);
                    extract_thinking_from_content(&content, fields.as_deref(), false)
                } else {
                    (None, content)
                };
                let tool_name = m.get("name").and_then(Value::as_str).map(String::from);
                let timestamp = m
                    .get("timestamp")
                    .or_else(|| m.get("created_at"))
                    .and_then(parse_timestamp_secs);
                let model = m.get("model").and_then(Value::as_str).map(String::from);
                let tool_calls = m
                    .get("toolCalls")
                    .or_else(|| m.get("tool_calls"))
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .enumerate()
                            .map(|(ti, t)| {
                                let fn_obj = t.get("function").filter(|v| v.is_object()).unwrap_or(t);
                                HermesToolCall {
                                    id: t
                                        .get("id")
                                        .and_then(Value::as_str)
                                        .map(String::from)
                                        .unwrap_or_else(|| format!("{id}-tool-{ti}")),
                                    name: t
                                        .get("name")
                                        .or_else(|| fn_obj.get("name"))
                                        .and_then(Value::as_str)
                                        .unwrap_or("tool")
                                        .to_string(),
                                    arguments: fn_obj.get("arguments").cloned(),
                                    output: t.get("output").and_then(Value::as_str).map(String::from),
                                    status: if t.get("status").and_then(Value::as_str) == Some("error")
                                    {
                                        ToolCallStatus::Error
                                    } else {
                                        ToolCallStatus::Done
                                    },
                                    duration_ms: t.get("durationMs").and_then(Value::as_u64),
                                    preview: None,
                                    inline_diff: t.get("inline_diff").and_then(Value::as_str).map(String::from),
                                    waiting_approval: false,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                HermesMessage {
                    id,
                    role,
                    content,
                    thinking,
                    tool_name,
                    timestamp,
                    model,
                    tool_calls,
                    approval: None,
                    clarify: None,
                    streaming: false,
                }
            })
            .collect())
    }

    pub async fn rename_session(&self, session_id: &str, title: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(session_id);
        self.request_json(
            Method::PATCH,
            &format!("/api/sessions/{enc}"),
            &[],
            Some(json!({ "title": title, "profile": self.profile() })),
        )
        .await
    }

    pub async fn pin_session(&self, session_id: &str, pinned: bool) -> Result<Value, String> {
        let enc = urlencoding_segment(session_id);
        self.request_json(
            Method::PATCH,
            &format!("/api/sessions/{enc}"),
            &[],
            Some(json!({ "pinned": pinned, "profile": self.profile() })),
        )
        .await
    }

    pub async fn archive_session(&self, session_id: &str, archived: bool) -> Result<Value, String> {
        let enc = urlencoding_segment(session_id);
        self.request_json(
            Method::PATCH,
            &format!("/api/sessions/{enc}"),
            &[],
            Some(json!({ "archived": archived, "profile": self.profile() })),
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn delete_session(&self, session_id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(session_id);
        self.request_json(
            Method::DELETE,
            &format!("/api/sessions/{enc}"),
            &[],
            Some(json!({ "profile": self.profile() })),
        )
        .await
    }

    pub async fn get_model_info(&self) -> Result<(Option<String>, Option<String>), String> {
        let v = self
            .request_json(Method::GET, "/api/model/info", &[], None)
            .await?;
        Ok((
            v.get("model").and_then(Value::as_str).map(String::from),
            v.get("provider").and_then(Value::as_str).map(String::from),
        ))
    }

    pub async fn get_model_options(&self, refresh: bool) -> Result<Vec<HermesModelOption>, String> {
        let mut q = vec![("include_unconfigured", "1".to_string())];
        if refresh {
            q.push(("refresh", "1".to_string()));
        }
        let v = self
            .request_json(Method::GET, "/api/model/options", &q, None)
            .await?;
        if let Some(arr) = v.as_array() {
            return Ok(arr
                .iter()
                .filter_map(|x| serde_json::from_value(x.clone()).ok())
                .collect());
        }
        if let Some(arr) = v.get("models").or_else(|| v.get("options")).and_then(Value::as_array) {
            return Ok(arr
                .iter()
                .filter_map(|x| serde_json::from_value(x.clone()).ok())
                .collect());
        }
        if let Some(providers) = v.get("providers").and_then(Value::as_array) {
            let mut flat = Vec::new();
            for p in providers {
                let prov_id = p
                    .get("slug")
                    .or_else(|| p.get("id"))
                    .or_else(|| p.get("provider"))
                    .or_else(|| p.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("provider")
                    .to_string();
                let p_auth = p
                    .get("authenticated")
                    .or_else(|| p.get("configured"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(models) = p.get("models").and_then(Value::as_array) {
                    for m in models {
                        if let Some(id) = m.as_str() {
                            flat.push(HermesModelOption {
                                id: id.to_string(),
                                name: Some(id.to_string()),
                                provider: prov_id.clone(),
                                configured: p_auth,
                                context_length: None,
                            });
                        } else if m.is_object() {
                            let id = m
                                .get("id")
                                .or_else(|| m.get("model"))
                                .or_else(|| m.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            if id.is_empty() {
                                continue;
                            }
                            flat.push(HermesModelOption {
                                name: m
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .map(String::from)
                                    .or_else(|| Some(id.clone())),
                                id,
                                provider: prov_id.clone(),
                                configured: m
                                    .get("configured")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(p_auth),
                                context_length: m.get("context_length").and_then(Value::as_u64),
                            });
                        }
                    }
                }
            }
            return Ok(flat);
        }
        Ok(vec![])
    }

    pub async fn set_default_model(&self, model: &str, provider: Option<&str>) -> Result<Value, String> {
        let mut body = json!({ "model": model });
        if let Some(p) = provider.filter(|p| !p.is_empty() && *p != "auto") {
            body["provider"] = json!(p);
        }
        self.request_json(Method::POST, "/api/model/set", &[], Some(body))
            .await
    }

    // Skills, Toolsets & MCP
    pub async fn list_skills(&self) -> Result<Vec<HermesSkill>, String> {
        let v = self.request_json(Method::GET, "/api/skills", &[], None).await?;
        Ok(extract_vec(&v, &["skills", "items"]))
    }

    pub async fn toggle_skill(&self, name: &str, enabled: bool) -> Result<Value, String> {
        self.request_json(
            Method::PUT,
            "/api/skills/toggle",
            &[],
            Some(json!({ "name": name, "enabled": enabled })),
        )
        .await
    }

    pub async fn get_skill_content(&self, name: &str) -> Result<String, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/skills/content",
                &[("name", name.to_string())],
                None,
            )
            .await?;
        Ok(v.as_str()
            .map(String::from)
            .or_else(|| v.get("content").and_then(Value::as_str).map(String::from))
            .unwrap_or_default())
    }

    pub async fn save_skill_content(&self, name: &str, content: &str) -> Result<Value, String> {
        self.request_json(
            Method::PUT,
            "/api/skills/content",
            &[],
            Some(json!({ "name": name, "content": content })),
        )
        .await
    }

    pub async fn search_skills_hub(&self, query: &str) -> Result<Vec<Value>, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/skills/hub/search",
                &[("q", query.to_string())],
                None,
            )
            .await?;
        Ok(extract_vec(&v, &["results", "skills", "items"]))
    }

    pub async fn install_hub_skill(&self, identifier: &str) -> Result<Value, String> {
        self.request_json(
            Method::POST,
            "/api/skills/hub/install",
            &[],
            Some(json!({ "identifier": identifier, "name": identifier })),
        )
        .await
    }

    pub async fn list_toolsets(&self) -> Result<Vec<HermesToolset>, String> {
        let v = self
            .request_json(Method::GET, "/api/tools/toolsets", &[], None)
            .await?;
        Ok(extract_vec(&v, &["toolsets", "items"]))
    }

    pub async fn toggle_toolset(&self, name: &str, enabled: bool) -> Result<Value, String> {
        let enc = urlencoding_segment(name);
        self.request_json(
            Method::PUT,
            &format!("/api/tools/toolsets/{enc}"),
            &[],
            Some(json!({ "enabled": enabled })),
        )
        .await
    }

    pub async fn list_mcp_servers(&self) -> Result<Vec<HermesMcpServer>, String> {
        let v = self
            .request_json(Method::GET, "/api/mcp/servers", &[], None)
            .await?;
        if v.is_array() {
            return Ok(extract_vec(&v, &[]));
        }
        if let Some(arr) = v.get("servers").and_then(Value::as_array) {
            return Ok(arr
                .iter()
                .filter_map(|x| serde_json::from_value(x.clone()).ok())
                .collect());
        }
        if let Some(map) = v.get("servers").and_then(Value::as_object) {
            return Ok(map
                .iter()
                .map(|(name, cfg)| {
                    let mut s: HermesMcpServer =
                        serde_json::from_value(cfg.clone()).unwrap_or_default();
                    s.name = name.clone();
                    s
                })
                .collect());
        }
        Ok(vec![])
    }

    pub async fn add_mcp_server(&self, server: &HermesMcpServer) -> Result<Value, String> {
        self.request_json(
            Method::POST,
            "/api/mcp/servers",
            &[],
            Some(serde_json::to_value(server).unwrap_or_default()),
        )
        .await
    }

    pub async fn delete_mcp_server(&self, name: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(name);
        self.request_json(Method::DELETE, &format!("/api/mcp/servers/{enc}"), &[], None)
            .await
    }

    pub async fn test_mcp_server(&self, name: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(name);
        self.request_json(Method::POST, &format!("/api/mcp/servers/{enc}/test"), &[], None)
            .await
    }

    pub async fn toggle_mcp_server(&self, name: &str, enabled: bool) -> Result<Value, String> {
        let enc = urlencoding_segment(name);
        self.request_json(
            Method::PUT,
            &format!("/api/mcp/servers/{enc}/enabled"),
            &[],
            Some(json!({ "enabled": enabled })),
        )
        .await
    }

    // Cron
    pub async fn list_cron_jobs(&self) -> Result<Vec<HermesCronJob>, String> {
        let v = self.request_json(Method::GET, "/api/cron/jobs", &[], None).await?;
        let raw: Vec<Value> = extract_vec(&v, &["jobs", "items"]);
        Ok(raw
            .into_iter()
            .map(|job| {
                let sched_val = job.get("schedule");
                let schedule = job
                    .get("schedule_display")
                    .and_then(Value::as_str)
                    .or_else(|| sched_val.and_then(|s| s.get("display")).and_then(Value::as_str))
                    .or_else(|| sched_val.and_then(|s| s.get("expr")).and_then(Value::as_str))
                    .or_else(|| sched_val.and_then(|s| s.get("value")).and_then(Value::as_str))
                    .or_else(|| sched_val.and_then(Value::as_str))
                    .unwrap_or("")
                    .to_string();
                let paused = job.get("paused").and_then(Value::as_bool).unwrap_or(false)
                    || job.get("enabled").and_then(Value::as_bool) == Some(false);
                HermesCronJob {
                    id: job
                        .get("id")
                        .and_then(|v| v.as_str().map(String::from).or_else(|| Some(v.to_string())))
                        .unwrap_or_default(),
                    name: job
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Scheduled job")
                        .to_string(),
                    schedule,
                    prompt: job
                        .get("prompt")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    enabled: Some(!paused),
                    paused,
                    profile: job.get("profile").and_then(Value::as_str).map(String::from),
                    last_run: job.get("last_run").and_then(Value::as_str).map(String::from),
                    next_run: job.get("next_run").and_then(Value::as_str).map(String::from),
                    status: job.get("status").and_then(Value::as_str).map(String::from),
                }
            })
            .collect())
    }

    pub async fn create_cron_job(
        &self,
        name: &str,
        schedule: &str,
        prompt: &str,
        profile: &str,
    ) -> Result<Value, String> {
        self.request_json(
            Method::POST,
            "/api/cron/jobs",
            &[],
            Some(json!({
                "name": name,
                "schedule": schedule,
                "prompt": prompt,
                "profile": profile,
                "enabled": true,
            })),
        )
        .await
    }

    pub async fn delete_cron_job(&self, id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(id);
        self.request_json(Method::DELETE, &format!("/api/cron/jobs/{enc}"), &[], None)
            .await
    }

    pub async fn pause_cron_job(&self, id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(id);
        self.request_json(Method::POST, &format!("/api/cron/jobs/{enc}/pause"), &[], None)
            .await
    }

    pub async fn resume_cron_job(&self, id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(id);
        self.request_json(Method::POST, &format!("/api/cron/jobs/{enc}/resume"), &[], None)
            .await
    }

    pub async fn trigger_cron_job(&self, id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(id);
        self.request_json(Method::POST, &format!("/api/cron/jobs/{enc}/trigger"), &[], None)
            .await
    }

    // Memory / Curator / Ops
    pub async fn run_curator(&self) -> Result<Value, String> {
        self.request_json(Method::POST, "/api/curator/run", &[], None).await
    }

    pub async fn get_learning_graph(&self) -> Result<HermesLearningGraph, String> {
        let v = self
            .request_json(Method::GET, "/api/learning/graph", &[], None)
            .await?;
        Ok(HermesLearningGraph {
            nodes: extract_vec(&v, &["nodes", "items"]),
            edges: extract_vec(&v, &["edges", "links"]),
        })
    }

    pub async fn run_doctor(&self) -> Result<Value, String> {
        self.request_json(Method::POST, "/api/ops/doctor", &[], None).await
    }

    pub async fn run_security_audit(&self) -> Result<Value, String> {
        self.request_json(Method::POST, "/api/ops/security-audit", &[], None)
            .await
    }

    // Messaging & Webhooks
    pub async fn list_messaging_platforms(&self) -> Result<Vec<HermesMessagingPlatform>, String> {
        let v = self
            .request_json(Method::GET, "/api/messaging/platforms", &[], None)
            .await?;
        Ok(extract_vec(&v, &["platforms", "items"]))
    }

    pub async fn test_messaging_platform(&self, id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(id);
        self.request_json(
            Method::POST,
            &format!("/api/messaging/platforms/{enc}/test"),
            &[],
            None,
        )
        .await
    }

    pub async fn list_webhooks(&self) -> Result<Vec<HermesWebhook>, String> {
        let v = self.request_json(Method::GET, "/api/webhooks", &[], None).await?;
        Ok(extract_vec(&v, &["webhooks", "items"]))
    }

    pub async fn create_webhook(&self, name: &str, url: &str) -> Result<Value, String> {
        self.request_json(
            Method::POST,
            "/api/webhooks",
            &[],
            Some(json!({ "name": name, "url": url, "enabled": true })),
        )
        .await
    }

    pub async fn delete_webhook(&self, id: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(id);
        self.request_json(Method::DELETE, &format!("/api/webhooks/{enc}"), &[], None)
            .await
    }

    // Remote files
    pub async fn list_files(&self, dir_path: &str) -> Result<Vec<WorkspaceFileEntry>, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/files",
                &[("path", dir_path.to_string())],
                None,
            )
            .await?;
        let raw: Vec<Value> = extract_vec(&v, &["entries", "files", "items"]);
        Ok(raw
            .into_iter()
            .map(|e| WorkspaceFileEntry {
                name: e.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                path: e.get("path").and_then(Value::as_str).unwrap_or("").to_string(),
                is_dir: e
                    .get("is_directory")
                    .or_else(|| e.get("is_dir"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                size: e.get("size").and_then(Value::as_u64).unwrap_or(0),
                modified: e.get("modified").and_then(Value::as_u64).unwrap_or(0),
            })
            .collect())
    }

    pub async fn read_file(&self, file_path: &str) -> Result<String, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/files/read",
                &[("path", file_path.to_string())],
                None,
            )
            .await?;
        if let Some(s) = v.as_str() {
            return Ok(s.to_string());
        }
        if let Some(c) = v.get("content").and_then(Value::as_str) {
            return Ok(c.to_string());
        }
        Ok(String::new())
    }

    // Config, Env, Logs, Analytics
    pub async fn get_raw_config(&self) -> Result<String, String> {
        let v = self
            .request_json(Method::GET, "/api/config/raw", &[], None)
            .await?;
        Ok(v.as_str()
            .map(String::from)
            .or_else(|| v.get("content").and_then(Value::as_str).map(String::from))
            .or_else(|| v.get("yaml").and_then(Value::as_str).map(String::from))
            .unwrap_or_default())
    }

    pub async fn save_raw_config(&self, content: &str) -> Result<Value, String> {
        self.request_json(
            Method::PUT,
            "/api/config/raw",
            &[],
            Some(json!({ "content": content })),
        )
        .await
    }

    pub async fn list_env(&self) -> Result<Vec<HermesEnvEntry>, String> {
        let v = self.request_json(Method::GET, "/api/env", &[], None).await?;
        if v.is_array() {
            return Ok(extract_vec(&v, &[]));
        }
        if v.get("vars").and_then(Value::as_array).is_some()
            || v.get("entries").and_then(Value::as_array).is_some()
        {
            return Ok(extract_vec(&v, &["vars", "entries"]));
        }
        if let Some(map) = v.get("env").and_then(Value::as_object) {
            return Ok(map
                .iter()
                .map(|(k, val)| HermesEnvEntry {
                    key: k.clone(),
                    masked: val.as_str().map(String::from),
                    is_set: Some(!val.is_null()),
                })
                .collect());
        }
        Ok(vec![])
    }

    pub async fn set_env(&self, key: &str, value: &str) -> Result<Value, String> {
        self.request_json(
            Method::PUT,
            "/api/env",
            &[],
            Some(json!({ "key": key, "value": value })),
        )
        .await
    }

    pub async fn delete_env(&self, key: &str) -> Result<Value, String> {
        self.request_json(
            Method::DELETE,
            "/api/env",
            &[("key", key.to_string())],
            Some(json!({ "key": key })),
        )
        .await
    }

    pub async fn get_logs(&self, lines: usize) -> Result<Vec<HermesLogEntry>, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/logs",
                &[("lines", lines.to_string())],
                None,
            )
            .await?;
        if let Some(s) = v.as_str() {
            return Ok(s
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| HermesLogEntry {
                    message: l.to_string(),
                    ..Default::default()
                })
                .collect());
        }
        let arr: Vec<Value> = extract_vec(&v, &["logs", "lines", "entries"]);
        Ok(arr
            .into_iter()
            .map(|item| {
                if let Some(s) = item.as_str() {
                    HermesLogEntry {
                        message: s.to_string(),
                        ..Default::default()
                    }
                } else {
                    serde_json::from_value(item).unwrap_or_default()
                }
            })
            .collect())
    }

    pub async fn get_usage_analytics(
        &self,
        days: usize,
        profile: &str,
    ) -> Result<HermesUsageReport, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/analytics/usage",
                &[("days", days.to_string()), ("profile", profile.to_owned())],
                None,
            )
            .await?;
        parse_usage_report(v)
    }

    // Kanban Boards
    pub async fn list_kanban_boards(&self) -> Result<Vec<HermesBoardSummary>, String> {
        let v = self
            .request_json(Method::GET, "/api/plugins/kanban/boards", &[], None)
            .await?;
        let raw: Vec<Value> = extract_vec(&v, &["boards", "items"]);
        Ok(raw
            .into_iter()
            .map(|b| HermesBoardSummary {
                slug: b.get("slug").and_then(Value::as_str).unwrap_or("default").to_string(),
                name: b.get("name").and_then(Value::as_str).unwrap_or("Board").to_string(),
                description: b.get("description").and_then(Value::as_str).map(String::from),
                color: b.get("color").and_then(Value::as_str).map(String::from),
                workspace_path: b
                    .get("default_workdir")
                    .or_else(|| b.get("workspace_path"))
                    .and_then(Value::as_str)
                    .map(String::from),
                task_count: b
                    .get("total")
                    .or_else(|| b.get("task_count"))
                    .and_then(Value::as_u64),
            })
            .collect())
    }

    pub async fn create_kanban_board(&self, board: &HermesBoardSummary) -> Result<Value, String> {
        self.request_json(
            Method::POST,
            "/api/plugins/kanban/boards",
            &[],
            Some(json!({
                "name": board.name,
                "slug": board.slug,
                "description": board.description,
                "color": board.color,
                "default_workdir": board.workspace_path.as_deref().filter(|s| !s.is_empty()),
            })),
        )
        .await
    }

    pub async fn delete_kanban_board(&self, slug: &str) -> Result<Value, String> {
        let enc = urlencoding_segment(slug);
        self.request_json(
            Method::DELETE,
            &format!("/api/plugins/kanban/boards/{enc}"),
            &[],
            None,
        )
        .await
    }

    pub async fn get_kanban_board(&self, board_slug: &str) -> Result<Vec<HermesKanbanTask>, String> {
        let v = self
            .request_json(
                Method::GET,
                "/api/plugins/kanban/board",
                &[("board", board_slug.to_string())],
                None,
            )
            .await?;
        let mut out = Vec::new();
        let normalize = |item: &Value, col_status: Option<&str>| -> HermesKanbanTask {
            let status_str = item
                .get("status")
                .and_then(Value::as_str)
                .or(col_status)
                .unwrap_or("todo");
            HermesKanbanTask {
                id: item
                    .get("id")
                    .and_then(|x| x.as_str().map(String::from).or_else(|| Some(x.to_string())))
                    .unwrap_or_default(),
                board: Some(
                    item.get("board")
                        .and_then(Value::as_str)
                        .unwrap_or(board_slug)
                        .to_string(),
                ),
                title: item
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("Untitled Task")
                    .to_string(),
                body: item.get("body").and_then(Value::as_str).map(String::from),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .map(String::from),
                status: HermesKanbanStatus::from_str_lossy(status_str),
                priority: item.get("priority").cloned(),
                assignee: item.get("assignee").and_then(Value::as_str).map(String::from),
                model: item
                    .get("model_override")
                    .or_else(|| item.get("model"))
                    .and_then(Value::as_str)
                    .map(String::from),
                provider: item
                    .get("provider_override")
                    .or_else(|| item.get("provider"))
                    .and_then(Value::as_str)
                    .map(String::from),
                cwd: item
                    .get("workspace_path")
                    .or_else(|| item.get("cwd"))
                    .and_then(Value::as_str)
                    .map(String::from),
                session_id: item.get("session_id").and_then(Value::as_str).map(String::from),
            }
        };

        if let Some(arr) = v.get("tasks").and_then(Value::as_array) {
            for item in arr {
                out.push(normalize(item, None));
            }
        } else if let Some(cols) = v.get("columns").and_then(Value::as_array) {
            for col in cols {
                let cname = col.get("name").and_then(Value::as_str);
                if let Some(tasks) = col.get("tasks").and_then(Value::as_array) {
                    for item in tasks {
                        out.push(normalize(item, cname));
                    }
                }
            }
        } else if let Some(cols) = v.get("columns").and_then(Value::as_object) {
            for (status_key, list) in cols {
                if let Some(tasks) = list.as_array() {
                    for item in tasks {
                        out.push(normalize(item, Some(status_key)));
                    }
                }
            }
        }
        Ok(out)
    }

    pub async fn create_kanban_task(
        &self,
        task: &HermesKanbanTask,
        board_slug: &str,
    ) -> Result<Value, String> {
        let mut res = self
            .request_json(
                Method::POST,
                "/api/plugins/kanban/tasks",
                &[("board", board_slug.to_string())],
                Some(json!({
                    "title": task.title,
                    "body": task.body,
                    "priority": task.priority_num(),
                    "assignee": task.assignee,
                    "model_override": task.model.as_deref().filter(|s| !s.is_empty()),
                    "provider_override": task.provider.as_deref().filter(|s| !s.is_empty()),
                    "workspace_kind": if task.cwd.as_deref().is_some_and(|c| !c.is_empty()) { "dir" } else { "scratch" },
                    "workspace_path": task.cwd.as_deref().filter(|s| !s.is_empty()),
                    "triage": task.status == HermesKanbanStatus::Triage,
                })),
            )
            .await?;
        if let Some(created_id) = res
            .get("task")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
        {
            let created_status = res
                .get("task")
                .and_then(|t| t.get("status"))
                .and_then(Value::as_str);
            if created_status != Some(task.status.as_str()) {
                if let Ok(updated) = self
                    .update_kanban_task(
                        created_id,
                        json!({ "status": task.status.as_str() }),
                        Some(board_slug),
                    )
                    .await
                {
                    res = updated;
                }
            }
        }
        Ok(res)
    }

    pub async fn update_kanban_task(
        &self,
        task_id: &str,
        patch: Value,
        board_slug: Option<&str>,
    ) -> Result<Value, String> {
        let enc = urlencoding_segment(task_id);
        let q: Vec<(&str, String)> = board_slug
            .map(|b| vec![("board", b.to_string())])
            .unwrap_or_default();
        self.request_json(
            Method::PATCH,
            &format!("/api/plugins/kanban/tasks/{enc}"),
            &q,
            Some(patch),
        )
        .await
    }

    pub async fn delete_kanban_task(
        &self,
        task_id: &str,
        board_slug: Option<&str>,
    ) -> Result<Value, String> {
        let enc = urlencoding_segment(task_id);
        let q: Vec<(&str, String)> = board_slug
            .map(|b| vec![("board", b.to_string())])
            .unwrap_or_default();
        self.request_json(
            Method::DELETE,
            &format!("/api/plugins/kanban/tasks/{enc}"),
            &q,
            None,
        )
        .await
    }

    pub async fn dispatch_kanban(&self, board_slug: Option<&str>) -> Result<Value, String> {
        let q: Vec<(&str, String)> = board_slug
            .filter(|b| *b != "all")
            .map(|b| vec![("board", b.to_string())])
            .unwrap_or_default();
        self.request_json(Method::POST, "/api/plugins/kanban/dispatch", &q, None)
            .await
    }
}

fn parse_usage_report(value: Value) -> Result<HermesUsageReport, String> {
    if value.is_array() {
        serde_json::from_value(value).map(|daily| HermesUsageReport {
            daily,
            ..Default::default()
        })
    } else {
        serde_json::from_value(value)
    }
    .map_err(|err| format!("Invalid Hermes usage report: {err}"))
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    #[test]
    fn dashboard_usage_keeps_daily_counts_and_model_rows() {
        let report = parse_usage_report(json!({
            "daily": [{
                "day": "2026-11-25", "input_tokens": 120, "output_tokens": 45,
                "estimated_cost": 0.025, "sessions": 1, "api_calls": 3
            }],
            "by_model": [{"model": "provider/model", "sessions": 1}],
            "totals": {"total_input": 120, "total_output": 45, "total_sessions": 1},
            "period_days": 14
        })).unwrap();

        let day = &report.daily[0];
        assert_eq!(day.date.to_string(), "2026-11-25");
        assert_eq!(day.total_tokens(), Some(165));
        assert_eq!(day.cost_usd, Some(0.025));
        assert_eq!(day.requests, Some(3));
        assert_eq!(report.by_model.unwrap()[0].model, "provider/model");
        assert_eq!(report.totals.unwrap().total_sessions, Some(1));
        assert_eq!(report.period_days, Some(14));
    }

    #[test]
    fn supports_older_daily_responses_but_rejects_unrelated_payloads() {
        let report = parse_usage_report(json!([
            {"date": "2026-11-24", "tokens": 12, "cost_usd": 0.0, "requests": 0}
        ])).unwrap();
        assert_eq!(report.daily[0].total_tokens(), Some(12));
        assert_eq!(report.daily[0].cost_usd, Some(0.0));
        assert_eq!(report.daily[0].requests, Some(0));
        assert!(report.by_model.is_none());
        assert!(parse_usage_report(json!({"daily": []})).unwrap().daily.is_empty());
        assert!(parse_usage_report(json!({"error": "unavailable"})).is_err());
        assert!(parse_usage_report(json!({"daily": [{"day": "not a date"}]})).is_err());
    }
}

fn extract_vec<T: serde::de::DeserializeOwned>(value: &Value, keys: &[&str]) -> Vec<T> {
    if let Some(arr) = value.as_array() {
        return arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
    }
    if let Some(obj) = value.as_object() {
        for key in keys {
            if let Some(arr) = obj.get(*key).and_then(Value::as_array) {
                return arr
                    .iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect();
            }
        }
    }
    Vec::new()
}

fn urlencoding_segment(raw: &str) -> String {
    url::form_urlencoded::byte_serialize(raw.as_bytes()).collect()
}
