use crate::ssh::SshTarget;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HermesServerMode {
    Dashboard,
    Openai,
}

impl Default for HermesServerMode {
    fn default() -> Self {
        Self::Dashboard
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HermesAuthType {
    None,
    SessionToken,
    Bearer,
    Cookie,
}

impl Default for HermesAuthType {
    fn default() -> Self {
        Self::SessionToken
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HermesConnection {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub mode: HermesServerMode,
    #[serde(default)]
    pub auth_type: HermesAuthType,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<f64>,
    #[serde(default)]
    pub auth_provider: Option<String>,
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub ssh: Option<SshTarget>,
}

impl Default for HermesConnection {
    fn default() -> Self {
        Self {
            id: "local-rust".into(),
            name: "This computer".into(),
            url: "http://127.0.0.1:9119".into(),
            mode: HermesServerMode::Dashboard,
            auth_type: HermesAuthType::SessionToken,
            token: None,
            refresh_token: None,
            expires_at: None,
            auth_provider: None,
            default_profile: None,
            ssh: None,
        }
    }
}

impl HermesConnection {
    pub fn is_local(&self) -> bool {
        self.id == "local-rust" || self.id == "local-dev"
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesStatus {
    pub ok: Option<bool>,
    pub version: Option<String>,
    pub gateway_running: Option<bool>,
    pub gateway_state: Option<String>,
    pub gateway_pid: Option<u64>,
    pub active_profile: Option<String>,
    pub active_sessions: Option<u64>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub uptime_seconds: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesProfile {
    pub name: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub default: bool,
    pub description: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub path: Option<String>,
    pub session_count: Option<u64>,
    pub skill_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesSession {
    pub id: String,
    pub title: Option<String>,
    pub name: Option<String>,
    pub profile: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub source: Option<String>,
    pub message_count: Option<i64>,
    pub created_at: Option<Value>,
    pub updated_at: Option<Value>,
    pub started_at: Option<Value>,
    pub preview: Option<String>,
    pub tokens_used: Option<i64>,
    pub context_limit: Option<u64>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    pub board_slug: Option<String>,
}

impl HermesSession {
    pub fn display_title(&self) -> &str {
        self.title
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.name.as_deref().filter(|s| !s.trim().is_empty()))
            .unwrap_or("Untitled Chat")
    }

    pub fn timestamp_secs(&self) -> Option<f64> {
        let pick = self
            .updated_at
            .as_ref()
            .or(self.started_at.as_ref())
            .or(self.created_at.as_ref())?;
        parse_timestamp_secs(pick)
    }
}

pub fn parse_timestamp_secs(v: &Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(if n > 1e12 { n / 1000.0 } else { n });
    }
    if let Some(s) = v.as_str() {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp() as f64);
        }
        if let Ok(n) = s.parse::<f64>() {
            return Some(if n > 1e12 { n / 1000.0 } else { n });
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallStatus {
    Running,
    Done,
    Error,
}

impl Default for ToolCallStatus {
    fn default() -> Self {
        Self::Done
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Option<Value>,
    pub output: Option<String>,
    #[serde(default)]
    pub status: ToolCallStatus,
    pub duration_ms: Option<u64>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Allowed,
    Always,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesApprovalRequest {
    pub id: String,
    pub tool: Option<String>,
    pub title: String,
    pub command: Option<String>,
    pub detail: Option<String>,
    pub settled: Option<ApprovalDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesClarifyRequest {
    pub id: String,
    pub question: String,
    pub choices: Option<Vec<String>>,
    pub answered: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub thinking: Option<String>,
    pub tool_name: Option<String>,
    pub timestamp: Option<f64>,
    pub model: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<HermesToolCall>,
    pub approval: Option<HermesApprovalRequest>,
    pub clarify: Option<HermesClarifyRequest>,
    #[serde(default)]
    pub streaming: bool,
}

impl HermesMessage {
    pub fn new_assistant(id: String) -> Self {
        Self {
            id,
            role: MessageRole::Assistant,
            content: String::new(),
            thinking: None,
            tool_name: None,
            timestamp: Some(now_secs()),
            model: None,
            tool_calls: Vec::new(),
            approval: None,
            clarify: None,
            streaming: true,
        }
    }
}

pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesModelOption {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub configured: bool,
    pub context_length: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesSkill {
    pub name: String,
    pub path: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub source: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesToolset {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub tools: Option<Vec<String>>,
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesMcpServer {
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    pub transport: Option<String>,
    pub enabled: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesCronJob {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub prompt: String,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub paused: bool,
    pub profile: Option<String>,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesMessagingPlatform {
    pub id: String,
    pub name: Option<String>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub connected: bool,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesWebhook {
    pub id: String,
    pub name: Option<String>,
    pub url: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesEnvEntry {
    pub key: String,
    pub masked: Option<String>,
    pub is_set: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesLogEntry {
    pub timestamp: Option<String>,
    pub level: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesUsagePoint {
    pub date: String,
    pub tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub requests: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HermesKanbanStatus {
    Triage,
    Todo,
    Scheduled,
    Blocked,
    Ready,
    Running,
    Review,
    Done,
    Archived,
}

impl HermesKanbanStatus {
    pub const COLUMNS: &'static [(Self, &'static str, u32)] = &[
        (Self::Triage, "Triage", 0x777777),
        (Self::Todo, "To Do", 0x1084FE),
        (Self::Scheduled, "Scheduled", 0x97683D),
        (Self::Blocked, "Blocked", 0xFF6700),
        (Self::Ready, "Ready", 0x9159FE),
        (Self::Running, "Running", 0xFF9800),
        (Self::Review, "Review", 0x00BCA6),
        (Self::Done, "Done", 0x00C972),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Triage => "Triage",
            Self::Todo => "To Do",
            Self::Scheduled => "Scheduled",
            Self::Blocked => "Blocked",
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Review => "Review",
            Self::Done => "Done",
            Self::Archived => "Archived",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Triage => "triage",
            Self::Todo => "todo",
            Self::Scheduled => "scheduled",
            Self::Blocked => "blocked",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Review => "review",
            Self::Done => "done",
            Self::Archived => "archived",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "triage" => Self::Triage,
            "scheduled" => Self::Scheduled,
            "blocked" => Self::Blocked,
            "ready" => Self::Ready,
            "running" => Self::Running,
            "review" => Self::Review,
            "done" => Self::Done,
            "archived" => Self::Archived,
            _ => Self::Todo,
        }
    }
}

impl Default for HermesKanbanStatus {
    fn default() -> Self {
        Self::Todo
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesBoardSummary {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub workspace_path: Option<String>,
    pub task_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesKanbanTask {
    pub id: String,
    pub board: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub status: HermesKanbanStatus,
    pub priority: Option<Value>,
    pub assignee: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

impl HermesKanbanTask {
    pub fn priority_num(&self) -> i32 {
        match &self.priority {
            Some(Value::Number(n)) => n.as_i64().unwrap_or(0) as i32,
            Some(Value::String(s)) => match s.as_str() {
                "low" => -1,
                "medium" => 0,
                "high" => 1,
                "urgent" => 2,
                other => other.parse().unwrap_or(0),
            },
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesLearningNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesLearningGraph {
    pub nodes: Vec<HermesLearningNode>,
    pub edges: Vec<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesSessionUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub context_tokens: Option<u64>,
    pub context_limit: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HermesTodoItem {
    pub id: Option<String>,
    pub content: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProviderInfo {
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub enum HermesStreamEvent {
    Status {
        status: String,
        detail: Option<String>,
    },
    GatewayReady,
    SessionInfo {
        session_id: Option<String>,
        model: Option<String>,
        provider: Option<String>,
    },
    SessionUsage {
        session_id: Option<String>,
        usage: HermesSessionUsage,
    },
    StatusUpdate {
        session_id: Option<String>,
        text: String,
    },
    MessageStart {
        session_id: Option<String>,
    },
    TextDelta {
        session_id: Option<String>,
        delta: String,
    },
    ThinkingDelta {
        session_id: Option<String>,
        delta: String,
    },
    MessageInterim {
        session_id: Option<String>,
        content: String,
    },
    ToolStart {
        session_id: Option<String>,
        tool_call: HermesToolCall,
    },
    ToolProgress {
        session_id: Option<String>,
        tool_call_id: String,
        preview: String,
    },
    ToolEnd {
        session_id: Option<String>,
        tool_call_id: String,
        output: Option<String>,
        error: Option<String>,
        duration_ms: Option<u64>,
    },
    TodoUpdated {
        session_id: Option<String>,
        todos: Vec<HermesTodoItem>,
    },
    ApprovalRequest {
        session_id: Option<String>,
        approval: HermesApprovalRequest,
    },
    ApprovalSettled {
        session_id: Option<String>,
        id: String,
        decision: ApprovalDecision,
    },
    ClarifyRequest {
        session_id: Option<String>,
        clarify: HermesClarifyRequest,
    },
    ClarifySettled {
        session_id: Option<String>,
        id: String,
        answer: String,
    },
    MessageComplete {
        session_id: Option<String>,
        content: Option<String>,
    },
    Error {
        session_id: Option<String>,
        message: String,
    },
}
