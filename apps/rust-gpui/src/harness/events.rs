use crate::theme::bot_face::{classify_hermes_activity, BotFaceState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Conversation mode chosen before the first user message and locked thereafter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConversationMode {
    #[default]
    Chat,
    Work,
}

impl ConversationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Work => "work",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "work" => Self::Work,
            _ => Self::Chat,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Work => "Work",
        }
    }
}

/// Persisted conversation mode + Work project binding for a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModeBinding {
    pub session_id: String,
    pub mode: ConversationMode,
    pub locked: bool,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub cwd: Option<String>,
    pub server_id: Option<String>,
    pub updated_at_ms: i64,
}

/// Reusable Work-mode project bound to a validated directory on a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkProject {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub server_id: String,
    pub is_home: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Single authoritative runtime execution state driving both the main chat/work
/// stream inline `face/` animation and inspector indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionState {
    #[default]
    Idle,
    Thinking,
    ExecutingTools,
    Editing,
    Writing,
    Waiting,
    Completed,
    Failed,
}

impl ExecutionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Thinking => "Thinking",
            Self::ExecutingTools => "Running tool",
            Self::Editing => "Editing files",
            Self::Writing => "Writing",
            Self::Waiting => "Waiting",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
        }
    }

    /// Map the authoritative runtime execution state to the existing `face/` engine
    /// `BotFaceState` while reusing every existing clip/morph asset.
    pub fn to_face_state(self, active_tool: Option<&str>, status_text: Option<&str>) -> BotFaceState {
        match self {
            Self::Idle => BotFaceState::Idle,
            Self::Thinking => BotFaceState::Thinking,
            Self::Editing => BotFaceState::Writing,
            Self::ExecutingTools => {
                let st = classify_hermes_activity(active_tool, status_text);
                if st == BotFaceState::Thinking {
                    BotFaceState::Working
                } else {
                    st
                }
            }
            Self::Writing => BotFaceState::Streaming,
            Self::Waiting => {
                if active_tool
                    .map(|t| t.contains("delegate") || t.contains("subagent"))
                    .unwrap_or(false)
                {
                    BotFaceState::Waiting
                } else {
                    BotFaceState::Blocked
                }
            }
            Self::Completed => BotFaceState::Finished,
            Self::Failed => BotFaceState::Failed,
        }
    }
}

/// Stable lifecycle classification for tool calls (distinguishing approval wait,
/// active execution, error, and completion without ever faking success).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ToolExecutionStatus {
    WaitingForApproval,
    Running,
    #[default]
    Completed,
    Failed,
    Cancelled,
}

#[allow(dead_code)]
impl ToolExecutionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingForApproval => "Waiting for approval",
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// Durable trajectory event kinds (append-only, deduplicated by `event_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryEventKind {
    TurnStart,
    UserMessage,
    ReasoningSummary,
    ToolCallStart,
    ToolCallResult,
    PtcExecution,
    PtcSubcall,
    ApprovalRequested,
    ApprovalDecided,
    SubagentSpawned,
    SubagentUpdate,
    SubagentCompleted,
    GoalChanged,
    Retry,
    Error,
    TurnCompleted,
}

impl TrajectoryEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TurnStart => "turn_start",
            Self::UserMessage => "user_message",
            Self::ReasoningSummary => "reasoning_summary",
            Self::ToolCallStart => "tool_call_start",
            Self::ToolCallResult => "tool_call_result",
            Self::PtcExecution => "ptc_execution",
            Self::PtcSubcall => "ptc_subcall",
            Self::ApprovalRequested => "approval_requested",
            Self::ApprovalDecided => "approval_decided",
            Self::SubagentSpawned => "subagent_spawned",
            Self::SubagentUpdate => "subagent_update",
            Self::SubagentCompleted => "subagent_completed",
            Self::GoalChanged => "goal_changed",
            Self::Retry => "retry",
            Self::Error => "error",
            Self::TurnCompleted => "turn_completed",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "turn_start" => Self::TurnStart,
            "user_message" => Self::UserMessage,
            "reasoning_summary" => Self::ReasoningSummary,
            "tool_call_start" => Self::ToolCallStart,
            "tool_call_result" => Self::ToolCallResult,
            "ptc_execution" => Self::PtcExecution,
            "ptc_subcall" => Self::PtcSubcall,
            "approval_requested" => Self::ApprovalRequested,
            "approval_decided" => Self::ApprovalDecided,
            "subagent_spawned" => Self::SubagentSpawned,
            "subagent_update" => Self::SubagentUpdate,
            "subagent_completed" => Self::SubagentCompleted,
            "goal_changed" => Self::GoalChanged,
            "retry" => Self::Retry,
            "error" => Self::Error,
            _ => Self::TurnCompleted,
        }
    }

    pub fn badge_label(self) -> &'static str {
        match self {
            Self::TurnStart => "TURN",
            Self::UserMessage => "USER",
            Self::ReasoningSummary => "THINK",
            Self::ToolCallStart => "CALL",
            Self::ToolCallResult => "RESULT",
            Self::PtcExecution => "PTC",
            Self::PtcSubcall => "SUBCALL",
            Self::ApprovalRequested => "APPROVE",
            Self::ApprovalDecided => "DECISION",
            Self::SubagentSpawned => "AGENT+",
            Self::SubagentUpdate => "AGENT",
            Self::SubagentCompleted => "AGENT✓",
            Self::GoalChanged => "GOAL",
            Self::Retry => "RETRY",
            Self::Error => "ERROR",
            Self::TurnCompleted => "DONE",
        }
    }
}

/// Durable append-only trajectory entry linked to stable session, run, turn,
/// step, agent, and tool-call identifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryEvent {
    pub seq: i64,
    pub event_id: String,
    pub session_id: String,
    pub run_id: String,
    pub turn: u32,
    pub step: u32,
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub kind: TrajectoryEventKind,
    pub title: String,
    pub summary: String,
    pub is_error: bool,
    pub duration_ms: Option<u64>,
    pub payload: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubagentStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Interrupted,
}

impl SubagentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "completed" | "complete" | "done" | "success" => Self::Completed,
            "failed" | "error" => Self::Failed,
            "interrupted" | "cancelled" | "canceled" | "stopped" => Self::Interrupted,
            _ => Self::Running,
        }
    }
}

/// Tracked subagent node with parent-child relationship and live metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentNode {
    pub subagent_id: String,
    pub session_id: String,
    pub parent_agent_id: String,
    pub child_session_id: Option<String>,
    pub depth: u32,
    pub goal: String,
    pub model: Option<String>,
    pub status: SubagentStatus,
    pub current_operation: String,
    pub tool_count: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub duration_ms: Option<u64>,
    pub summary: Option<String>,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Capability classification for discovered Hermes slash commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CommandCapability {
    #[default]
    Available,
    RequiresTerminal,
    RequiresConfig,
    Unavailable,
}

impl CommandCapability {
    pub fn badge(self) -> Option<&'static str> {
        match self {
            Self::Available => None,
            Self::RequiresTerminal => Some("Terminal"),
            Self::RequiresConfig => Some("Config"),
            Self::Unavailable => Some("Unavailable"),
        }
    }
}

/// Discoverable slash command metadata surfaced in the composer popover and palette.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub category: String,
    pub description: String,
    pub args_hint: String,
    pub capability: CommandCapability,
    pub source: String,
}

/// Canonical built-in Hermes v0.21.0 + Rust harness slash-command baseline used
/// before or alongside live `commands.catalog` discovery from the Hermes server.
pub fn builtin_hermes_commands() -> Vec<SlashCommandInfo> {
    let entries: &[(&str, &[&str], &str, &str, &str, CommandCapability)] = &[
        ("/goal", &[], "Goals", "Create, pause, resume, inspect, or cancel a persistent background goal", "[text | status | pause | resume | cancel | complete]", CommandCapability::Available),
        ("/subgoal", &[], "Goals", "Add or manage criteria on the active goal", "[text | remove N | clear]", CommandCapability::Available),
        ("/plan", &[], "Work", "Draft an implementation plan before making changes", "[task]", CommandCapability::Available),
        ("/review", &[], "Work", "Spawn a code review pass on current changes", "[review instructions]", CommandCapability::Available),
        ("/diff", &[], "Git", "Inspect repository diff in the active working directory", "[staged|all|session] [--stat] [path...]", CommandCapability::Available),
        ("/worktree", &[], "Git", "Manage Git worktrees for isolated work", "[new [name]|list|prune]", CommandCapability::Available),
        ("/agents", &["/tasks"], "Agents", "List active subagents and background tasks", "", CommandCapability::Available),
        ("/bg", &[], "Agents", "Run a prompt as a background task", "<prompt>", CommandCapability::Available),
        ("/btw", &[], "Session", "Ask a quick side question without interrupting the main flow", "<question>", CommandCapability::Available),
        ("/steer", &[], "Session", "Steer an active running turn with new guidance", "<prompt>", CommandCapability::Available),
        ("/queue", &["/q"], "Session", "Queue a follow-up prompt behind the active turn", "<prompt>", CommandCapability::Available),
        ("/stop", &[], "Session", "Interrupt the active run immediately", "", CommandCapability::Available),
        ("/retry", &[], "Session", "Retry the last prompt", "", CommandCapability::Available),
        ("/undo", &[], "Session", "Undo the last N turns", "[N]", CommandCapability::Available),
        ("/branch", &["/fork"], "Session", "Branch the current conversation into a new session", "[name]", CommandCapability::Available),
        ("/compress", &["/compact"], "Session", "Compress conversation context to free token budget", "[focus topic | --preview]", CommandCapability::Available),
        ("/rollback", &[], "Session", "List or restore file edit checkpoints", "[number] [--all]", CommandCapability::Available),
        ("/status", &[], "Info", "Show active session, model, profile, and runtime status", "", CommandCapability::Available),
        ("/context", &["/ctx"], "Info", "Inspect context window token breakdown", "[all]", CommandCapability::Available),
        ("/usage", &[], "Info", "Show token usage and cost breakdown", "[reset [--force]]", CommandCapability::Available),
        ("/insights", &[], "Info", "Inspect usage analytics over recent days", "[days]", CommandCapability::Available),
        ("/model", &[], "Configuration", "Switch model or provider for the session", "[model] [--provider name]", CommandCapability::Available),
        ("/reasoning", &[], "Configuration", "Adjust reasoning effort level", "[none|low|medium|high|xhigh]", CommandCapability::Available),
        ("/fast", &[], "Configuration", "Toggle fast/priority inference mode", "[normal|fast|status]", CommandCapability::Available),
        ("/approvals", &[], "Configuration", "Set tool approval policy", "[manual|smart|off]", CommandCapability::Available),
        ("/yolo", &[], "Configuration", "Toggle auto-approve mode for tool calls", "", CommandCapability::Available),
        ("/tools", &[], "Tools & Skills", "Inspect or toggle active tools", "[list|enable|disable] [name...]", CommandCapability::Available),
        ("/skills", &[], "Tools & Skills", "Search, inspect, or audit skills", "[search|browse|inspect|audit]", CommandCapability::RequiresConfig),
        ("/memory", &[], "Tools & Skills", "Inspect or manage persistent memory entries", "[pending|approve|reject]", CommandCapability::Available),
        ("/learn", &[], "Tools & Skills", "Distill learnings from this session into skills/memory", "<what to learn from>", CommandCapability::Available),
        ("/init", &[], "Tools & Skills", "Initialize workspace instructions for the current directory", "[notes]", CommandCapability::Available),
        ("/loop", &["/proactive"], "Automation", "Run a recurring prompt loop", "[interval] <prompt> | status | stop", CommandCapability::Available),
        ("/cron", &[], "Automation", "Manage scheduled Hermes cron jobs", "[list|add|pause|resume|run|remove]", CommandCapability::RequiresTerminal),
        ("/reload-mcp", &["/reload_mcp"], "Tools & Skills", "Reload configured MCP servers", "", CommandCapability::Available),
        ("/reload-skills", &["/reload_skills"], "Tools & Skills", "Reload skill files from disk", "", CommandCapability::Available),
        ("/save", &[], "Session", "Export session transcript", "<json|md|html> [filename]", CommandCapability::Available),
        ("/title", &[], "Session", "Rename the current conversation", "[name]", CommandCapability::Available),
        ("/version", &["/v"], "Info", "Show installed Hermes Agent version", "", CommandCapability::Available),
        ("/help", &[], "Info", "List available Hermes commands", "[filter]", CommandCapability::Available),
        ("/clear", &[], "Terminal", "Clear terminal viewport (CLI/TUI only)", "", CommandCapability::RequiresTerminal),
        ("/snapshot", &["/snap"], "Terminal", "Create or manage CLI workspace snapshots", "[create|restore|prune]", CommandCapability::RequiresTerminal),
        ("/config", &[], "Configuration", "Open raw Hermes YAML configuration", "", CommandCapability::RequiresConfig),
        ("/browser", &[], "Tools & Skills", "Manage connected browser automation session", "[connect|disconnect|status]", CommandCapability::RequiresConfig),
        ("/plugins", &[], "Tools & Skills", "Manage Hermes plugins", "", CommandCapability::RequiresConfig),
        ("/approve", &[], "Messaging", "Messaging-gateway approval shortcut (use inline card in desktop)", "[session|always]", CommandCapability::Unavailable),
        ("/deny", &[], "Messaging", "Messaging-gateway denial shortcut (use inline card in desktop)", "[reason]", CommandCapability::Unavailable),
    ];

    entries
        .iter()
        .map(|(name, aliases, category, desc, args, cap)| SlashCommandInfo {
            name: (*name).to_string(),
            aliases: aliases.iter().map(|s| (*s).to_string()).collect(),
            category: (*category).to_string(),
            description: (*desc).to_string(),
            args_hint: (*args).to_string(),
            capability: *cap,
            source: "hermes-v0.21.0".to_string(),
        })
        .collect()
}

/// Parse the live Hermes v0.21.0 `commands.catalog` JSON-RPC response shape:
/// `{ categories: [{ name, pairs: [[cmd, desc], ...] }], pairs: [[cmd, desc]], canon: { alias: canon }, commands: { cmd: { argument_mode, desktop } } }`
/// and merge with built-in defaults without duplicates.
pub fn parse_hermes_commands_catalog(val: &Value) -> Vec<SlashCommandInfo> {
    let mut merged = builtin_hermes_commands();
    let mut by_name: std::collections::HashMap<String, usize> = merged
        .iter()
        .enumerate()
        .map(|(i, c)| (c.name.clone(), i))
        .collect();

    let meta_map = val.get("commands").and_then(Value::as_object);
    let canon_map = val.get("canon").and_then(Value::as_object);

    let mut register_pair = |raw_name: &str, raw_desc: &str, category: &str| {
        let trimmed = raw_name.trim();
        if trimmed.is_empty() {
            return;
        }
        let name = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            format!("/{trimmed}")
        };
        if let Some(canon) = canon_map
            .and_then(|m| m.get(&name))
            .and_then(Value::as_str)
        {
            if canon != name {
                if let Some(&idx) = by_name.get(canon) {
                    if !merged[idx].aliases.contains(&name) {
                        merged[idx].aliases.push(name);
                    }
                }
                return;
            }
        }

        let (description, args_hint) =
            if let Some((desc_part, usage_part)) = raw_desc.split_once("(usage:") {
                let cleaned = usage_part.trim().trim_end_matches(')').trim();
                let hint = cleaned
                    .strip_prefix(&name)
                    .unwrap_or(cleaned)
                    .trim()
                    .to_string();
                (desc_part.trim().to_string(), hint)
            } else {
                (raw_desc.trim().to_string(), String::new())
            };

        let meta = meta_map.and_then(|m| m.get(&name));
        let desktop = meta
            .and_then(|m| m.get("desktop"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let capability = match desktop {
            "terminal" => CommandCapability::RequiresTerminal,
            "settings" | "config" => CommandCapability::RequiresConfig,
            "messaging" | "gateway" | "hidden" => CommandCapability::Unavailable,
            _ => CommandCapability::Available,
        };

        if let Some(&idx) = by_name.get(&name) {
            if !description.is_empty() {
                merged[idx].description = description;
            }
            if !args_hint.is_empty() {
                merged[idx].args_hint = args_hint;
            }
            if !desktop.is_empty() {
                merged[idx].capability = capability;
            }
            merged[idx].source = "live-catalog".into();
        } else {
            let idx = merged.len();
            by_name.insert(name.clone(), idx);
            merged.push(SlashCommandInfo {
                name,
                aliases: Vec::new(),
                category: category.to_string(),
                description,
                args_hint,
                capability,
                source: "live-catalog".into(),
            });
        }
    };

    if let Some(cats) = val.get("categories").and_then(Value::as_array) {
        for cat in cats {
            let cat_name = cat
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Hermes");
            if let Some(pairs) = cat.get("pairs").and_then(Value::as_array) {
                for pair in pairs {
                    if let Some(arr) = pair.as_array() {
                        let cmd = arr.first().and_then(Value::as_str).unwrap_or("");
                        let desc = arr.get(1).and_then(Value::as_str).unwrap_or("");
                        register_pair(cmd, desc, cat_name);
                    }
                }
            }
        }
    }

    if let Some(pairs) = val.get("pairs").and_then(Value::as_array) {
        for pair in pairs {
            if let Some(arr) = pair.as_array() {
                let cmd = arr.first().and_then(Value::as_str).unwrap_or("");
                let desc = arr.get(1).and_then(Value::as_str).unwrap_or("");
                register_pair(cmd, desc, "Hermes");
            }
        }
    }

    if let Some(arr) = val
        .get("commands")
        .and_then(Value::as_array)
        .or_else(|| val.as_array())
    {
        for item in arr {
            let raw_name = item
                .get("name")
                .or_else(|| item.get("command"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let desc = item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            let cat = item
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("Hermes");
            register_pair(raw_name, desc, cat);
        }
    }

    merged
}

pub fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_hermes_commands_catalog_handles_v021_categories_canon_and_desktop_meta() {
        let payload = json!({
            "categories": [
                {
                    "name": "Session",
                    "pairs": [
                        ["/review", "Review current git changes (usage: /review [branch])"],
                        ["/clear", "Clear terminal screen"]
                    ]
                }
            ],
            "pairs": [
                ["/review", "Review current git changes (usage: /review [branch])"],
                ["/compact", "Compress context (alias for /compress)"]
            ],
            "canon": {
                "/review": "/review",
                "/compact": "/compress"
            },
            "commands": {
                "/review": { "argument_mode": "text", "desktop": null },
                "/clear": { "argument_mode": "none", "desktop": "terminal" }
            }
        });
        let cmds = parse_hermes_commands_catalog(&payload);
        let review = cmds.iter().find(|c| c.name == "/review").expect("/review parsed");
        assert_eq!(review.source, "live-catalog");
        assert_eq!(review.args_hint, "[branch]");
        assert_eq!(review.capability, CommandCapability::Available);
        let clear = cmds.iter().find(|c| c.name == "/clear").expect("/clear parsed");
        assert_eq!(clear.capability, CommandCapability::RequiresTerminal);
        assert!(cmds.iter().all(|c| c.name != "/compact"));
    }

    #[test]
    fn execution_state_maps_all_required_runtime_states_to_existing_face_assets() {
        assert_eq!(ExecutionState::Idle.to_face_state(None, None), BotFaceState::Idle);
        assert_eq!(ExecutionState::Thinking.to_face_state(None, None), BotFaceState::Thinking);
        assert_eq!(ExecutionState::Editing.to_face_state(Some("patch"), None), BotFaceState::Writing);
        assert_eq!(ExecutionState::Writing.to_face_state(None, None), BotFaceState::Streaming);
        assert_eq!(
            ExecutionState::ExecutingTools.to_face_state(Some("search_files"), None),
            BotFaceState::Searching
        );
        assert_eq!(
            ExecutionState::ExecutingTools.to_face_state(Some("terminal"), None),
            BotFaceState::Running
        );
        assert_eq!(
            ExecutionState::Waiting.to_face_state(Some("delegate_task"), None),
            BotFaceState::Waiting
        );
        assert_eq!(
            ExecutionState::Waiting.to_face_state(None, None),
            BotFaceState::Blocked
        );
        assert_eq!(ExecutionState::Completed.to_face_state(None, None), BotFaceState::Finished);
        assert_eq!(ExecutionState::Failed.to_face_state(None, None), BotFaceState::Failed);
    }
}
