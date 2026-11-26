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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Allowed,
    Always,
    Denied,
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct HermesApprovalRequest {
    pub id: String,
    pub tool: Option<String>,
    pub title: String,
    pub command: Option<String>,
    pub detail: Option<String>,
    pub settled: Option<ApprovalDecision>,
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct HermesClarifyRequest {
    pub id: String,
    pub question: String,
    pub choices: Option<Vec<String>>,
    pub answered: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

const THINK_TAGS: [(&str, &str); 4] = [
    ("<think>", "</think>"),
    ("<thinking>", "</thinking>"),
    ("<reasoning>", "</reasoning>"),
    ("<thought>", "</thought>"),
];

pub fn merge_thinking(first: Option<&str>, second: Option<&str>) -> Option<String> {
    let mut blocks = Vec::new();
    push_thinking(&mut blocks, first.unwrap_or_default());
    push_thinking(&mut blocks, second.unwrap_or_default());
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

fn push_thinking<'a>(blocks: &mut Vec<&'a str>, block: &'a str) {
    let block = block.trim();
    // A provider may repeat an inline block within a field containing multiple paragraphs.
    let contains_block = |text: &str, block: &str| {
        text.match_indices(block).any(|(start, _)| {
            let end = start + block.len();
            (start == 0 || text[..start].ends_with("\n\n"))
                && (end == text.len() || text[end..].starts_with("\n\n"))
        })
    };
    if !block.is_empty() && !blocks.iter().any(|old| contains_block(old, block)) {
        blocks.retain(|old| !contains_block(block, old));
        blocks.push(block);
    }
}

pub fn thinking_from_fields(value: &Value) -> Option<String> {
    merge_thinking(
        value.get("thinking").and_then(Value::as_str),
        value.get("reasoning_content").and_then(Value::as_str),
    )
}

/// Split reasoning markup out of an assistant response. Call on the accumulated raw text,
/// not the visible text from an earlier call: a tag may span any number of stream deltas.
pub fn extract_thinking_from_content(
    raw_content: &str,
    existing_thinking: Option<&str>,
    is_streaming: bool,
) -> (Option<String>, String) {
    let (thinking, visible, _) = split_thinking_content(raw_content, existing_thinking, is_streaming);
    (thinking, visible)
}

/// The third return value indicates that a subsequent delta can still finish a tag or block.
pub(crate) fn split_thinking_content(
    raw_content: &str,
    existing_thinking: Option<&str>,
    is_streaming: bool,
) -> (Option<String>, String, bool) {
    let mut blocks = Vec::new();
    push_thinking(&mut blocks, existing_thinking.unwrap_or_default());
    let mut visible = String::new();
    let mut thought = String::new();
    let mut open_tags = Vec::new();
    let mut inline_blocks = Vec::new();
    let mut rest = raw_content;
    let mut removed_markup = false;
    let mut partial_tag = false;

    while let Some(at) = rest.find('<') {
        let dest = if open_tags.is_empty() { &mut visible } else { &mut thought };
        dest.push_str(&rest[..at]);
        rest = &rest[at..];

        let starts_tag = |tag: &str| {
            rest.as_bytes().get(..tag.len()).is_some_and(|prefix| {
                prefix.eq_ignore_ascii_case(tag.as_bytes())
            })
        };
        if let Some((open, close)) = THINK_TAGS.iter().find(|(open, _)| starts_tag(open)) {
            rest = &rest[open.len()..];
            open_tags.push(*close);
        } else if let Some((_, close)) = THINK_TAGS.iter().find(|(_, close)| starts_tag(close)) {
            rest = &rest[close.len()..];
            if let Some(index) = open_tags.iter().rposition(|tag| tag == close) {
                open_tags.truncate(index);
                if open_tags.is_empty() {
                    inline_blocks.push(std::mem::take(&mut thought));
                }
            }
        } else if (is_streaming || rest.len() > 2 || !open_tags.is_empty())
            && THINK_TAGS.iter().any(|(open, close)| {
                [open, close].into_iter().any(|tag| {
                    tag.as_bytes().get(..rest.len()).is_some_and(|prefix| {
                        prefix.eq_ignore_ascii_case(rest.as_bytes())
                    })
                })
            })
        {
            // Hide an incomplete tag until the next delta proves what it is.
            removed_markup = true;
            partial_tag = true;
            rest = "";
            break;
        } else {
            dest.push('<');
            rest = &rest[1..];
            continue;
        }
        removed_markup = true;
    }

    if open_tags.is_empty() {
        visible.push_str(rest);
    } else {
        // Keep even a truncated final block private if the turn is interrupted.
        thought.push_str(rest);
        inline_blocks.push(thought);
    }
    for block in &inline_blocks {
        push_thinking(&mut blocks, block);
    }

    if removed_markup {
        // Strip blank framing lines without changing indentation or Markdown hard breaks.
        if let Some(first) = visible.find(|c: char| !c.is_whitespace()) {
            let start = visible[..first].rfind('\n').map_or(0, |index| index + 1);
            let last = visible.trim_end().len();
            let end = visible[last..].find('\n').map_or(visible.len(), |index| last + index);
            visible.truncate(end);
            visible.drain(..start);
        } else {
            visible.clear();
        }
    }
    ((!blocks.is_empty()).then(|| blocks.join("\n\n")), visible, partial_tag || !open_tags.is_empty())
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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HermesUsageReport {
    #[serde(alias = "points", alias = "usage", alias = "items")]
    pub daily: Vec<HermesUsagePoint>,
    pub by_model: Option<Vec<HermesUsageModel>>,
    pub totals: Option<HermesUsageTotals>,
    pub period_days: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HermesUsagePoint {
    #[serde(alias = "day")]
    pub date: chrono::NaiveDate,
    pub tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    #[serde(alias = "estimated_cost")]
    pub cost_usd: Option<f64>,
    #[serde(alias = "api_calls")]
    pub requests: Option<u64>,
    pub sessions: Option<u64>,
}

impl HermesUsagePoint {
    pub fn total_tokens(&self) -> Option<u64> {
        self.tokens.or_else(|| match (self.input_tokens, self.output_tokens) {
            (None, None) => None,
            (input, output) => Some(input.unwrap_or(0).saturating_add(output.unwrap_or(0))),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HermesUsageModel {
    pub model: String,
    pub sessions: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HermesUsageTotals {
    pub total_input: Option<u64>,
    pub total_output: Option<u64>,
    pub total_estimated_cost: Option<f64>,
    pub total_sessions: Option<u64>,
    pub total_api_calls: Option<u64>,
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

    #[allow(dead_code)]
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

#[derive(Debug, Clone, Default, Hash, Serialize, Deserialize)]
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
        thinking: Option<String>,
    },
    ThinkingDelta {
        session_id: Option<String>,
        delta: String,
    },
    MessageInterim {
        session_id: Option<String>,
        content: String,
        thinking: Option<String>,
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
        thinking: Option<String>,
    },
    Error {
        session_id: Option<String>,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{extract_thinking_from_content as extract, thinking_from_fields};

    #[test]
    fn extracts_all_tag_spellings_case_insensitively_and_keeps_answer_order() {
        assert_eq!(
            extract("A<ThInK> 一 </THINK>B<Thinking>二</thinking><REASONING>三</reasoning><thought>四</Thought>C", None, false),
            (Some("一\n\n二\n\n三\n\n四".into()), "ABC".into()),
        );
    }

    #[test]
    fn partial_tags_never_enter_visible_text() {
        let raw = "<reasoning>検討中</ReAsOnInG>Answer";
        for (end, _) in raw.char_indices().skip(1) {
            let (_, visible) = extract(&raw[..end], None, true);
            assert!("Answer".starts_with(&visible), "prefix leaked: {visible:?}");
        }
        assert_eq!(extract("<think>leté… </th", None, true), (Some("leté…".into()), String::new()));
        assert_eq!(extract("A<reasoning>unfinished", None, false), (Some("unfinished".into()), "A".into()));
        assert_eq!(extract("length <", None, false), (None, "length <".into()));
    }

    #[test]
    fn merges_fields_and_inline_blocks_without_duplicate_paragraphs() {
        let fields = thinking_from_fields(&serde_json::json!({
            "thinking": null, "reasoning_content": "\n Step one\n\nStep two \n"
        }));
        assert_eq!(
            extract("<think>Step two</think><think>Step three</think><think>Step three</think>\n\nAnswer", fields.as_deref(), false),
            (Some("Step one\n\nStep two\n\nStep three".into()), "Answer".into()),
        );
    }

    #[test]
    fn preserves_plain_text_and_markdown_indentation() {
        let plain = "\n    café < 42\n<b>text</b>  \n";
        assert_eq!(extract(plain, Some(" \n "), false), (None, plain.into()));
        assert_eq!(extract("<think>why</think>\n\n    code", None, false), (Some("why".into()), "    code".into()));
        assert_eq!(extract("<think> \n </think>", None, false), (None, String::new()));
    }
}
