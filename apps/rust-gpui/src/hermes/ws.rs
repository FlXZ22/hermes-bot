use crate::hermes::client::HermesClient;
use crate::hermes::types::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;

#[derive(Clone)]
pub struct HermesRealtimeSession {
    client: HermesClient,
    tx: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>,
    events: broadcast::Sender<HermesStreamEvent>,
    rpc_seq: Arc<AtomicU64>,
    connected: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl HermesRealtimeSession {
    pub fn new(client: HermesClient) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            client,
            tx: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            events,
            rpc_seq: Arc::new(AtomicU64::new(1)),
            connected: Arc::new(AtomicBool::new(false)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            task: Arc::new(Mutex::new(None)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HermesStreamEvent> {
        self.events.subscribe()
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub async fn disconnect(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.connected.store(false, Ordering::SeqCst);
        self.tx.lock().await.take();
        if let Some(h) = self.task.lock().await.take() {
            h.abort();
        }
        let mut p = self.pending.lock().await;
        for (_, sender) in p.drain() {
            let _ = sender.send(Err("WebSocket disconnected".into()));
        }
    }

    pub async fn connect(&self, profile: Option<String>) -> Result<(), String> {
        self.disconnect().await;
        self.stop_flag.store(false, Ordering::SeqCst);
        let _ = self.events.send(HermesStreamEvent::Status {
            status: "connecting".into(),
            detail: None,
        });

        let ws_url = self.build_ws_url(profile.as_deref()).await;
        let (ws_stream, _) = tokio::time::timeout(
            Duration::from_secs(10),
            tokio_tungstenite::connect_async(&ws_url),
        )
        .await
        .map_err(|_| "WebSocket connection timed out".to_string())?
        .map_err(|e| format!("Could not reach Hermes WebSocket: {e}"))?;

        self.connected.store(true, Ordering::SeqCst);
        let _ = self.events.send(HermesStreamEvent::Status {
            status: "connected".into(),
            detail: None,
        });

        self.spawn_io_loop(ws_stream, profile).await;
        Ok(())
    }

    async fn build_ws_url(&self, profile: Option<&str>) -> String {
        let conn = self.client.connection();
        let base = conn.url.trim().trim_end_matches('/');
        let ws_base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base.to_string()
        };
        let mut params = Vec::new();
        if let Some(p) = profile.filter(|s| !s.is_empty()) {
            params.push(("profile".to_string(), p.to_string()));
        }
        match conn.auth_type {
            HermesAuthType::SessionToken => {
                if let Some(tok) = conn.token.as_deref().filter(|t| !t.is_empty()) {
                    params.push(("token".into(), tok.to_string()));
                }
            }
            HermesAuthType::Bearer | HermesAuthType::Cookie => {
                if let Some(ticket) = self.client.mint_ws_ticket().await {
                    params.push(("ticket".into(), ticket));
                } else if let Some(tok) = conn.token.as_deref().filter(|t| !t.is_empty()) {
                    params.push(("token".into(), tok.to_string()));
                }
            }
            HermesAuthType::None => {}
        }
        if params.is_empty() {
            format!("{ws_base}/api/ws")
        } else {
            let mut ser = url::form_urlencoded::Serializer::new(String::new());
            for (k, v) in params {
                ser.append_pair(&k, &v);
            }
            format!("{ws_base}/api/ws?{}", ser.finish())
        }
    }

    async fn spawn_io_loop(
        &self,
        initial_stream: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        profile: Option<String>,
    ) {
        let (initial_out_tx, initial_out_rx) = mpsc::unbounded_channel::<String>();
        *self.tx.lock().await = Some(initial_out_tx);
        let me = self.clone();
        let handle = tokio::spawn(async move {
            let mut stream = Some((initial_stream, initial_out_rx));
            let mut attempt: u32 = 0;

            loop {
                if me.stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                let (ws, mut out_rx) = match stream.take() {
                    Some(pair) => pair,
                    None => {
                        let delay_ms = (500u64.saturating_mul(1u64 << attempt.min(5))).min(15_000);
                        attempt = attempt.saturating_add(1);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        if me.stop_flag.load(Ordering::SeqCst) {
                            break;
                        }
                        let url = me.build_ws_url(profile.as_deref()).await;
                        match tokio::time::timeout(
                            Duration::from_secs(10),
                            tokio_tungstenite::connect_async(&url),
                        )
                        .await
                        {
                            Ok(Ok((s, _))) => {
                                attempt = 0;
                                let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
                                *me.tx.lock().await = Some(out_tx);
                                me.connected.store(true, Ordering::SeqCst);
                                let _ = me.events.send(HermesStreamEvent::Status {
                                    status: "connected".into(),
                                    detail: None,
                                });
                                (s, out_rx)
                            }
                            _ => continue,
                        }
                    }
                };

                let (mut sink, mut reader) = ws.split();

                let writer = tokio::spawn(async move {
                    while let Some(msg) = out_rx.recv().await {
                        if sink.send(WsMessage::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                });

                let hb_me = me.clone();
                let heartbeat = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(20));
                    interval.tick().await;
                    loop {
                        interval.tick().await;
                        if !hb_me.is_connected() {
                            break;
                        }
                        let _ = hb_me.rpc("gateway.ping", json!({}), 10_000).await;
                    }
                });

                while let Some(item) = reader.next().await {
                    match item {
                        Ok(WsMessage::Text(txt)) => {
                            for line in txt.split('\n') {
                                let trimmed = line.trim();
                                if !trimmed.is_empty() {
                                    me.handle_raw_frame(trimmed).await;
                                }
                            }
                        }
                        Ok(WsMessage::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }

                writer.abort();
                heartbeat.abort();
                me.connected.store(false, Ordering::SeqCst);
                me.tx.lock().await.take();
                {
                    let mut p = me.pending.lock().await;
                    for (_, sender) in p.drain() {
                        let _ = sender.send(Err("WebSocket closed".into()));
                    }
                }
                if me.stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                let _ = me.events.send(HermesStreamEvent::Status {
                    status: "disconnected".into(),
                    detail: None,
                });
            }
        });
        *self.task.lock().await = Some(handle);
    }

    async fn handle_raw_frame(&self, raw: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(raw) else {
            return;
        };

        // JSON-RPC response
        if let Some(id_val) = msg.get("id") {
            let id_key = id_val
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| id_val.to_string());
            if let Some(sender) = self.pending.lock().await.remove(&id_key) {
                if let Some(err) = msg.get("error") {
                    let text = err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("RPC error")
                        .to_string();
                    let _ = sender.send(Err(text));
                } else {
                    let _ = sender.send(Ok(msg.get("result").cloned().unwrap_or(Value::Null)));
                }
                return;
            }
        }

        // Event envelope
        let (event_type, session_id, payload) =
            if msg.get("method").and_then(Value::as_str) == Some("event") {
                let env = msg.get("params").cloned().unwrap_or(Value::Null);
                let et = env
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let sid = env
                    .get("session_id")
                    .or_else(|| env.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(String::from);
                let pl = env
                    .get("payload")
                    .filter(|v| v.is_object())
                    .cloned()
                    .unwrap_or(env);
                (et, sid, pl)
            } else {
                let et = msg
                    .get("method")
                    .or_else(|| msg.get("type"))
                    .or_else(|| msg.get("event"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let pl = msg
                    .get("params")
                    .or_else(|| msg.get("data"))
                    .cloned()
                    .unwrap_or_else(|| msg.clone());
                let sid = pl
                    .get("session_id")
                    .or_else(|| pl.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(String::from);
                (et, sid, pl)
            };

        let ev = match event_type.as_str() {
            "gateway.ready" => Some(HermesStreamEvent::GatewayReady),
            "session.info" => Some(HermesStreamEvent::SessionInfo {
                session_id,
                model: payload.get("model").and_then(Value::as_str).map(String::from),
                provider: payload
                    .get("provider")
                    .and_then(Value::as_str)
                    .map(String::from),
            }),
            "session.usage" => {
                let usage_val = payload.get("usage").unwrap_or(&payload).clone();
                Some(HermesStreamEvent::SessionUsage {
                    session_id,
                    usage: serde_json::from_value(usage_val).unwrap_or_default(),
                })
            }
            "session.title" => payload
                .get("title")
                .and_then(Value::as_str)
                .map(|t| HermesStreamEvent::SessionTitle {
                    session_id,
                    title: t.to_string(),
                }),
            "status.update" => Some(HermesStreamEvent::StatusUpdate {
                session_id,
                kind: payload.get("kind").and_then(Value::as_str).map(String::from),
                text: payload
                    .get("text")
                    .or_else(|| payload.get("status"))
                    .or_else(|| payload.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            "message.start" => Some(HermesStreamEvent::MessageStart { session_id }),
            "message.delta" | "text.delta" | "delta" | "assistant.delta" | "stream.delta" => {
                let d = payload
                    .get("delta")
                    .or_else(|| payload.get("text"))
                    .or_else(|| payload.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                // Delta whitespace is significant; do not trim it before concatenation.
                let thinking = ["thinking", "reasoning_content"].into_iter().find_map(|key| {
                    payload.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
                });
                if d.is_empty() && thinking.is_none() {
                    None
                } else {
                    Some(HermesStreamEvent::TextDelta {
                        session_id,
                        delta: d.to_string(),
                        thinking: thinking.map(String::from),
                    })
                }
            }
            "thinking.delta" | "reasoning.delta" | "reasoning.available" => {
                let d = payload
                    .get("delta")
                    .or_else(|| payload.get("text"))
                    .or_else(|| payload.get("reasoning"))
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("thinking").and_then(Value::as_str))
                    .or_else(|| payload.get("reasoning_content").and_then(Value::as_str))
                    .unwrap_or("");
                if d.is_empty() {
                    None
                } else {
                    Some(HermesStreamEvent::ThinkingDelta {
                        session_id,
                        delta: d.to_string(),
                    })
                }
            }
            "message.interim" => {
                let c = payload
                    .get("content")
                    .or_else(|| payload.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let thinking = thinking_from_fields(&payload);
                if c.is_empty() && thinking.is_none() {
                    None
                } else {
                    Some(HermesStreamEvent::MessageInterim {
                        session_id,
                        content: c.to_string(),
                        thinking,
                    })
                }
            }
            "tool.generating" => Some(HermesStreamEvent::ToolGenerating {
                session_id,
                name: payload
                    .get("name")
                    .or_else(|| payload.get("tool"))
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
            }),
            "tool.start" | "tool_call.start" => Some(HermesStreamEvent::ToolStart {
                session_id,
                tool_call: HermesToolCall {
                    id: payload
                        .get("tool_id")
                        .or_else(|| payload.get("call_id"))
                        .or_else(|| payload.get("id"))
                        .or_else(|| payload.get("tool_call_id"))
                        .and_then(Value::as_str)
                        .map(String::from)
                        .unwrap_or_else(|| format!("tool-{}", uuid::Uuid::new_v4())),
                    name: payload
                        .get("name")
                        .or_else(|| payload.get("tool"))
                        .or_else(|| payload.get("tool_name"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string(),
                    arguments: payload
                        .get("arguments")
                        .or_else(|| payload.get("args"))
                        .or_else(|| payload.get("input"))
                        .cloned()
                        .or_else(|| {
                            payload
                                .get("args_text")
                                .and_then(Value::as_str)
                                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        }),
                    preview: payload
                        .get("preview")
                        .or_else(|| payload.get("context"))
                        .and_then(Value::as_str)
                        .map(String::from),
                    output: None,
                    status: ToolCallStatus::Running,
                    duration_ms: None,
                    inline_diff: None,
                    waiting_approval: false,
                },
            }),
            "tool.progress" => Some(HermesStreamEvent::ToolProgress {
                session_id,
                tool_call_id: payload
                    .get("tool_id")
                    .or_else(|| payload.get("call_id"))
                    .or_else(|| payload.get("id"))
                    .or_else(|| payload.get("tool_call_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                preview: payload
                    .get("preview")
                    .or_else(|| payload.get("text"))
                    .or_else(|| payload.get("output"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            "tool.complete" | "tool.end" | "tool_call.end" | "tool.result" => {
                let duration_ms = payload
                    .get("duration_ms")
                    .or_else(|| payload.get("elapsed_ms"))
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        payload
                            .get("duration_s")
                            .and_then(Value::as_f64)
                            .filter(|s| s.is_finite() && *s >= 0.0)
                            .map(|s| (s * 1000.0).round() as u64)
                    });
                let output = payload
                    .get("result_text")
                    .or_else(|| payload.get("output"))
                    .or_else(|| payload.get("result"))
                    .or_else(|| payload.get("summary"))
                    .map(|v| {
                        v.as_str()
                            .map(String::from)
                            .unwrap_or_else(|| serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()))
                    });
                let error = payload
                    .get("error")
                    .and_then(Value::as_str)
                    .map(String::from)
                    .or_else(|| {
                        if payload.get("is_error").and_then(Value::as_bool) == Some(true) {
                            output.clone().or_else(|| Some("Tool execution failed".into()))
                        } else {
                            None
                        }
                    });
                Some(HermesStreamEvent::ToolEnd {
                    session_id,
                    tool_call_id: payload
                        .get("tool_id")
                        .or_else(|| payload.get("call_id"))
                        .or_else(|| payload.get("id"))
                        .or_else(|| payload.get("tool_call_id"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    output,
                    error,
                    duration_ms,
                    inline_diff: payload
                        .get("inline_diff")
                        .and_then(Value::as_str)
                        .map(String::from),
                })
            }
            "subagent.start" => Some(HermesStreamEvent::SubagentStart {
                session_id,
                subagent_id: payload
                    .get("subagent_id")
                    .or_else(|| payload.get("id"))
                    .or_else(|| payload.get("child_session_id"))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!(
                            "subagent-{}",
                            payload.get("task_index").and_then(Value::as_u64).unwrap_or(0)
                        )
                    }),
                parent_id: payload.get("parent_id").and_then(Value::as_str).map(String::from),
                child_session_id: payload
                    .get("child_session_id")
                    .and_then(Value::as_str)
                    .map(String::from),
                depth: payload.get("depth").and_then(Value::as_u64).unwrap_or(1) as u32,
                goal: payload
                    .get("goal")
                    .or_else(|| payload.get("task"))
                    .and_then(Value::as_str)
                    .unwrap_or("Subagent task")
                    .to_string(),
                model: payload.get("model").and_then(Value::as_str).map(String::from),
            }),
            "subagent.tool" | "subagent.thinking" => {
                let subagent_id = payload
                    .get("subagent_id")
                    .or_else(|| payload.get("id"))
                    .or_else(|| payload.get("child_session_id"))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!(
                            "subagent-{}",
                            payload.get("task_index").and_then(Value::as_u64).unwrap_or(0)
                        )
                    });
                let tool_name = payload.get("tool_name").and_then(Value::as_str).map(String::from);
                let operation = if event_type == "subagent.thinking" {
                    payload
                        .get("text")
                        .and_then(Value::as_str)
                        .map(|t| format!("Thinking: {}", t.lines().next().unwrap_or(t)))
                        .unwrap_or_else(|| "Thinking…".into())
                } else {
                    let preview = payload.get("tool_preview").and_then(Value::as_str).unwrap_or("");
                    match (tool_name.as_deref(), preview.is_empty()) {
                        (Some(tn), false) => format!("{tn}: {preview}"),
                        (Some(tn), true) => format!("Running {tn}"),
                        (None, false) => preview.to_string(),
                        (None, true) => "Working…".into(),
                    }
                };
                Some(HermesStreamEvent::SubagentUpdate {
                    session_id,
                    subagent_id,
                    operation,
                    tool_name,
                    tool_count: payload.get("tool_count").and_then(Value::as_u64),
                    input_tokens: payload.get("input_tokens").and_then(Value::as_u64),
                    output_tokens: payload.get("output_tokens").and_then(Value::as_u64),
                })
            }
            "subagent.complete" => Some(HermesStreamEvent::SubagentComplete {
                session_id,
                subagent_id: payload
                    .get("subagent_id")
                    .or_else(|| payload.get("id"))
                    .or_else(|| payload.get("child_session_id"))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or_else(|| {
                        format!(
                            "subagent-{}",
                            payload.get("task_index").and_then(Value::as_u64).unwrap_or(0)
                        )
                    }),
                status: payload
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("completed")
                    .to_string(),
                summary: payload
                    .get("summary")
                    .or_else(|| payload.get("output_tail"))
                    .and_then(Value::as_str)
                    .map(String::from),
                duration_ms: payload
                    .get("duration_seconds")
                    .or_else(|| payload.get("duration_s"))
                    .and_then(Value::as_f64)
                    .filter(|s| s.is_finite() && *s >= 0.0)
                    .map(|s| (s * 1000.0).round() as u64),
                input_tokens: payload.get("input_tokens").and_then(Value::as_u64),
                output_tokens: payload.get("output_tokens").and_then(Value::as_u64),
            }),
            "todo.updated" => {
                let raw_list = payload
                    .get("todos")
                    .or_else(|| payload.get("items"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let todos = raw_list
                    .into_iter()
                    .enumerate()
                    .map(|(idx, t)| HermesTodoItem {
                        id: t
                            .get("id")
                            .and_then(Value::as_str)
                            .map(String::from)
                            .or_else(|| Some(idx.to_string())),
                        content: t
                            .get("content")
                            .or_else(|| t.get("title"))
                            .or_else(|| t.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        status: t
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("pending")
                            .to_string(),
                    })
                    .collect();
                Some(HermesStreamEvent::TodoUpdated { session_id, todos })
            }
            "approval.request" => Some(HermesStreamEvent::ApprovalRequest {
                session_id,
                approval: HermesApprovalRequest {
                    id: payload
                        .get("request_id")
                        .or_else(|| payload.get("id"))
                        .or_else(|| payload.get("approval_id"))
                        .and_then(Value::as_str)
                        .map(String::from)
                        .unwrap_or_else(|| format!("appr-{}", uuid::Uuid::new_v4())),
                    tool: payload.get("tool").and_then(Value::as_str).map(String::from),
                    title: payload
                        .get("title")
                        .or_else(|| payload.get("description"))
                        .or_else(|| payload.get("tool"))
                        .and_then(Value::as_str)
                        .unwrap_or("Run command")
                        .to_string(),
                    command: payload
                        .get("command")
                        .and_then(Value::as_str)
                        .map(String::from),
                    detail: payload
                        .get("command")
                        .or_else(|| payload.get("detail"))
                        .or_else(|| payload.get("reason"))
                        .and_then(Value::as_str)
                        .map(String::from),
                    settled: None,
                },
            }),
            "approval.settled" => {
                let raw_dec = payload
                    .get("decision")
                    .or_else(|| payload.get("choice"))
                    .and_then(Value::as_str)
                    .unwrap_or("allowed");
                let decision = match raw_dec {
                    "deny" | "denied" => ApprovalDecision::Denied,
                    "always" => ApprovalDecision::Always,
                    _ => ApprovalDecision::Allowed,
                };
                Some(HermesStreamEvent::ApprovalSettled {
                    session_id,
                    id: payload
                        .get("request_id")
                        .or_else(|| payload.get("id"))
                        .or_else(|| payload.get("approval_id"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    decision,
                })
            }
            "clarify.request" => Some(HermesStreamEvent::ClarifyRequest {
                session_id,
                clarify: HermesClarifyRequest {
                    id: payload
                        .get("request_id")
                        .or_else(|| payload.get("id"))
                        .and_then(Value::as_str)
                        .map(String::from)
                        .unwrap_or_else(|| format!("clarify-{}", uuid::Uuid::new_v4())),
                    question: payload
                        .get("question")
                        .or_else(|| payload.get("prompt"))
                        .and_then(Value::as_str)
                        .unwrap_or("Please clarify")
                        .to_string(),
                    choices: payload.get("choices").and_then(Value::as_array).map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    }),
                    answered: None,
                },
            }),
            "clarify.settled" => Some(HermesStreamEvent::ClarifySettled {
                session_id,
                id: payload
                    .get("request_id")
                    .or_else(|| payload.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                answer: payload
                    .get("answer")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            "message.complete" | "turn.complete" | "done" | "session.idle" => {
                let usage = payload
                    .get("usage")
                    .and_then(|u| serde_json::from_value::<HermesSessionUsage>(u.clone()).ok());
                Some(HermesStreamEvent::MessageComplete {
                    session_id,
                    content: payload
                        .get("content")
                        .or_else(|| payload.get("text"))
                        .and_then(Value::as_str)
                        .map(String::from),
                    thinking: thinking_from_fields(&payload).or_else(|| {
                        payload
                            .get("reasoning")
                            .and_then(Value::as_str)
                            .map(String::from)
                    }),
                    status: payload
                        .get("status")
                        .and_then(Value::as_str)
                        .map(String::from),
                    error: payload
                        .get("error")
                        .or_else(|| payload.get("failure_reason"))
                        .and_then(Value::as_str)
                        .map(String::from),
                    usage,
                })
            }
            "error" => Some(HermesStreamEvent::Error {
                session_id,
                message: payload
                    .get("message")
                    .or_else(|| payload.get("error"))
                    .and_then(Value::as_str)
                    .unwrap_or("Hermes error")
                    .to_string(),
            }),
            _ => None,
        };

        if let Some(ev) = ev {
            let _ = self.events.send(ev);
        }
    }

    pub async fn rpc(&self, method: &str, params: Value, timeout_ms: u64) -> Result<Value, String> {
        let sender = self
            .tx
            .lock()
            .await
            .clone()
            .ok_or_else(|| "WebSocket is not connected".to_string())?;
        let id = format!("r{}", self.rpc_seq.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if sender.send(frame.to_string()).is_err() {
            self.pending.lock().await.remove(&id);
            return Err("WebSocket channel closed".into());
        }

        match tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err("WebSocket closed before RPC reply".into()),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(format!("RPC {method} timed out"))
            }
        }
    }

    pub async fn create_session(&self, params: Value) -> Result<Value, String> {
        self.rpc("session.create", params, 60_000).await
    }

    pub async fn resume_session(&self, session_id: &str, profile: &str) -> Result<Value, String> {
        self.rpc(
            "session.resume",
            json!({ "session_id": session_id, "profile": profile, "omit_messages": true }),
            30_000,
        )
        .await
    }

    pub async fn submit_prompt(&self, session_id: &str, text: &str) -> Result<Value, String> {
        self.rpc(
            "prompt.submit",
            json!({ "session_id": session_id, "text": text }),
            120_000,
        )
        .await
    }

    pub async fn interrupt_session(&self, session_id: &str) -> Result<Value, String> {
        self.rpc("session.interrupt", json!({ "session_id": session_id }), 15_000)
            .await
    }

    pub async fn compress_session(&self, session_id: &str) -> Result<Value, String> {
        self.rpc("session.compress", json!({ "session_id": session_id }), 60_000)
            .await
    }

    pub async fn branch_session(&self, session_id: &str) -> Result<Value, String> {
        self.rpc("session.branch", json!({ "session_id": session_id }), 30_000)
            .await
    }

    pub async fn set_session_title(&self, session_id: &str, title: &str) -> Result<Value, String> {
        self.rpc(
            "session.title",
            json!({ "session_id": session_id, "title": title }),
            15_000,
        )
        .await
    }

    pub async fn respond_approval(
        &self,
        session_id: &str,
        request_id: &str,
        choice: &str,
    ) -> Result<Value, String> {
        self.rpc(
            "approval.respond",
            json!({ "session_id": session_id, "request_id": request_id, "choice": choice }),
            15_000,
        )
        .await
    }

    pub async fn respond_clarify(
        &self,
        session_id: &str,
        request_id: &str,
        answer: &str,
    ) -> Result<Value, String> {
        self.rpc(
            "clarify.respond",
            json!({ "session_id": session_id, "request_id": request_id, "answer": answer }),
            15_000,
        )
        .await
    }

    pub async fn set_session_config(
        &self,
        session_id: &str,
        key: &str,
        value: &str,
    ) -> Result<Value, String> {
        let res = self
            .rpc(
                "config.set",
                json!({ "session_id": session_id, "key": key, "value": value, "scope": "session" }),
                15_000,
            )
            .await?;
        if res.get("confirm_required").and_then(Value::as_bool) == Some(true) {
            let msg = res
                .get("confirm_message")
                .or_else(|| res.get("warning"))
                .and_then(Value::as_str)
                .unwrap_or("Model change requires confirmation in Hermes");
            return Err(msg.to_string());
        }
        Ok(res)
    }

    pub async fn dispatch_command(&self, session_id: &str, input: &str) -> Result<Value, String> {
        let trimmed = input.trim().trim_start_matches('/');
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();
        self.rpc(
            "command.dispatch",
            json!({ "session_id": session_id, "name": name, "arg": arg }),
            60_000,
        )
        .await
    }

    pub async fn slash_exec(&self, session_id: &str, input: &str) -> Result<Value, String> {
        self.rpc(
            "slash.exec",
            json!({ "session_id": session_id, "input": input }),
            60_000,
        )
        .await
    }

    pub async fn commands_catalog(&self) -> Result<Value, String> {
        self.rpc("commands.catalog", json!({}), 15_000).await
    }

    pub async fn projects_list(&self) -> Result<Value, String> {
        self.rpc("projects.list", json!({}), 15_000).await
    }

    pub async fn set_session_cwd(&self, session_id: &str, cwd: &str) -> Result<Value, String> {
        self.rpc(
            "session.cwd.set",
            json!({ "session_id": session_id, "cwd": cwd }),
            15_000,
        )
        .await
    }

    pub async fn interrupt_subagent(&self, session_id: &str, subagent_id: &str) -> Result<Value, String> {
        self.rpc(
            "subagent.interrupt",
            json!({ "session_id": session_id, "subagent_id": subagent_id }),
            15_000,
        )
        .await
    }

    pub async fn create_project(
        &self,
        name: &str,
        description: Option<&str>,
        color: Option<&str>,
        primary_path: &str,
        board_slug: &str,
    ) -> Result<Value, String> {
        self.rpc(
            "projects.create",
            json!({
                "name": name,
                "description": description,
                "color": color,
                "primary_path": primary_path,
                "board_slug": board_slug,
            }),
            15_000,
        )
        .await
    }
}
