use crate::backend::hermes_backend::{BackendInfo, HermesSupervisor};
use crate::backend::remote::{self, RemoteLogin};
use crate::backend::sqlite_store;
use crate::backend::terminal::TerminalManager;
use crate::backend::updater::{AppUpdater, UpdateStatus as AppUpdateStatus};
use crate::backend::workspace::{self, GitWorkspaceStatus, LinuxSystemStats, WorkspaceFileEntry};
use crate::harness::events::{
    builtin_hermes_commands, now_epoch_ms, ConversationMode, ExecutionState,
    SessionModeBinding, SlashCommandInfo, SubagentNode, SubagentStatus, TrajectoryEvent,
    TrajectoryEventKind, WorkProject,
};
use crate::harness::goal_worker::{
    parse_goal_slash_command, GoalCommand, GoalRuntimeSnapshot, GoalWorker,
};
use crate::harness::metrics::SessionLiveMetrics;
use crate::harness::ptc::is_ptc_tool;
use crate::harness::store::HarnessStore;
use crate::hermes::client::HermesClient;
use crate::hermes::types::*;
use crate::hermes::ws::HermesRealtimeSession;
use crate::state::persistence::{
    default_board, load_ui_prefs, save_ui_prefs, BotAvatarOverride, PersistedBoardState,
    UiPreferences,
};
use crate::theme::bot_face::{classify_hermes_activity, BotFaceState};
use crate::theme::components::slugify;
use crate::theme::tokens::{SandTheme, ThemeMode};
use gpui::{Context, EventEmitter};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppViewId {
    Chat,
    Boards,
    Skills,
    Cron,
    Memory,
    Gateway,
    Analytics,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightPaneTabId {
    Files,
    Terminal,
    Git,
    Agents,
    Trajectory,
    Bot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    None,
    Low,
    Medium,
    High,
    XHigh,
}

impl ReasoningEffort {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "none" => Self::None,
            "low" => Self::Low,
            "high" => Self::High,
            "xhigh" => Self::XHigh,
            _ => Self::Medium,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::XHigh,
            Self::XHigh => Self::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Local,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnSettlementOutcome {
    Success,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Default)]
struct StreamText {
    raw_content: String,
    raw_thinking: String,
    snapshot_thinking: Option<String>,
}

impl StreamText {
    fn structured_thinking(&self, snapshot: Option<&str>) -> Option<String> {
        let existing = merge_thinking(self.snapshot_thinking.as_deref(), Some(&self.raw_thinking));
        merge_thinking(existing.as_deref(), snapshot)
    }

    fn publish(&self, message: &mut HermesMessage) {
        let structured = self.structured_thinking(None);
        (message.thinking, message.content) =
            extract_thinking_from_content(&self.raw_content, structured.as_deref(), true);
    }

    fn apply_snapshot(
        &self,
        message: &mut HermesMessage,
        content: Option<&str>,
        thinking: Option<&str>,
    ) {
        let structured = self.structured_thinking(thinking);
        if let Some(content) = content.filter(|text| text.starts_with(&self.raw_content)) {
            // A final snapshot extending the raw stream already contains its inline thoughts,
            // including the completed version of any block that was still in progress.
            (message.thinking, message.content) =
                extract_thinking_from_content(content, structured.as_deref(), false);
            return;
        }
        let (stream_thinking, stream_content) =
            extract_thinking_from_content(&self.raw_content, structured.as_deref(), false);
        if let Some(content) = content {
            let (thinking, visible) =
                extract_thinking_from_content(content, stream_thinking.as_deref(), false);
            message.thinking = thinking;
            message.content = if visible.trim().is_empty() { stream_content } else { visible };
        } else {
            message.thinking = stream_thinking;
            message.content = stream_content;
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionChatState {
    pub messages: Vec<HermesMessage>,
    pub loading_history: bool,
    pub older_offset: Option<usize>,
    pub is_streaming: bool,
    pub has_streamed_text: bool,
    pub active_tool_name: Option<String>,
    pub live_status_text: String,
    pub awaiting_user_response: bool,
    pub last_settlement: Option<(TurnSettlementOutcome, Instant)>,
    pub todos: Vec<HermesTodoItem>,
    pub session_usage: Option<HermesSessionUsage>,
    stream_text: StreamText,
}

impl Default for SessionChatState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            loading_history: false,
            older_offset: None,
            is_streaming: false,
            has_streamed_text: false,
            active_tool_name: None,
            live_status_text: String::new(),
            awaiting_user_response: false,
            last_settlement: None,
            todos: Vec::new(),
            session_usage: None,
            stream_text: StreamText::default(),
        }
    }
}

impl SessionChatState {
    fn streaming_message_index(&mut self) -> usize {
        if let Some(index) = self.messages.iter().rposition(|m| m.streaming) {
            index
        } else {
            self.messages.push(HermesMessage::new_assistant(format!(
                "asst-{}", uuid::Uuid::new_v4().simple()
            )));
            self.messages.len() - 1
        }
    }

    fn append_stream_text(&mut self, text: &str, thinking: Option<&str>) {
        let index = self.streaming_message_index();
        self.stream_text.raw_content.push_str(text);
        if let Some(thinking) = thinking {
            self.stream_text.raw_thinking.push_str(thinking);
        }
        let message = &mut self.messages[index];
        self.stream_text.publish(message);
        self.has_streamed_text = !message.content.trim().is_empty();
    }

    fn apply_interim_snapshot(&mut self, content: &str, thinking: Option<&str>) {
        let (extracted, visible, incomplete) = split_thinking_content(content, thinking, true);
        if !visible.trim().is_empty() && !incomplete {
            self.settle_stream_text(Some(content), thinking);
            self.has_streamed_text = true;
            return;
        }

        // Reasoning-only snapshots and snapshots ending mid-tag do not end the message.
        let index = self.streaming_message_index();
        let extra = if incomplete || (!self.has_streamed_text && !content.is_empty()) {
            if !self.stream_text.raw_content.starts_with(content) {
                self.stream_text.raw_content = content.to_owned();
            }
            thinking
        } else {
            extracted.as_deref()
        };
        self.stream_text.snapshot_thinking =
            merge_thinking(self.stream_text.snapshot_thinking.as_deref(), extra);
        self.stream_text.publish(&mut self.messages[index]);
        self.has_streamed_text = !self.messages[index].content.trim().is_empty();
    }

    fn settle_stream_text(&mut self, content: Option<&str>, thinking: Option<&str>) -> Option<usize> {
        let stream = std::mem::take(&mut self.stream_text);
        let index = self.messages.iter().rposition(|m| m.streaming).or_else(|| {
            let (thought, visible) = extract_thinking_from_content(content.unwrap_or_default(), thinking, false);
            if thought.is_none() && visible.trim().is_empty() {
                return None;
            }
            // Some gateways repeat the interim snapshot when the turn completes.
            if let Some(index) = self.messages.len().checked_sub(1) {
                let last = &mut self.messages[index];
                let matches_last = last.content == visible && !visible.is_empty();
                let completes_interim = self.has_streamed_text && visible.is_empty() && !last.content.is_empty();
                if last.role == MessageRole::Assistant && (matches_last || completes_interim) {
                    last.thinking = merge_thinking(last.thinking.as_deref(), thought.as_deref());
                    return None;
                }
            }
            Some(self.streaming_message_index())
        })?;
        let message = &mut self.messages[index];
        stream.apply_snapshot(message, content, thinking);
        message.streaming = false;
        Some(index)
    }

    /// Single authoritative execution state driving inline chat/work animations and status.
    pub fn execution_state(&self) -> ExecutionState {
        if self.awaiting_user_response {
            return ExecutionState::Waiting;
        }
        if self.is_streaming {
            if let Some(tool) = self.active_tool_name.as_deref() {
                let classified = classify_hermes_activity(Some(tool), Some(&self.live_status_text));
                return match classified {
                    BotFaceState::Writing => ExecutionState::Editing,
                    BotFaceState::Waiting => ExecutionState::Waiting,
                    _ => ExecutionState::ExecutingTools,
                };
            }
            if self.has_streamed_text {
                return ExecutionState::Writing;
            }
            return ExecutionState::Thinking;
        }
        if let Some((outcome, settled_at)) = self.last_settlement {
            if settled_at.elapsed() < Duration::from_secs(8) {
                return match outcome {
                    TurnSettlementOutcome::Success => ExecutionState::Completed,
                    TurnSettlementOutcome::Error => ExecutionState::Failed,
                    TurnSettlementOutcome::Cancelled => ExecutionState::Idle,
                };
            }
        }
        ExecutionState::Idle
    }

    pub fn bot_state(&self) -> BotFaceState {
        self.execution_state()
            .to_face_state(self.active_tool_name.as_deref(), Some(&self.live_status_text))
    }
}

#[derive(Debug, Clone)]
pub struct AvatarEditorTarget {
    pub seed: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct AttachmentFile {
    pub name: String,
    #[allow(dead_code)]
    pub path: String,
    pub mime: String,
    pub bytes: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
struct ProfileCache {
    sessions: Vec<HermesSession>,
    chats: HashMap<String, SessionChatState>,
    soul_md: String,
    memory_md: String,
    user_md: String,
    active_model: Option<String>,
    active_provider: Option<String>,
    skills: Vec<HermesSkill>,
    toolsets: Vec<HermesToolset>,
    mcp_servers: Vec<HermesMcpServer>,
    cron_jobs: Vec<HermesCronJob>,
    loaded_views: HashSet<AppViewId>,
}

pub struct StateChanged;
impl EventEmitter<StateChanged> for AppState {}

pub struct AppState {
    // Shared Rust runtime & backend services
    pub tokio_rt: Arc<tokio::runtime::Runtime>,
    pub supervisor: HermesSupervisor,
    pub remote_login: RemoteLogin,
    pub terminals: TerminalManager,
    pub updater: AppUpdater,

    // Live clients
    pub client: Option<HermesClient>,
    pub realtime: Option<HermesRealtimeSession>,
    pub ws_generation: u64,
    pub profile_generation: u64,
    pub sidebar_version: u64,
    pub shell_version: u64,
    was_disconnected: bool,
    loaded_views: HashSet<AppViewId>,
    profile_cache: HashMap<String, ProfileCache>,
    synced_session_config: HashMap<String, (ReasoningEffort, bool, bool)>,
    pub bot_finished_at: HashMap<String, Instant>,

    // UI Shell state
    pub prefs: UiPreferences,
    pub active_view: AppViewId,
    pub sidebar_collapsed: bool,
    pub right_pane_open: bool,
    pub right_pane_tab: RightPaneTabId,
    pub command_palette_open: bool,
    pub model_picker_open: bool,
    pub avatar_editor_target: Option<AvatarEditorTarget>,
    pub toast: Option<(String, Instant)>,
    /// Retained for the legacy chat call sites; face animations are compositor-driven.

    // Onboarding & Connection
    pub setup_open: bool,
    pub initializing: bool,
    pub connection: HermesConnection,
    pub connection_status: ConnectionStatus,
    pub connection_detail: String,
    pub rust_backend: Option<BackendInfo>,
    pub hermes_status: Option<HermesStatus>,
    pub system_stats: Option<LinuxSystemStats>,
    pub bootstrap_ms: Option<u64>,
    pub restarting_backend: bool,
    pub refreshing_models: bool,

    // Profiles / Bots & Memory
    pub profiles: Vec<HermesProfile>,
    pub active_profile: String,
    pub profile_switching: bool,
    pub bot_avatars: HashMap<String, BotAvatarOverride>,
    pub soul_md: String,
    pub memory_md: String,
    pub user_md: String,
    pub learning_graph: Option<HermesLearningGraph>,

    // Boards & Kanban
    pub boards: Vec<HermesBoardSummary>,
    pub active_board_slug: String,
    pub session_boards: HashMap<String, String>,
    pub tasks: Vec<HermesKanbanTask>,

    // Sessions & Chat
    pub sessions: Vec<HermesSession>,
    pub active_session_id: Option<String>,
    pub chats: HashMap<String, SessionChatState>,
    pub live_ids: HashMap<String, String>,     // stored_id -> wire_id
    pub wire_to_stored: HashMap<String, String>, // wire_id -> stored_id
    pub draft_session_ids: HashSet<String>,
    pub attachments: Vec<AttachmentFile>,

    // Models & Inference controls
    pub models: Vec<HermesModelOption>,
    pub active_model: String,
    pub active_provider: String,
    pub reasoning_effort: ReasoningEffort,
    pub fast_mode: bool,
    pub yolo_mode: bool,

    // Capabilities & Ops data
    pub skills: Vec<HermesSkill>,
    pub toolsets: Vec<HermesToolset>,
    pub mcp_servers: Vec<HermesMcpServer>,
    pub cron_jobs: Vec<HermesCronJob>,
    pub messaging_platforms: Vec<HermesMessagingPlatform>,
    pub webhooks: Vec<HermesWebhook>,
    pub env_entries: Vec<HermesEnvEntry>,
    pub raw_config: String,
    pub server_logs: Vec<String>,
    pub diag_output: String,
    pub update_status: Option<AppUpdateStatus>,

    // Inspector state
    pub inspector_cwd: String,
    pub inspector_files: Vec<WorkspaceFileEntry>,
    pub inspector_file_error: Option<String>,
    pub inspector_preview_path: Option<String>,
    pub inspector_preview_content: String,
    pub inspector_git: Option<GitWorkspaceStatus>,
    pub inspector_git_loading: bool,
    pub inspector_git_error: Option<String>,
    inspector_files_gen: u64,
    inspector_preview_gen: u64,
    inspector_git_gen: u64,

    // DeepSeek-inspired Coding Agent Harness state (durable via harness.db)
    pub harness_store: Option<HarnessStore>,
    pub goal_worker: Option<GoalWorker>,
    pub draft_mode: ConversationMode,
    pub session_modes: HashMap<String, SessionModeBinding>,
    pub work_projects: Vec<WorkProject>,
    pub selected_work_project_id: Option<String>,
    pub project_picker_open: bool,
    pub slash_commands: Vec<SlashCommandInfo>,
    pub trajectory_events: HashMap<String, Vec<TrajectoryEvent>>,
    pub subagents: HashMap<String, Vec<SubagentNode>>,
    pub session_metrics: HashMap<String, SessionLiveMetrics>,
    pub inspect_trajectory_call_id: Option<String>,
    run_counters: HashMap<String, (String, u32, u32)>,
}

const HISTORY_PAGE_SIZE: usize = 48;

impl AppState {
    pub fn new(tokio_rt: Arc<tokio::runtime::Runtime>, _cx: &mut Context<Self>) -> Self {
        let prefs = load_ui_prefs();
        let supervisor = HermesSupervisor::new();
        let home_dir = dirs::home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/home".into());
        let harness_store = HarnessStore::open_default().ok();
        let (goal_worker, session_modes, work_projects) = if let Some(ref st) = harness_store {
            st.mark_running_subagents_interrupted_on_recovery();
            let gw = GoalWorker::new(st.clone());
            let modes = st
                .load_all_mode_bindings()
                .into_iter()
                .map(|b| (b.session_id.clone(), b))
                .collect();
            let projs = st.list_projects(None);
            (Some(gw), modes, projs)
        } else {
            (None, HashMap::new(), Vec::new())
        };
        let selected_work_project_id = work_projects.first().map(|p| p.id.clone());

        Self {
            tokio_rt,
            supervisor,
            remote_login: RemoteLogin::default(),
            terminals: TerminalManager::default(),
            updater: AppUpdater::new(),
            client: None,
            realtime: None,
            ws_generation: 0,
            profile_generation: 0,
            sidebar_version: 0,
            shell_version: 0,
            was_disconnected: false,
            loaded_views: HashSet::new(),
            profile_cache: HashMap::new(),
            synced_session_config: HashMap::new(),
            bot_finished_at: HashMap::new(),
            prefs,
            active_view: AppViewId::Chat,
            sidebar_collapsed: false,
            right_pane_open: false,
            right_pane_tab: RightPaneTabId::Files,
            command_palette_open: false,
            model_picker_open: false,
            avatar_editor_target: None,
            toast: None,
            setup_open: true,
            initializing: true,
            connection: HermesConnection::default(),
            connection_status: ConnectionStatus::Connecting,
            connection_detail: "Choose where Hermes runs".into(),
            rust_backend: None,
            hermes_status: None,
            system_stats: None,
            bootstrap_ms: None,
            restarting_backend: false,
            refreshing_models: false,
            profiles: vec![HermesProfile {
                name: "default".into(),
                active: true,
                default: true,
                description: Some("Primary Hermes Agent".into()),
                ..Default::default()
            }],
            active_profile: "default".into(),
            profile_switching: false,
            bot_avatars: HashMap::new(),
            soul_md: String::new(),
            memory_md: String::new(),
            user_md: String::new(),
            learning_graph: None,
            boards: vec![default_board()],
            active_board_slug: "all".into(),
            session_boards: HashMap::new(),
            tasks: Vec::new(),
            sessions: Vec::new(),
            active_session_id: None,
            chats: HashMap::new(),
            live_ids: HashMap::new(),
            wire_to_stored: HashMap::new(),
            draft_session_ids: HashSet::new(),
            attachments: Vec::new(),
            models: Vec::new(),
            active_model: "anthropic/claude-opus-4.6".into(),
            active_provider: "auto".into(),
            reasoning_effort: ReasoningEffort::Medium,
            fast_mode: false,
            yolo_mode: false,
            skills: Vec::new(),
            toolsets: Vec::new(),
            mcp_servers: Vec::new(),
            cron_jobs: Vec::new(),
            messaging_platforms: Vec::new(),
            webhooks: Vec::new(),
            env_entries: Vec::new(),
            raw_config: String::new(),
            server_logs: Vec::new(),
            diag_output: String::new(),
            update_status: None,
            inspector_cwd: home_dir,
            inspector_files: Vec::new(),
            inspector_file_error: None,
            inspector_preview_path: None,
            inspector_preview_content: String::new(),
            inspector_git: None,
            inspector_git_loading: false,
            inspector_git_error: None,
            inspector_files_gen: 0,
            inspector_preview_gen: 0,
            inspector_git_gen: 0,
            harness_store,
            goal_worker,
            draft_mode: ConversationMode::Chat,
            session_modes,
            work_projects,
            selected_work_project_id,
            project_picker_open: false,
            slash_commands: builtin_hermes_commands(),
            trajectory_events: HashMap::new(),
            subagents: HashMap::new(),
            session_metrics: HashMap::new(),
            inspect_trajectory_call_id: None,
            run_counters: HashMap::new(),
        }
    }

    pub fn theme_mode(&self) -> ThemeMode {
        self.prefs.theme
    }

    pub fn sand_theme(&self) -> SandTheme {
        let seed = self.active_bot_seed();
        let color = self
            .bot_avatars
            .get(&seed)
            .and_then(|a| a.color_id.as_deref());
        SandTheme::for_mode(self.prefs.theme, &seed, color)
    }

    pub fn active_bot_seed(&self) -> String {
        self.active_session()
            .and_then(|s| s.profile.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| self.active_profile.clone())
    }

    pub fn active_session(&self) -> Option<&HermesSession> {
        let id = self.active_session_id.as_deref()?;
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn active_chat(&self) -> SessionChatState {
        let key = self.active_session_id.as_deref().unwrap_or("new");
        self.chats.get(key).cloned().unwrap_or_default()
    }

    pub fn active_chat_ref(&self) -> Option<&SessionChatState> {
        let key = self.active_session_id.as_deref().unwrap_or("new");
        self.chats.get(key)
    }

    pub fn active_bot_state(&self) -> BotFaceState {
        self.active_chat_ref()
            .map(SessionChatState::bot_state)
            .unwrap_or(BotFaceState::Idle)
    }

    pub fn active_is_streaming(&self) -> bool {
        self.active_chat_ref().is_some_and(|c| c.is_streaming)
    }

    pub fn active_execution_state(&self) -> ExecutionState {
        self.active_chat_ref()
            .map(SessionChatState::execution_state)
            .unwrap_or(ExecutionState::Idle)
    }

    pub fn reduced_motion(&self) -> bool {
        self.prefs.reduced_motion
            || std::env::var("REDUCED_MOTION")
                .map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes"))
                .unwrap_or(false)
    }

    pub fn set_reduced_motion(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.prefs.reduced_motion = enabled;
        let prefs = self.prefs.clone();
        self.tokio_rt.spawn_blocking(move || {
            save_ui_prefs(&prefs);
        });
        self.bump_shell();
        cx.notify();
    }

    pub fn active_conversation_mode(&self) -> ConversationMode {
        if let Some(sid) = self.active_session_id.as_deref() {
            if let Some(binding) = self.session_modes.get(sid) {
                return binding.mode;
            }
            if let Some(sess) = self.sessions.iter().find(|s| s.id == sid) {
                if sess.cwd.as_deref().is_some_and(|c| !c.trim().is_empty())
                    && !self.draft_session_ids.contains(sid)
                {
                    return ConversationMode::Work;
                }
            }
        }
        self.draft_mode
    }

    pub fn is_active_mode_locked(&self) -> bool {
        let Some(sid) = self.active_session_id.as_deref() else {
            return false;
        };
        if self.session_modes.get(sid).is_some_and(|b| b.locked) {
            return true;
        }
        self.chats
            .get(sid)
            .is_some_and(|c| !c.messages.is_empty())
            || self
                .sessions
                .iter()
                .find(|s| s.id == sid)
                .and_then(|s| s.message_count)
                .unwrap_or(0)
                > 0
    }

    pub fn active_mode_binding(&self) -> Option<&SessionModeBinding> {
        let sid = self.active_session_id.as_deref()?;
        self.session_modes.get(sid)
    }

    pub fn active_work_project(&self) -> Option<&WorkProject> {
        if let Some(binding) = self.active_mode_binding() {
            if let Some(pid) = binding.project_id.as_deref() {
                if let Some(p) = self.work_projects.iter().find(|p| p.id == pid) {
                    return Some(p);
                }
            }
            if let Some(cwd) = binding.cwd.as_deref() {
                if let Some(p) = self.work_projects.iter().find(|p| p.cwd == cwd) {
                    return Some(p);
                }
            }
        }
        let sel = self.selected_work_project_id.as_deref()?;
        self.work_projects.iter().find(|p| p.id == sel)
    }

    pub fn active_work_cwd(&self) -> Option<String> {
        self.active_mode_binding()
            .and_then(|b| b.cwd.clone())
            .filter(|c| !c.trim().is_empty())
            .or_else(|| {
                self.active_session()
                    .and_then(|s| s.cwd.clone())
                    .filter(|c| !c.trim().is_empty())
            })
            .or_else(|| self.active_work_project().map(|p| p.cwd.clone()))
    }

    pub fn set_draft_mode(&mut self, mode: ConversationMode, cx: &mut Context<Self>) {
        if self.is_active_mode_locked() {
            self.show_toast(
                "Conversation mode is locked after the first message. Start a new conversation to switch modes.",
                cx,
            );
            return;
        }
        self.draft_mode = mode;
        if let Some(sid) = self.active_session_id.clone() {
            let proj = if mode == ConversationMode::Work {
                self.active_work_project().cloned()
            } else {
                None
            };
            let binding = SessionModeBinding {
                session_id: sid.clone(),
                mode,
                locked: false,
                project_id: proj.as_ref().map(|p| p.id.clone()),
                project_name: proj.as_ref().map(|p| p.name.clone()),
                cwd: proj.as_ref().map(|p| p.cwd.clone()),
                server_id: Some(self.connection.id.clone()),
                updated_at_ms: now_epoch_ms(),
            };
            if let Some(ref cwd) = binding.cwd {
                self.inspector_cwd = cwd.clone();
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == sid) {
                    s.cwd = Some(cwd.clone());
                }
            }
            self.persist_mode_binding(binding);
        }
        if mode == ConversationMode::Work {
            if let Some(p) = self.active_work_project().cloned() {
                self.inspector_cwd = p.cwd;
                self.refresh_inspector_git(cx);
            }
        }
        self.bump_shell();
        cx.notify();
    }

    pub fn open_project_picker(&mut self, cx: &mut Context<Self>) {
        self.project_picker_open = true;
        self.bump_shell();
        cx.notify();
    }

    pub fn close_project_picker(&mut self, cx: &mut Context<Self>) {
        self.project_picker_open = false;
        self.bump_shell();
        cx.notify();
    }

    pub fn select_work_project(&mut self, project_id: String, cx: &mut Context<Self>) {
        let Some(proj) = self.work_projects.iter().find(|p| p.id == project_id).cloned() else {
            return;
        };
        self.selected_work_project_id = Some(proj.id.clone());
        self.inspector_cwd = proj.cwd.clone();
        self.project_picker_open = false;

        if let Some(sid) = self.active_session_id.clone() {
            let locked = self.is_active_mode_locked();
            let binding = SessionModeBinding {
                session_id: sid.clone(),
                mode: ConversationMode::Work,
                locked,
                project_id: Some(proj.id.clone()),
                project_name: Some(proj.name.clone()),
                cwd: Some(proj.cwd.clone()),
                server_id: Some(self.connection.id.clone()),
                updated_at_ms: now_epoch_ms(),
            };
            self.persist_mode_binding(binding);
            if let Some(sess) = self.sessions.iter_mut().find(|s| s.id == sid) {
                sess.cwd = Some(proj.cwd.clone());
            }
            if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
                if let Some(live) = self.live_ids.get(&sid).cloned() {
                    let cwd_sync = proj.cwd.clone();
                    let rt = self.tokio_rt.clone();
                    cx.spawn(async move |_, _| {
                        let _ = rt.spawn(async move { ws.set_session_cwd(&live, &cwd_sync).await }).await;
                    })
                    .detach();
                }
            }
        }

        self.refresh_inspector_files(cx);
        self.refresh_inspector_git(cx);
        self.bump_sidebar();
        self.bump_shell();
        self.show_toast(format!("Project: {} ({})", proj.name, proj.cwd), cx);
        cx.notify();
    }

    pub fn create_and_select_work_project(
        &mut self,
        name: String,
        canonical_path: String,
        is_home: bool,
        cx: &mut Context<Self>,
    ) {
        let clean_name = if name.trim().is_empty() {
            std::path::Path::new(&canonical_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Workspace".into())
        } else {
            name.trim().to_string()
        };
        let now = now_epoch_ms();
        let server_id = self.connection.id.clone();

        let existing_id = self
            .work_projects
            .iter()
            .find(|p| p.server_id == server_id && p.cwd == canonical_path)
            .map(|p| p.id.clone());
        let project = WorkProject {
            id: existing_id.unwrap_or_else(|| format!("proj-{}", uuid::Uuid::new_v4().simple())),
            name: clean_name.clone(),
            cwd: canonical_path.clone(),
            server_id: server_id.clone(),
            is_home,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.work_projects
            .retain(|p| !(p.server_id == server_id && p.cwd == canonical_path));
        self.work_projects.insert(0, project.clone());

        if let Some(store) = self.harness_store.clone() {
            let p_save = project.clone();
            let rt = self.tokio_rt.clone();
            rt.spawn_blocking(move || {
                let _ = store.upsert_project(&p_save);
            });
        }
        // Best-effort sync to Hermes server projects.db
        if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
            let n = clean_name.clone();
            let c = canonical_path.clone();
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { ws.create_project(&n, None, Some("blue"), &c, "default").await })
                    .await;
            })
            .detach();
        }

        self.select_work_project(project.id, cx);
    }

    fn persist_mode_binding(&mut self, binding: SessionModeBinding) {
        self.session_modes
            .insert(binding.session_id.clone(), binding.clone());
        if let Some(store) = self.harness_store.clone() {
            let rt = self.tokio_rt.clone();
            rt.spawn_blocking(move || {
                let _ = store.save_mode_binding(&binding);
            });
        }
    }

    pub fn active_goal_snapshot(&self) -> Option<GoalRuntimeSnapshot> {
        let sid = self.active_session_id.as_deref()?;
        self.goal_worker.as_ref()?.snapshot_for_session(sid)
    }

    pub fn active_goal_events(&self) -> Vec<crate::harness::goal_worker::GoalEventRecord> {
        let Some(snap) = self.active_goal_snapshot() else {
            return Vec::new();
        };
        self.goal_worker
            .as_ref()
            .map(|gw| gw.goal_events(&snap.goal.goal_id))
            .unwrap_or_default()
    }

    pub fn refresh_server_projects(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) else {
            return;
        };
        let server_id = self.connection.id.clone();
        let rt = self.tokio_rt.clone();
        let store = self.harness_store.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(val)) = rt.spawn(async move { ws.projects_list().await }).await else {
                return;
            };
            let arr = val
                .get("projects")
                .and_then(Value::as_array)
                .or_else(|| val.as_array())
                .cloned()
                .unwrap_or_default();
            if arr.is_empty() {
                return;
            }
            let now = now_epoch_ms();
            let mut fetched = Vec::new();
            for item in arr {
                let cwd = item
                    .get("cwd")
                    .or_else(|| item.get("path"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if cwd.is_empty() {
                    continue;
                }
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| {
                        std::path::Path::new(cwd)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Project".into())
                    });
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|s| format!("srv-{server_id}-{s}"))
                    .unwrap_or_else(|| format!("proj-{}", uuid::Uuid::new_v4().simple()));
                fetched.push(WorkProject {
                    id,
                    name,
                    cwd: cwd.to_string(),
                    server_id: server_id.clone(),
                    is_home: false,
                    created_at_ms: now,
                    updated_at_ms: now,
                });
            }
            if fetched.is_empty() {
                return;
            }
            let _ = this.update(cx, |s, cx| {
                let mut changed = false;
                for proj in fetched {
                    if !s
                        .work_projects
                        .iter()
                        .any(|p| p.server_id == proj.server_id && p.cwd == proj.cwd)
                    {
                        if let Some(ref st) = store {
                            let st_c = st.clone();
                            let p_save = proj.clone();
                            s.tokio_rt.spawn_blocking(move || {
                                let _ = st_c.upsert_project(&p_save);
                            });
                        }
                        s.work_projects.push(proj);
                        changed = true;
                    }
                }
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn active_metrics(&self) -> SessionLiveMetrics {
        let key = self.active_session_id.as_deref().unwrap_or("new");
        self.session_metrics.get(key).cloned().unwrap_or_default()
    }

    pub fn active_trajectory(&self) -> Vec<TrajectoryEvent> {
        let Some(sid) = self.active_session_id.as_deref() else {
            return Vec::new();
        };
        self.trajectory_events.get(sid).cloned().unwrap_or_default()
    }

    pub fn active_subagents(&self) -> Vec<SubagentNode> {
        let Some(sid) = self.active_session_id.as_deref() else {
            return Vec::new();
        };
        self.subagents.get(sid).cloned().unwrap_or_default()
    }

    pub fn inspect_tool_in_trajectory(&mut self, tool_call_id: Option<String>, cx: &mut Context<Self>) {
        self.inspect_trajectory_call_id = tool_call_id;
        self.right_pane_open = true;
        self.right_pane_tab = RightPaneTabId::Trajectory;
        self.bump_shell();
        cx.notify();
    }

    pub fn interrupt_subagent(&mut self, subagent_id: String, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        if let Some(list) = self.subagents.get_mut(&sid) {
            if let Some(node) = list.iter_mut().find(|n| n.subagent_id == subagent_id) {
                node.status = SubagentStatus::Interrupted;
                node.current_operation = "Interrupted by user".into();
                node.updated_at_ms = now_epoch_ms();
                if let Some(store) = self.harness_store.clone() {
                    let node_save = node.clone();
                    self.tokio_rt.spawn_blocking(move || {
                        let _ = store.upsert_subagent(&node_save);
                    });
                }
            }
        }
        if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
            let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
            let sub_id = subagent_id.clone();
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { ws.interrupt_subagent(&live, &sub_id).await })
                    .await;
            })
            .detach();
        }
        self.show_toast(format!("Interrupted subagent {subagent_id}"), cx);
        cx.notify();
    }

    pub fn refresh_slash_commands(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) else {
            return;
        };
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(val)) = rt.spawn(async move { ws.commands_catalog().await }).await else {
                return;
            };
            let merged = crate::harness::events::parse_hermes_commands_catalog(&val);
            if merged.is_empty() {
                return;
            }
            let _ = this.update(cx, |s, cx| {
                s.slash_commands = merged;
                cx.notify();
            });
        })
        .detach();
    }

    fn next_run_turn_step(&mut self, session_id: &str, new_turn: bool, new_step: bool) -> (String, u32, u32) {
        let entry = self
            .run_counters
            .entry(session_id.to_string())
            .or_insert_with(|| (format!("run-{}", uuid::Uuid::new_v4().simple()), 0, 0));
        if new_turn {
            entry.0 = format!("run-{}", uuid::Uuid::new_v4().simple());
            entry.1 = entry.1.saturating_add(1);
            entry.2 = 1;
        } else if new_step {
            if entry.1 == 0 {
                entry.1 = 1;
            }
            entry.2 = entry.2.saturating_add(1);
        } else if entry.1 == 0 {
            entry.1 = 1;
            entry.2 = 1;
        }
        entry.clone()
    }

    #[allow(clippy::too_many_arguments)]
    fn record_trajectory_event(
        &mut self,
        session_id: &str,
        event_id: String,
        kind: TrajectoryEventKind,
        agent_id: String,
        parent_agent_id: Option<String>,
        tool_call_id: Option<String>,
        title: String,
        summary: String,
        is_error: bool,
        duration_ms: Option<u64>,
        payload: Value,
    ) {
        let new_turn = kind == TrajectoryEventKind::TurnStart;
        let new_step = matches!(
            kind,
            TrajectoryEventKind::ToolCallStart | TrajectoryEventKind::PtcExecution
        );
        let (run_id, turn, step) = self.next_run_turn_step(session_id, new_turn, new_step);
        let list = self.trajectory_events.entry(session_id.to_string()).or_default();
        if list.iter().any(|e| e.event_id == event_id) {
            return;
        }
        let next_seq = list.last().map(|e| e.seq + 1).unwrap_or(1);
        let ev = TrajectoryEvent {
            seq: next_seq,
            event_id,
            session_id: session_id.to_string(),
            run_id,
            turn,
            step,
            agent_id,
            parent_agent_id,
            tool_call_id,
            kind,
            title,
            summary,
            is_error,
            duration_ms,
            payload,
            created_at_ms: now_epoch_ms(),
        };
        list.push(ev.clone());
        if list.len() > 2000 {
            let drain = list.len() - 2000;
            list.drain(..drain);
        }
        if let Some(store) = self.harness_store.clone() {
            let rt = self.tokio_rt.clone();
            rt.spawn_blocking(move || {
                let _ = store.append_trajectory_event(&ev);
            });
        }
    }

    fn hydrate_session_harness_data(&mut self, sid: String, cx: &mut Context<Self>) {
        let Some(store) = self.harness_store.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let sid_l = sid.clone();
            let (traj, subs) = rt
                .spawn_blocking(move || {
                    let t = store.load_trajectory_events(&sid_l, 500);
                    let sub = store.load_subagents(&sid_l);
                    (t, sub)
                })
                .await
                .unwrap_or_default();
            let _ = this.update(cx, |s, cx| {
                if !traj.is_empty() && s.trajectory_events.get(&sid).map(|v| v.len()).unwrap_or(0) < traj.len() {
                    s.trajectory_events.insert(sid.clone(), traj);
                }
                if !subs.is_empty() && !s.subagents.contains_key(&sid) {
                    s.subagents.insert(sid.clone(), subs);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn pause_active_goal(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(worker) = self.goal_worker.clone() else {
            return;
        };
        match worker.pause_goal(&sid, None, "Paused by user") {
            Ok(snap) => {
                self.record_trajectory_event(
                    &sid,
                    format!("goal-pause-{}-{}", snap.goal.goal_id, snap.goal.revision),
                    TrajectoryEventKind::GoalChanged,
                    "lead".into(),
                    None,
                    None,
                    "Goal paused".into(),
                    snap.goal.objective.clone(),
                    false,
                    None,
                    json!({"goalId": snap.goal.goal_id, "phase": "paused", "revision": snap.goal.revision}),
                );
                if self.chats.get(&sid).is_some_and(|c| c.is_streaming) {
                    self.interrupt_stream(cx);
                }
                if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
                    let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
                    let rt = self.tokio_rt.clone();
                    cx.spawn(async move |_, _| {
                        let _ = rt.spawn(async move { ws.dispatch_command(&live, "/goal pause").await }).await;
                    })
                    .detach();
                }
                self.show_toast("Goal paused", cx);
                cx.notify();
            }
            Err(e) => self.show_toast(e, cx),
        }
    }

    pub fn resume_active_goal(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(worker) = self.goal_worker.clone() else {
            return;
        };
        match worker.resume_goal(&sid, None) {
            Ok(snap) => {
                self.record_trajectory_event(
                    &sid,
                    format!("goal-resume-{}-{}", snap.goal.goal_id, snap.goal.revision),
                    TrajectoryEventKind::GoalChanged,
                    "lead".into(),
                    None,
                    None,
                    "Goal resumed".into(),
                    snap.goal.objective.clone(),
                    false,
                    None,
                    json!({"goalId": snap.goal.goal_id, "phase": "active", "revision": snap.goal.revision}),
                );
                self.show_toast("Goal resumed", cx);
                self.dispatch_next_goal_round(sid, cx);
                cx.notify();
            }
            Err(e) => self.show_toast(e, cx),
        }
    }

    pub fn cancel_active_goal(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(worker) = self.goal_worker.clone() else {
            return;
        };
        match worker.cancel_goal(&sid, "Cancelled by user") {
            Ok(snap) => {
                self.record_trajectory_event(
                    &sid,
                    format!("goal-cancel-{}-{}", snap.goal.goal_id, snap.goal.revision),
                    TrajectoryEventKind::GoalChanged,
                    "lead".into(),
                    None,
                    None,
                    "Goal cancelled".into(),
                    snap.goal.objective.clone(),
                    false,
                    None,
                    json!({"goalId": snap.goal.goal_id, "phase": "cancelled", "revision": snap.goal.revision}),
                );
                if self.chats.get(&sid).is_some_and(|c| c.is_streaming) {
                    self.interrupt_stream(cx);
                }
                if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
                    let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
                    let rt = self.tokio_rt.clone();
                    cx.spawn(async move |_, _| {
                        let _ = rt.spawn(async move { ws.dispatch_command(&live, "/goal clear").await }).await;
                    })
                    .detach();
                }
                self.show_toast("Goal cancelled", cx);
                cx.notify();
            }
            Err(e) => self.show_toast(e, cx),
        }
    }

    pub fn complete_active_goal(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(worker) = self.goal_worker.clone() else {
            return;
        };
        match worker.complete_goal(&sid, Some("Completed by user".into())) {
            Ok(snap) => {
                self.record_trajectory_event(
                    &sid,
                    format!("goal-complete-{}-{}", snap.goal.goal_id, snap.goal.revision),
                    TrajectoryEventKind::GoalChanged,
                    "lead".into(),
                    None,
                    None,
                    "Goal completed".into(),
                    snap.goal.objective.clone(),
                    false,
                    None,
                    json!({"goalId": snap.goal.goal_id, "phase": "complete", "revision": snap.goal.revision}),
                );
                self.show_toast("Goal marked complete", cx);
                cx.notify();
            }
            Err(e) => self.show_toast(e, cx),
        }
    }

    fn dispatch_next_goal_round(&mut self, session_id: String, cx: &mut Context<Self>) {
        let Some(worker) = self.goal_worker.clone() else {
            return;
        };
        let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) else {
            return;
        };
        if self.chats.get(&session_id).is_some_and(|c| c.is_streaming) {
            return;
        }
        let tokens_now = self
            .session_metrics
            .get(&session_id)
            .and_then(|m| m.total_tokens().value);
        let Ok(Some(round)) = worker.try_begin_round(&session_id, tokens_now) else {
            cx.notify();
            return;
        };
        let Some(snap) = worker.snapshot_for_session(&session_id) else {
            return;
        };

        let prompt = if round == 1 {
            format!(
                "[Goal · Round 1/{}]\nObjective: {}\n\nWork autonomously toward this objective. Verify your changes and end your final message with `GOAL_COMPLETE` once every acceptance criterion is verified.",
                snap.goal.max_rounds, snap.goal.objective
            )
        } else {
            format!(
                "[Goal Continuation · Round {}/{}]\nObjective: {}\n\nContinue making verified progress toward the objective. When completely finished and verified, include `GOAL_COMPLETE` in your response.",
                round, snap.goal.max_rounds, snap.goal.objective
            )
        };

        let chat = self.chats.entry(session_id.clone()).or_default();
        chat.is_streaming = true;
        chat.has_streamed_text = false;
        chat.active_tool_name = None;
        chat.last_settlement = None;
        chat.live_status_text = format!("Goal round {round}/{}…", snap.goal.max_rounds);
        self.session_metrics
            .entry(session_id.clone())
            .or_default()
            .on_turn_start();

        self.record_trajectory_event(
            &session_id,
            format!("goal-round-{}-{round}", snap.goal.goal_id),
            TrajectoryEventKind::TurnStart,
            "lead".into(),
            None,
            None,
            format!("Goal round {round}/{}", snap.goal.max_rounds),
            snap.goal.objective.clone(),
            false,
            None,
            json!({"goalId": snap.goal.goal_id, "round": round, "maxRounds": snap.goal.max_rounds}),
        );

        let existing_live = self.live_ids.get(&session_id).cloned();
        let profile = self.active_profile.clone();
        let model = self.active_model.clone();
        let provider = self.active_provider.clone();
        let effort = self.reasoning_effort.as_str().to_string();
        let fast = self.fast_mode;
        let cwd = snap
            .goal
            .cwd
            .clone()
            .or_else(|| self.session_modes.get(&session_id).and_then(|b| b.cwd.clone()));
        let rt = self.tokio_rt.clone();
        let sid_bg = session_id.clone();

        cx.spawn(async move |this, cx| {
            let sid_inner = sid_bg.clone();
            let res: Result<String, String> = rt
                .spawn(async move {
                    let mut live_sid = existing_live;
                    if live_sid.is_none() && !sid_inner.starts_with("session_") {
                        if let Ok(resumed) = ws.resume_session(&sid_inner, &profile).await {
                            live_sid = resumed
                                .get("session_id")
                                .and_then(Value::as_str)
                                .map(String::from);
                        }
                    }
                    if live_sid.is_none() {
                        let mut params = json!({
                            "source": "desktop",
                            "profile": profile,
                            "model": model,
                            "reasoning_effort": effort,
                            "fast": fast,
                        });
                        if provider != "auto" && !provider.is_empty() {
                            params["provider"] = json!(provider);
                        }
                        if let Some(c) = cwd {
                            params["cwd"] = json!(c);
                        }
                        let created = ws.create_session(params).await?;
                        live_sid = created
                            .get("session_id")
                            .and_then(Value::as_str)
                            .map(String::from);
                    }
                    let live = live_sid.unwrap_or(sid_inner);
                    ws.submit_prompt(&live, &prompt).await?;
                    Ok(live)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let _ = this.update(cx, |s, cx| match res {
                Ok(live) => {
                    s.live_ids.insert(sid_bg.clone(), live.clone());
                    s.wire_to_stored.insert(live, sid_bg);
                    cx.notify();
                }
                Err(err) => {
                    if let Some(c) = s.chats.get_mut(&sid_bg) {
                        c.is_streaming = false;
                        c.last_settlement = Some((TurnSettlementOutcome::Error, Instant::now()));
                    }
                    if let Some(ref w) = s.goal_worker {
                        let _ = w.fail_goal(&sid_bg, &err);
                    }
                    s.show_toast(format!("Goal round failed: {err}"), cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn bot_recently_finished(&self, profile: &str) -> bool {
        if profile == self.active_profile && self.active_is_streaming() {
            return false;
        }
        let any_streaming_for_profile = self.sessions.iter().any(|s| {
            let p = s
                .profile
                .as_deref()
                .filter(|p| !p.is_empty())
                .unwrap_or(self.active_profile.as_str());
            p == profile && self.chats.get(&s.id).is_some_and(|c| c.is_streaming)
        });
        if any_streaming_for_profile {
            return false;
        }
        self.bot_finished_at
            .get(profile)
            .is_some_and(|at| at.elapsed() < Duration::from_secs(8))
    }

    pub fn bump_sidebar(&mut self) {
        self.sidebar_version = self.sidebar_version.wrapping_add(1);
    }

    pub fn bump_shell(&mut self) {
        self.shell_version = self.shell_version.wrapping_add(1);
    }

    pub fn set_active_view(&mut self, view: AppViewId, cx: &mut Context<Self>) {
        if self.active_view != view {
            self.active_view = view;
            self.bump_sidebar();
            self.bump_shell();
        }
        self.ensure_view_data(view, cx);
        cx.notify();
    }

    pub fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.prefs.theme = mode;
        let prefs = self.prefs.clone();
        self.tokio_rt.spawn_blocking(move || {
            save_ui_prefs(&prefs);
        });
        self.bump_sidebar();
        self.bump_shell();
        cx.notify();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let next = match self.prefs.theme {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.set_theme(next, cx);
    }

    pub fn show_toast(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some((msg.into(), Instant::now()));
        self.bump_shell();
        cx.notify();
    }

    pub fn persist_board_state(&self) {
        if !self.connection.is_local() {
            return;
        }
        let state = PersistedBoardState {
            boards: self.boards.clone(),
            session_boards: self.session_boards.clone(),
            tasks: self.tasks.clone(),
            bot_avatars: self.bot_avatars.clone(),
        };
        let home = self.supervisor.hermes_home().clone();
        self.tokio_rt.spawn_blocking(move || {
            if let Ok(val) = serde_json::to_value(&state) {
                let _ = sqlite_store::save_board_state(&home, &val);
            }
        });
    }

    pub fn bootstrap_saved_connection(&mut self, cx: &mut Context<Self>) {
        let rt = self.tokio_rt.clone();
        let saved = remote::connection_path()
            .ok()
            .and_then(|p| remote::read_connection(&p).ok().flatten())
            .and_then(|v| serde_json::from_value::<HermesConnection>(v).ok());

        let Some(mut saved) = saved else {
            self.initializing = false;
            self.setup_open = true;
            self.bump_shell();
            cx.notify();
            return;
        };

        self.connection = saved.clone();
        self.initializing = true;
        self.connection_status = ConnectionStatus::Connecting;
        self.connection_detail = format!("Connecting to {}…", saved.name);
        self.bump_sidebar();
        self.bump_shell();
        cx.notify();

        if saved.is_local() {
            // Hydrate local SQLite & stats off the UI thread while Hermes starts.
            self.hydrate_local_async(None, cx);
            self.start_local_connection(true, cx);
        } else {
            // Check token expiry and refresh if needed
            cx.spawn(async move |this, cx| {
                let needs_refresh = saved.refresh_token.is_some()
                    && saved.expires_at.unwrap_or(0.0) <= now_secs() + 60.0;
                if needs_refresh {
                    let url = saved.url.clone();
                    let rtok = saved.refresh_token.clone().unwrap_or_default();
                    let prov = saved
                        .auth_provider
                        .clone()
                        .unwrap_or_else(|| "basic".into());
                    let refreshed = rt
                        .spawn(async move { remote::refresh(url, rtok, prov).await })
                        .await;
                    if let Ok(Ok(toks)) = refreshed {
                        saved.token = toks
                            .get("access_token")
                            .and_then(Value::as_str)
                            .map(String::from);
                        if let Some(rt2) = toks.get("refresh_token").and_then(Value::as_str) {
                            saved.refresh_token = Some(rt2.to_string());
                        }
                        if let Some(exp) = toks.get("expires_at").and_then(Value::as_f64) {
                            saved.expires_at = Some(exp);
                        }
                    }
                }
                let _ = this.update(cx, |s, cx| {
                    s.connect_to_hermes(saved, true, cx);
                });
            })
            .detach();
        }
    }

    pub fn start_local_connection(&mut self, auto_close_setup: bool, cx: &mut Context<Self>) {
        let rt = self.tokio_rt.clone();
        let sup = self.supervisor.clone();
        self.connection_status = ConnectionStatus::Connecting;
        self.connection_detail = "Starting Hermes on this computer…".into();
        self.bump_sidebar();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let boot_res: Result<(String, String, BackendInfo), String> = rt
                .spawn(async move {
                    sup.start_in_background().await;
                    let info = sup.wait_until_ready(Duration::from_secs(28)).await?;
                    if let (Some(url), Some(token)) = (info.url.clone(), info.token.clone()) {
                        Ok((url, token, info))
                    } else {
                        Err(
                            "Hermes could not start. Install Hermes Agent, or connect to a server."
                                .into(),
                        )
                    }
                })
                .await
                .unwrap_or_else(|e| Err(format!("Supervisor task failed: {e}")));

            let _ = this.update(cx, |s, cx| match boot_res {
                Ok((url, token, info)) => {
                    s.rust_backend = Some(info);
                    let conn = HermesConnection {
                        id: "local-rust".into(),
                        name: "This computer".into(),
                        url,
                        mode: HermesServerMode::Dashboard,
                        auth_type: HermesAuthType::SessionToken,
                        token: Some(token),
                        ..Default::default()
                    };
                    s.connect_to_hermes(conn, auto_close_setup, cx);
                }
                Err(err) => {
                    eprintln!("[hermes-gpui] connection failed: {err}");
                    s.initializing = false;
                    s.connection_status = ConnectionStatus::Error;
                    s.connection_detail = err;
                    s.bump_sidebar();
                    s.bump_shell();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn connect_to_hermes(
        &mut self,
        conn: HermesConnection,
        auto_close_setup: bool,
        cx: &mut Context<Self>,
    ) {
        let rt = self.tokio_rt.clone();
        let profile = conn
            .default_profile
            .clone()
            .unwrap_or_else(|| "default".into());
        let sup = self.supervisor.clone();
        let prev_rt = self.realtime.clone();
        let is_local = conn.is_local();
        let conn_clone = conn.clone();

        self.connection_status = ConnectionStatus::Connecting;
        self.connection_detail = format!("Checking {}…", conn.name);
        self.bump_sidebar();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result: Result<
                (HermesClient, HermesRealtimeSession, HermesStatus, Vec<HermesProfile>),
                String,
            > = rt
                .spawn(async move {
                    let client = HermesClient::new(conn_clone.clone(), Some(profile.clone()));
                    let ws = HermesRealtimeSession::new(client.clone());
                    let ((status, profiles), ()) =
                        tokio::try_join!(client.test_connection(), ws.connect(Some(profile)))?;
                    let _ = ws.rpc("gateway.ping", json!({}), 10_000).await?;
                    if let Some(old) = prev_rt {
                        old.disconnect().await;
                    }
                    if !is_local {
                        sup.stop().await;
                    }
                    // Persist connection (strip ephemeral local token/port)
                    if let Ok(path) = remote::connection_path() {
                        let mut saved = conn_clone.clone();
                        if saved.is_local() {
                            saved.url = "http://127.0.0.1:9119".into();
                            saved.token = None;
                        }
                        if let Ok(val) = serde_json::to_value(&saved) {
                            let _ = remote::save_connection(&path, &val);
                        }
                    }
                    Ok((client, ws, status, profiles))
                })
                .await
                .unwrap_or_else(|e| Err(format!("Connection task failed: {e}")));

            let _ = this.update(cx, |s, cx| match result {
                Ok((client, ws, status, profiles)) => {
                    s.ws_generation = s.ws_generation.wrapping_add(1);
                    s.profile_generation = s.profile_generation.wrapping_add(1);
                    s.was_disconnected = false;
                    s.loaded_views.clear();
                    s.profile_cache.clear();
                    s.synced_session_config.clear();
                    let gen = s.ws_generation;
                    let mut rx = ws.subscribe();

                    s.connection = conn;
                    s.client = Some(client);
                    s.realtime = Some(ws);
                    if !profiles.is_empty() {
                        if let Some(active) = profiles.iter().find(|p| p.active) {
                            s.active_profile = active.name.clone();
                            if let Some(c) = &s.client {
                                c.set_profile(Some(active.name.clone()));
                            }
                        }
                        s.profiles = profiles;
                    }
                    s.hermes_status = Some(status);
                    s.connection_status = if s.connection.is_local() {
                        ConnectionStatus::Local
                    } else {
                        ConnectionStatus::Connected
                    };
                    s.connection_detail = s.connection.name.clone();
                    s.initializing = false;
                    if auto_close_setup {
                        s.setup_open = false;
                    }
                    s.live_ids.clear();
                    s.wire_to_stored.clear();
                    s.bump_sidebar();
                    s.bump_shell();

                    if s.connection.is_local() {
                        let prof = s.active_profile.clone();
                        s.hydrate_local_async(Some(prof), cx);
                    }
                    s.refresh_core_capabilities(false, cx);
                    if s.right_pane_open {
                        s.refresh_inspector_files(cx);
                    }

                    // Bridge tokio broadcast stream into GPUI event loop cleanly
                    let (ev_tx, mut ev_rx) =
                        tokio::sync::mpsc::unbounded_channel::<HermesStreamEvent>();
                    s.tokio_rt.spawn(async move {
                        while let Ok(ev) = rx.recv().await {
                            if ev_tx.send(ev).is_err() {
                                break;
                            }
                        }
                    });
                    cx.spawn(async move |this, cx| {
                        while let Some(first_ev) = ev_rx.recv().await {
                            // Drain any bursty tokens already queued so GPUI paints once per frame batch
                            let mut batch = Vec::with_capacity(8);
                            batch.push(first_ev);
                            while batch.len() < 64 {
                                match ev_rx.try_recv() {
                                    Ok(next_ev) => batch.push(next_ev),
                                    Err(_) => break,
                                }
                            }
                            let keep = this.update(cx, |s, cx| {
                                if s.ws_generation != gen {
                                    return false;
                                }
                                for ev in batch {
                                    s.handle_stream_event(ev, cx);
                                }
                                true
                            });
                            if !matches!(keep, Ok(true)) {
                                break;
                            }
                        }
                    })
                    .detach();

                    cx.notify();
                }
                Err(err) => {
                    eprintln!("[hermes-gpui] connection failed: {err}");
                    s.initializing = false;
                    s.connection_status = ConnectionStatus::Error;
                    s.connection_detail = err;
                    s.bump_sidebar();
                    s.bump_shell();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn hydrate_local_async(&mut self, profile: Option<String>, cx: &mut Context<Self>) {
        let home = self.supervisor.hermes_home().clone();
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok((boot, stats)) = rt
                .spawn_blocking(move || {
                    let target_home = crate::backend::profile_home(&home, profile.as_deref())
                        .unwrap_or_else(|_| home.clone());
                    let boot = sqlite_store::load_local_bootstrap(&target_home);
                    let stats = workspace::get_linux_system_stats();
                    (boot, stats)
                })
                .await
            else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.apply_local_bootstrap(boot, stats);
                s.bump_sidebar();
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_local_bootstrap(
        &mut self,
        boot: sqlite_store::LocalBootstrapPayload,
        stats: LinuxSystemStats,
    ) {
        self.bootstrap_ms = Some(boot.elapsed_ms);
        self.system_stats = Some(stats);

        if let Some(m) = boot.active_model {
            self.active_model = m;
        }
        if let Some(p) = boot.active_provider {
            self.active_provider = p;
        }
        if let Some(eff) = boot.reasoning_effort {
            self.reasoning_effort = ReasoningEffort::from_str_lossy(&eff);
        }
        if !boot.sessions.is_empty() && self.sessions.is_empty() {
            self.sessions = boot
                .sessions
                .into_iter()
                .map(|s| HermesSession {
                    id: s.id,
                    title: s.title,
                    model: s.model,
                    source: s.source,
                    started_at: s.started_at.map(|v| json!(v)),
                    updated_at: s.ended_at.or(s.started_at).map(|v| json!(v)),
                    message_count: Some(s.message_count),
                    tokens_used: Some(s.input_tokens + s.output_tokens),
                    preview: s.preview,
                    cwd: s.cwd,
                    pinned: s.pinned,
                    archived: s.archived,
                    ..Default::default()
                })
                .collect();
        }
        if !boot.models.is_empty() && self.models.is_empty() {
            self.models = boot
                .models
                .into_iter()
                .map(|m| HermesModelOption {
                    id: m.id,
                    name: Some(m.name),
                    provider: m.provider,
                    configured: m.configured,
                    context_length: m.context_length,
                })
                .collect();
        }
        if !boot.skills.is_empty() && self.skills.is_empty() {
            self.skills = boot
                .skills
                .into_iter()
                .map(|s| HermesSkill {
                    name: s.name,
                    category: Some(s.category),
                    path: Some(s.path),
                    description: Some(s.description),
                    enabled: s.enabled,
                    source: None,
                })
                .collect();
        }
        if self.soul_md.is_empty() && !boot.soul_md.is_empty() {
            self.soul_md = boot.soul_md;
        }
        if self.memory_md.is_empty() && !boot.memory_md.is_empty() {
            self.memory_md = boot.memory_md;
        }
        if self.user_md.is_empty() && !boot.user_md.is_empty() {
            self.user_md = boot.user_md;
        }

        if let Ok(bs) = serde_json::from_value::<PersistedBoardState>(boot.board_state) {
            if !bs.boards.is_empty() {
                self.boards = bs.boards;
            }
            if self.session_boards.is_empty() {
                self.session_boards = bs.session_boards;
            }
            if self.tasks.is_empty() {
                self.tasks = bs.tasks;
            }
            if self.bot_avatars.is_empty() {
                self.bot_avatars = bs.bot_avatars;
            }
        }
        for proj in boot.projects {
            if !self.boards.iter().any(|b| b.slug == proj.slug) {
                self.boards.push(HermesBoardSummary {
                    slug: proj.slug,
                    name: proj.name,
                    description: proj.description,
                    color: proj.color.or_else(|| Some("violet".into())),
                    workspace_path: proj.primary_path,
                    task_count: None,
                });
            }
        }
    }

    /// Independent, non-blocking refresh of core sidebar/chat capabilities.
    /// Each endpoint updates the UI immediately as it arrives so slow provider
    /// discovery or secondary plugins never stall sessions or bot switching.
    pub fn refresh_core_capabilities(&mut self, refresh_models: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        let active_prof = self.active_profile.clone();

        // 1. Sessions (single fast page, highest priority for sidebar responsiveness)
        {
            let c = client.clone();
            let rt = rt.clone();
            cx.spawn(async move |this, cx| {
                let Ok(Ok(rows)) = rt.spawn(async move { c.list_sessions(100).await }).await else {
                    return;
                };
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation != gen {
                        return;
                    }
                    let mut merged = rows;
                    for prev in &s.sessions {
                        if (prev.id.starts_with("session_")
                            || s.draft_session_ids.contains(&prev.id))
                            && !merged.iter().any(|r| r.id == prev.id)
                        {
                            merged.push(prev.clone());
                        }
                    }
                    s.sessions = merged;
                    s.bump_sidebar();
                    cx.notify();
                });
            })
            .detach();
        }

        // 2. Active model info + profile SOUL (fast, small payloads)
        {
            let c = client.clone();
            let rt = rt.clone();
            let prof = active_prof.clone();
            cx.spawn(async move |this, cx| {
                let Ok((model_info, soul)) = rt
                    .spawn(async move {
                        tokio::join!(c.get_model_info(), c.get_profile_soul(&prof))
                    })
                    .await
                else {
                    return;
                };
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation != gen {
                        return;
                    }
                    if let Ok((m, p)) = model_info {
                        if s.active_session_id.is_none() {
                            if let Some(m) = m.filter(|v| !v.is_empty()) {
                                s.active_model = m;
                            }
                            if let Some(p) = p.filter(|v| !v.is_empty()) {
                                s.active_provider = p;
                            }
                        }
                    }
                    if let Ok(soul_txt) = soul {
                        if !soul_txt.is_empty() || s.soul_md.is_empty() {
                            s.soul_md = soul_txt;
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }

        // 3. Model catalog (runs independently so upstream provider queries never block UI)
        if refresh_models || self.models.is_empty() {
            let c = client.clone();
            let rt = rt.clone();
            cx.spawn(async move |this, cx| {
                let Ok(Ok(ms)) = rt
                    .spawn(async move { c.get_model_options(refresh_models).await })
                    .await
                else {
                    return;
                };
                if ms.is_empty() {
                    return;
                }
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation != gen {
                        return;
                    }
                    s.models = ms;
                    cx.notify();
                });
            })
            .detach();
        }

        // 4. Refresh currently visible secondary view if needed
        let current_view = self.active_view;
        if current_view != AppViewId::Chat {
            self.loaded_views.remove(&current_view);
            self.ensure_view_data(current_view, cx);
        }
    }

    /// Lazy per-view hydration so startup and bot switching never flood the backend
    /// with endpoints for tabs the user hasn't opened.
    pub fn ensure_view_data(&mut self, view: AppViewId, cx: &mut Context<Self>) {
        if self.client.is_none() || self.loaded_views.contains(&view) {
            return;
        }
        self.loaded_views.insert(view);
        match view {
            AppViewId::Chat | AppViewId::Analytics => {}
            AppViewId::Boards => self.refresh_boards(cx),
            AppViewId::Skills => {
                self.refresh_skills(cx);
                self.refresh_toolsets(cx);
                self.refresh_mcp_servers(cx);
            }
            AppViewId::Cron => self.refresh_cron_jobs(cx),
            AppViewId::Memory => {
                if self.connection.is_local() {
                    let prof = self.active_profile.clone();
                    self.hydrate_local_async(Some(prof), cx);
                }
            }
            AppViewId::Gateway => self.refresh_gateway_data(cx),
            AppViewId::Settings => self.refresh_settings_data(cx),
        }
    }

    pub fn refresh_profiles(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let ws_gen = self.ws_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(ps)) = rt.spawn(async move { c.list_profiles().await }).await else {
                return;
            };
            if ps.is_empty() {
                return;
            }
            let _ = this.update(cx, |s, cx| {
                if s.ws_generation != ws_gen {
                    return;
                }
                s.profiles = ps;
                s.bump_sidebar();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_boards(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(b_list)) = rt.spawn(async move {
                let boards = c.list_kanban_boards().await?;
                let slugs: Vec<String> = boards.iter().map(|b| b.slug.clone()).collect();
                let task_futs = slugs.into_iter().map(|slug| {
                    let c2 = c.clone();
                    async move { c2.get_kanban_board(&slug).await.unwrap_or_default() }
                });
                let all_tasks: Vec<HermesKanbanTask> =
                    futures_util::future::join_all(task_futs)
                        .await
                        .into_iter()
                        .flatten()
                        .collect();
                Ok::<(Vec<HermesBoardSummary>, Vec<HermesKanbanTask>), String>((boards, all_tasks))
            }).await else {
                return;
            };
            let (boards, board_tasks) = b_list;
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                for b in boards {
                    if let Some(existing) = s.boards.iter_mut().find(|x| x.slug == b.slug) {
                        *existing = b;
                    } else {
                        s.boards.push(b);
                    }
                }
                if !board_tasks.is_empty() {
                    let mut keep_local: Vec<HermesKanbanTask> = s
                        .tasks
                        .iter()
                        .filter(|t| t.id.starts_with("task_"))
                        .cloned()
                        .collect();
                    keep_local.extend(board_tasks);
                    s.tasks = keep_local;
                }
                s.bump_sidebar();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_skills(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(sk)) = rt.spawn(async move { c.list_skills().await }).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.skills = sk;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_toolsets(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(ts)) = rt.spawn(async move { c.list_toolsets().await }).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.toolsets = ts;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_mcp_servers(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(mcps)) = rt.spawn(async move { c.list_mcp_servers().await }).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.mcp_servers = mcps;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_cron_jobs(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(crons)) = rt.spawn(async move { c.list_cron_jobs().await }).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.cron_jobs = crons;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_gateway_data(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok((status, platforms, webhooks)) = rt
                .spawn(async move {
                    tokio::join!(
                        c.get_status(),
                        c.list_messaging_platforms(),
                        c.list_webhooks(),
                    )
                })
                .await
            else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                if let Ok(st) = status {
                    s.hermes_status = Some(st);
                }
                if let Ok(plats) = platforms {
                    s.messaging_platforms = plats;
                }
                if let Ok(hooks) = webhooks {
                    s.webhooks = hooks;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_settings_data(&mut self, cx: &mut Context<Self>) {
        self.refresh_profiles(cx);
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok((raw_cfg, env_list, logs_list)) = rt
                .spawn(async move {
                    tokio::join!(c.get_raw_config(), c.list_env(), c.get_logs(160))
                })
                .await
            else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                if let Ok(cfg_txt) = raw_cfg {
                    if !cfg_txt.is_empty() {
                        s.raw_config = cfg_txt;
                    }
                }
                if let Ok(envs) = env_list {
                    s.env_entries = envs;
                }
                if let Ok(logs) = logs_list {
                    s.server_logs = logs
                        .into_iter()
                        .map(|e| {
                            let ts = e.timestamp.unwrap_or_default();
                            let lvl = e.level.unwrap_or_else(|| "INFO".into());
                            if ts.is_empty() {
                                format!("[{lvl}] {}", e.message)
                            } else {
                                format!("{ts} [{lvl}] {}", e.message)
                            }
                        })
                        .collect();
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_env(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(envs)) = rt.spawn(async move { c.list_env().await }).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.env_entries = envs;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_logs(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(logs)) = rt.spawn(async move { c.get_logs(160).await }).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.server_logs = logs
                    .into_iter()
                    .map(|e| {
                        let ts = e.timestamp.unwrap_or_default();
                        let lvl = e.level.unwrap_or_else(|| "INFO".into());
                        if ts.is_empty() {
                            format!("[{lvl}] {}", e.message)
                        } else {
                            format!("{ts} [{lvl}] {}", e.message)
                        }
                    })
                    .collect();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn toggle_skill_optimistic(&mut self, name: String, enabled: bool, cx: &mut Context<Self>) {
        if let Some(sk) = self.skills.iter_mut().find(|s| s.name == name) {
            sk.enabled = enabled;
        }
        cx.notify();
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        let name_c = name.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.toggle_skill(&name_c, enabled).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        if let Some(sk) = s.skills.iter_mut().find(|x| x.name == name) {
                            sk.enabled = !enabled;
                        }
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn toggle_toolset_optimistic(
        &mut self,
        name: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(ts) = self.toolsets.iter_mut().find(|t| t.name == name) {
            ts.enabled = enabled;
        }
        cx.notify();
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        let name_c = name.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.toggle_toolset(&name_c, enabled).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        if let Some(ts) = s.toolsets.iter_mut().find(|x| x.name == name) {
                            ts.enabled = !enabled;
                        }
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn toggle_mcp_optimistic(&mut self, name: String, enabled: bool, cx: &mut Context<Self>) {
        if let Some(srv) = self.mcp_servers.iter_mut().find(|m| m.name == name) {
            srv.enabled = Some(enabled);
        }
        cx.notify();
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        let name_c = name.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.toggle_mcp_server(&name_c, enabled).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        if let Some(srv) = s.mcp_servers.iter_mut().find(|x| x.name == name) {
                            srv.enabled = Some(!enabled);
                        }
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn delete_mcp_optimistic(&mut self, name: String, cx: &mut Context<Self>) {
        let prev = self.mcp_servers.clone();
        self.mcp_servers.retain(|m| m.name != name);
        self.show_toast(format!("Removed MCP \"{name}\""), cx);
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.delete_mcp_server(&name).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        s.mcp_servers = prev;
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn toggle_cron_pause_optimistic(
        &mut self,
        id: String,
        pause: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(job) = self.cron_jobs.iter_mut().find(|j| j.id == id) {
            job.paused = pause;
            job.enabled = Some(!pause);
        }
        cx.notify();
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        let id_c = id.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    if pause {
                        c.pause_cron_job(&id_c).await
                    } else {
                        c.resume_cron_job(&id_c).await
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        if let Some(job) = s.cron_jobs.iter_mut().find(|j| j.id == id) {
                            job.paused = !pause;
                            job.enabled = Some(pause);
                        }
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn delete_cron_optimistic(&mut self, id: String, cx: &mut Context<Self>) {
        let prev = self.cron_jobs.clone();
        self.cron_jobs.retain(|j| j.id != id);
        self.show_toast("Deleted cron job", cx);
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.delete_cron_job(&id).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        s.cron_jobs = prev;
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn delete_webhook_optimistic(&mut self, id: String, cx: &mut Context<Self>) {
        let prev = self.webhooks.clone();
        self.webhooks.retain(|w| w.id != id);
        self.show_toast("Webhook deleted", cx);
        let Some(c) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.delete_webhook(&id).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            if let Err(err) = res {
                let _ = this.update(cx, |s, cx| {
                    if s.profile_generation == gen {
                        s.webhooks = prev;
                    }
                    s.show_toast(err, cx);
                });
            }
        })
        .detach();
    }

    pub fn refresh_models(&mut self, cx: &mut Context<Self>) {
        if self.refreshing_models {
            return;
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        self.refreshing_models = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.get_model_options(true).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| {
                s.refreshing_models = false;
                match res {
                    Ok(ms) => {
                        let count = ms.len();
                        if !ms.is_empty() {
                            s.models = ms;
                        }
                        s.show_toast(format!("Refreshed {count} models across providers"), cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn restart_backend(&mut self, cx: &mut Context<Self>) {
        if self.restarting_backend {
            return;
        }
        self.restarting_backend = true;
        self.show_toast("Restarting Hermes backend…", cx);
        let rt = self.tokio_rt.clone();
        let sup = self.supervisor.clone();
        let is_local = self.connection.is_local();
        let client = self.client.clone();
        let conn = self.connection.clone();

        cx.spawn(async move |this, cx| {
            if is_local {
                let _ = rt
                    .spawn(async move {
                        sup.restart().await;
                    })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    s.restarting_backend = false;
                    s.start_local_connection(false, cx);
                });
            } else {
                let _ = rt
                    .spawn(async move {
                        if let Some(c) = client {
                            let _ = c.gateway_action("restart").await;
                            let _ = c.rescan_dashboard_plugins().await;
                        }
                        tokio::time::sleep(Duration::from_millis(1200)).await;
                    })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    s.restarting_backend = false;
                    s.connect_to_hermes(conn, false, cx);
                });
            }
        })
        .detach();
    }

    pub fn switch_profile(&mut self, name: String, cx: &mut Context<Self>) {
        if name == self.active_profile {
            return;
        }
        let Some(client) = self.client.clone() else {
            self.show_toast("Connect to Hermes to switch bots", cx);
            return;
        };

        // Snapshot current profile into cache so switching back is instant (<1ms)
        let prev_profile = self.active_profile.clone();
        self.profile_cache.insert(
            prev_profile.clone(),
            ProfileCache {
                sessions: std::mem::take(&mut self.sessions),
                chats: std::mem::take(&mut self.chats),
                soul_md: std::mem::take(&mut self.soul_md),
                memory_md: std::mem::take(&mut self.memory_md),
                user_md: std::mem::take(&mut self.user_md),
                active_model: Some(self.active_model.clone()),
                active_provider: Some(self.active_provider.clone()),
                skills: std::mem::take(&mut self.skills),
                toolsets: std::mem::take(&mut self.toolsets),
                mcp_servers: std::mem::take(&mut self.mcp_servers),
                cron_jobs: std::mem::take(&mut self.cron_jobs),
                loaded_views: std::mem::take(&mut self.loaded_views),
            },
        );

        // Optimistically switch UI on frame 0
        self.profile_generation = self.profile_generation.wrapping_add(1);
        let gen = self.profile_generation;
        self.active_profile = name.clone();
        self.active_session_id = None;
        self.active_view = AppViewId::Chat;
        self.live_ids.clear();
        self.wire_to_stored.clear();
        self.synced_session_config.clear();
        self.profile_switching = true;
        for p in &mut self.profiles {
            p.active = p.name == name;
        }
        if let Some(cached) = self.profile_cache.remove(&name) {
            self.sessions = cached.sessions;
            self.chats = cached.chats;
            self.soul_md = cached.soul_md;
            self.memory_md = cached.memory_md;
            self.user_md = cached.user_md;
            if let Some(m) = cached.active_model {
                self.active_model = m;
            }
            if let Some(p) = cached.active_provider {
                self.active_provider = p;
            }
            self.skills = cached.skills;
            self.toolsets = cached.toolsets;
            self.mcp_servers = cached.mcp_servers;
            self.cron_jobs = cached.cron_jobs;
            self.loaded_views = cached.loaded_views;
        }
        client.set_profile(Some(name.clone()));
        self.bump_sidebar();
        self.bump_shell();
        cx.notify();

        // Hydrate local SQLite/files off the UI thread immediately
        if self.connection.is_local() {
            self.hydrate_local_async(Some(name.clone()), cx);
        }

        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let name_clone = name.clone();
            let res = rt
                .spawn(async move { client.set_active_profile(&name_clone).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                s.profile_switching = false;
                match res {
                    Ok(_) => {
                        s.loaded_views.clear();
                        s.refresh_core_capabilities(false, cx);
                        let label = if name == "default" { "Hermes" } else { &name };
                        s.show_toast(format!("Switched to {label}"), cx);
                    }
                    Err(err) => {
                        // Roll back to previous profile if server rejected the switch
                        if let Some(prev_cache) = s.profile_cache.remove(&prev_profile) {
                            s.sessions = prev_cache.sessions;
                            s.chats = prev_cache.chats;
                            s.soul_md = prev_cache.soul_md;
                            s.memory_md = prev_cache.memory_md;
                            s.user_md = prev_cache.user_md;
                            s.skills = prev_cache.skills;
                            s.toolsets = prev_cache.toolsets;
                            s.mcp_servers = prev_cache.mcp_servers;
                            s.cron_jobs = prev_cache.cron_jobs;
                            s.loaded_views = prev_cache.loaded_views;
                        }
                        s.active_profile = prev_profile.clone();
                        for p in &mut s.profiles {
                            p.active = p.name == prev_profile;
                        }
                        if let Some(c) = &s.client {
                            c.set_profile(Some(prev_profile));
                        }
                        s.bump_sidebar();
                        s.show_toast(err, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn select_session(&mut self, session_id: Option<String>, cx: &mut Context<Self>) {
        self.active_session_id = session_id.clone();
        self.active_view = AppViewId::Chat;

        if let Some(ref sid) = session_id {
            if let Some(binding) = self.session_modes.get(sid).cloned() {
                if let Some(cwd) = binding.cwd.filter(|c| !c.is_empty()) {
                    self.inspector_cwd = cwd;
                }
                if let Some(pid) = binding.project_id {
                    self.selected_work_project_id = Some(pid);
                }
            }
            if let Some(sess) = self.sessions.iter().find(|s| &s.id == sid) {
                if let Some(m) = sess.model.clone() {
                    self.active_model = m;
                }
                if let Some(p) = sess.provider.clone() {
                    self.active_provider = p;
                }
                if let Some(cwd) = sess.cwd.clone().filter(|c| !c.is_empty()) {
                    self.inspector_cwd = cwd;
                }
            }
            self.hydrate_session_harness_data(sid.clone(), cx);
            if self.active_conversation_mode() == ConversationMode::Work {
                self.refresh_inspector_git(cx);
            }
        }
        self.bump_sidebar();
        self.bump_shell();
        cx.notify();

        let Some(sid) = session_id else {
            return;
        };

        let chat_entry = self.chats.entry(sid.clone()).or_default();
        if chat_entry.is_streaming || chat_entry.loading_history {
            return;
        }
        let had_messages = !chat_entry.messages.is_empty();
        if !had_messages {
            chat_entry.loading_history = true;
        }

        let is_local = self.connection.is_local();
        let home = self.supervisor.hermes_home().clone();
        let profile = self.active_profile.clone();
        let gen = self.profile_generation;
        let client = self.client.clone();
        let realtime = self.realtime.clone();
        let already_live = self.live_ids.contains_key(&sid);
        let rt = self.tokio_rt.clone();

        cx.spawn(async move |this, cx| {
            // Instant local SQLite read on a blocking thread first
            if is_local && !had_messages {
                let sid_l = sid.clone();
                let prof_l = profile.clone();
                let local_rows = rt
                    .spawn_blocking(move || {
                        let target_home =
                            crate::backend::profile_home(&home, Some(&prof_l)).unwrap_or(home);
                        sqlite_store::load_local_session_messages(
                            &target_home,
                            &sid_l,
                            HISTORY_PAGE_SIZE,
                        )
                    })
                    .await
                    .unwrap_or_default();
                if !local_rows.is_empty() {
                    let _ = this.update(cx, |s, cx| {
                        if s.profile_generation != gen {
                            return;
                        }
                        let entry = s.chats.entry(sid.clone()).or_default();
                        if entry.messages.is_empty() {
                            entry.messages = local_rows
                                .into_iter()
                                .map(|r| HermesMessage {
                                    id: r.id,
                                    role: match r.role.as_str() {
                                        "user" => MessageRole::User,
                                        "system" => MessageRole::System,
                                        "tool" => MessageRole::Tool,
                                        _ => MessageRole::Assistant,
                                    },
                                    content: r.content,
                                    thinking: r.thinking,
                                    tool_name: None,
                                    timestamp: r.timestamp,
                                    model: r.model,
                                    tool_calls: Vec::new(),
                                    approval: None,
                                    clarify: None,
                                    streaming: false,
                                })
                                .collect();
                            cx.notify();
                        }
                    });
                }
            }

            // Authoritative gateway history + resume WS session concurrently
            let sid_r = sid.clone();
            let prof_r = profile.clone();
            let (remote_msgs, wire_id) = rt
                .spawn(async move {
                    let msgs_fut = async {
                        if let Some(c) = client {
                            c.get_session_messages(&sid_r, HISTORY_PAGE_SIZE, 0)
                                .await
                                .ok()
                        } else {
                            None
                        }
                    };
                    let wid_fut = async {
                        if !already_live {
                            if let Some(ws) = realtime {
                                if ws.is_connected() {
                                    return ws
                                        .resume_session(&sid_r, &prof_r)
                                        .await
                                        .ok()
                                        .and_then(|v| {
                                            v.get("session_id")
                                                .and_then(Value::as_str)
                                                .map(String::from)
                                        });
                                }
                            }
                        }
                        None
                    };
                    tokio::join!(msgs_fut, wid_fut)
                })
                .await
                .unwrap_or((None, None));

            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                if let Some(wid) = wire_id {
                    s.live_ids.insert(sid.clone(), wid.clone());
                    s.wire_to_stored.insert(wid, sid.clone());
                }
                let entry = s.chats.entry(sid.clone()).or_default();
                entry.loading_history = false;
                if let Some(msgs) = remote_msgs {
                    if !msgs.is_empty() && !entry.is_streaming {
                        let has_more = msgs.len() >= HISTORY_PAGE_SIZE;
                        entry.messages = msgs;
                        if has_more && entry.older_offset.is_none() {
                            entry.older_offset = Some(HISTORY_PAGE_SIZE);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn load_older_messages(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(offset) = self.chats.get(&sid).and_then(|c| c.older_offset) else {
            return;
        };
        if let Some(c) = self.chats.get_mut(&sid) {
            if c.loading_history {
                return;
            }
            c.loading_history = true;
        }
        cx.notify();
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let sid2 = sid.clone();
            let res = rt
                .spawn(async move {
                    client
                        .get_session_messages(&sid2, HISTORY_PAGE_SIZE, offset)
                        .await
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| {
                let entry = s.chats.entry(sid.clone()).or_default();
                entry.loading_history = false;
                match res {
                    Ok(older) => {
                        let next_off = if older.len() >= HISTORY_PAGE_SIZE {
                            Some(offset + older.len())
                        } else {
                            None
                        };
                        let existing_ids: HashSet<String> =
                            entry.messages.iter().map(|m| m.id.clone()).collect();
                        let mut prepended: Vec<HermesMessage> = older
                            .into_iter()
                            .filter(|m| !existing_ids.contains(&m.id))
                            .collect();
                        prepended.append(&mut entry.messages);
                        entry.messages = prepended;
                        entry.older_offset = next_off;
                    }
                    Err(e) => s.show_toast(e, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn create_new_session(
        &mut self,
        title: Option<String>,
        board_slug: Option<String>,
        cwd: Option<String>,
        model: Option<String>,
        provider: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let b_slug = board_slug.unwrap_or_else(|| {
            if self.active_board_slug != "all" {
                self.active_board_slug.clone()
            } else {
                "default".into()
            }
        });
        let board_cwd = self
            .boards
            .iter()
            .find(|b| b.slug == b_slug)
            .and_then(|b| b.workspace_path.clone());
        let resolved_cwd = cwd.or(board_cwd).filter(|s| !s.is_empty());
        let resolved_model = model.unwrap_or_else(|| self.active_model.clone());
        let resolved_provider = provider.unwrap_or_else(|| self.active_provider.clone());
        let profile = self.active_profile.clone();
        let effort = self.reasoning_effort.as_str().to_string();
        let fast = self.fast_mode;
        let realtime = self.realtime.clone();
        let rt = self.tokio_rt.clone();
        let gen = self.profile_generation;

        let resolved_cwd = resolved_cwd.or_else(|| {
            if self.draft_mode == ConversationMode::Work {
                self.active_work_project().map(|p| p.cwd.clone())
            } else {
                None
            }
        });
        let initial_mode = self.draft_mode;
        let initial_project = if initial_mode == ConversationMode::Work {
            self.active_work_project().cloned()
        } else {
            None
        };

        // Instant optimistic draft session on frame 0 (0ms latency)
        let local_id = format!("session_{}", uuid::Uuid::new_v4().simple());
        let sess = HermesSession {
            id: local_id.clone(),
            title: Some(title.clone().unwrap_or_else(|| {
                if initial_mode == ConversationMode::Work {
                    "New Work".into()
                } else {
                    "New Chat".into()
                }
            })),
            profile: Some(profile.clone()),
            model: Some(resolved_model.clone()),
            provider: Some(resolved_provider.clone()),
            source: Some("desktop".into()),
            cwd: resolved_cwd.clone(),
            board_slug: Some(b_slug.clone()),
            message_count: Some(0),
            created_at: Some(json!(now_secs())),
            updated_at: Some(json!(now_secs())),
            ..Default::default()
        };
        self.draft_session_ids.insert(local_id.clone());
        self.sessions.retain(|x| x.id != local_id);
        self.sessions.insert(0, sess);
        self.session_boards.insert(local_id.clone(), b_slug.clone());
        self.persist_mode_binding(SessionModeBinding {
            session_id: local_id.clone(),
            mode: initial_mode,
            locked: false,
            project_id: initial_project.as_ref().map(|p| p.id.clone()),
            project_name: initial_project.as_ref().map(|p| p.name.clone()),
            cwd: resolved_cwd.clone(),
            server_id: Some(self.connection.id.clone()),
            updated_at_ms: now_epoch_ms(),
        });
        if let Some(ref c) = resolved_cwd {
            self.inspector_cwd = c.clone();
        }
        self.persist_board_state();
        self.active_model = resolved_model.clone();
        self.active_provider = resolved_provider.clone();
        self.active_session_id = Some(local_id.clone());
        self.active_view = AppViewId::Chat;
        self.bump_sidebar();
        self.bump_shell();
        cx.notify();

        let Some(ws) = realtime.filter(|w| w.is_connected()) else {
            return;
        };

        cx.spawn(async move |this, cx| {
            let mut params = json!({
                "source": "desktop",
                "profile": profile,
                "model": resolved_model,
                "reasoning_effort": effort,
                "fast": fast,
            });
            if resolved_provider != "auto" && !resolved_provider.is_empty() {
                params["provider"] = json!(resolved_provider);
            }
            if let Some(ref c) = resolved_cwd {
                params["cwd"] = json!(c);
            }
            if let Some(ref t) = title {
                params["title"] = json!(t);
            }

            let created = rt
                .spawn(async move { ws.create_session(params).await.ok() })
                .await
                .unwrap_or(None);

            let Some(res) = created else {
                return;
            };
            let stored = res
                .get("stored_session_id")
                .or_else(|| res.get("session_key"))
                .or_else(|| res.get("session_id"))
                .and_then(Value::as_str)
                .map(String::from);
            let wire = res
                .get("session_id")
                .and_then(Value::as_str)
                .map(String::from);

            let _ = this.update(cx, |s, cx| {
                if s.profile_generation != gen {
                    return;
                }
                let final_id = stored.unwrap_or_else(|| local_id.clone());
                if final_id != local_id {
                    if let Some(existing) = s.sessions.iter_mut().find(|x| x.id == local_id) {
                        existing.id = final_id.clone();
                    }
                    if let Some(chat) = s.chats.remove(&local_id) {
                        s.chats.insert(final_id.clone(), chat);
                    }
                    if s.draft_session_ids.remove(&local_id) {
                        s.draft_session_ids.insert(final_id.clone());
                    }
                    if let Some(b) = s.session_boards.remove(&local_id) {
                        s.session_boards.insert(final_id.clone(), b);
                    }
                    if let Some(mut mb) = s.session_modes.remove(&local_id) {
                        mb.session_id = final_id.clone();
                        s.session_modes.insert(final_id.clone(), mb);
                    }
                    if let Some(traj) = s.trajectory_events.remove(&local_id) {
                        s.trajectory_events.insert(final_id.clone(), traj);
                    }
                    if let Some(subs) = s.subagents.remove(&local_id) {
                        s.subagents.insert(final_id.clone(), subs);
                    }
                    if let Some(met) = s.session_metrics.remove(&local_id) {
                        s.session_metrics.insert(final_id.clone(), met);
                    }
                    if let Some(ref gw) = s.goal_worker {
                        gw.rekey_session(&local_id, &final_id);
                    }
                    if let Some(store) = s.harness_store.clone() {
                        let old_k = local_id.clone();
                        let new_k = final_id.clone();
                        s.tokio_rt.spawn_blocking(move || {
                            let _ = store.rekey_session_id(&old_k, &new_k);
                        });
                    }
                    if s.active_session_id.as_deref() == Some(local_id.as_str()) {
                        s.active_session_id = Some(final_id.clone());
                    }
                    s.persist_board_state();
                }
                if let Some(wid) = wire {
                    s.live_ids.insert(final_id.clone(), wid.clone());
                    s.wire_to_stored.insert(wid, final_id);
                }
                s.bump_sidebar();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn send_message(&mut self, raw_text: String, cx: &mut Context<Self>) -> bool {
        let text = if raw_text.trim().is_empty() && !self.attachments.is_empty() {
            "Please review the attached files.".to_string()
        } else {
            raw_text.trim().to_string()
        };
        if text.is_empty() {
            return false;
        }

        // Client-side UI shortcut slash commands
        match text.as_str() {
            "/model" => {
                self.model_picker_open = true;
                self.bump_shell();
                cx.notify();
                return true;
            }
            "/restart" => {
                self.restart_backend(cx);
                return true;
            }
            "/boards" => {
                self.set_active_view(AppViewId::Boards, cx);
                return true;
            }
            "/skills" => {
                self.set_active_view(AppViewId::Skills, cx);
                return true;
            }
            "/cron" => {
                self.set_active_view(AppViewId::Cron, cx);
                return true;
            }
            "/memory" => {
                self.set_active_view(AppViewId::Memory, cx);
                return true;
            }
            "/clear" => {
                self.create_new_session(None, None, None, None, None, cx);
                return true;
            }
            "/compress" => {
                self.compress_active_session(cx);
                return true;
            }
            "/branch" => {
                self.branch_active_session(cx);
                return true;
            }
            _ => {}
        }

        // Require a saved project & directory before starting a Work conversation
        let active_mode = self.active_conversation_mode();
        if active_mode == ConversationMode::Work && self.active_work_cwd().is_none() {
            self.project_picker_open = true;
            self.bump_shell();
            self.show_toast("Choose or create a project directory before starting Work mode.", cx);
            cx.notify();
            return false;
        }

        // Persistent Rust `/goal` entry point (coordinates Hermes runs & durable background state)
        if let Some(goal_cmd) = parse_goal_slash_command(&text) {
            match goal_cmd {
                GoalCommand::Status => {
                    let msg = if let Some(snap) = self.active_goal_snapshot() {
                        format!(
                            "Goal [{}] ({:?}) · round {}/{} · {}",
                            snap.goal.phase.label(),
                            snap.activation,
                            snap.goal.rounds_started,
                            snap.goal.max_rounds,
                            snap.goal.objective
                        )
                    } else {
                        "No active goal in this conversation. Start one with `/goal <objective>`.".into()
                    };
                    self.show_toast(msg, cx);
                    return true;
                }
                GoalCommand::Pause => {
                    self.pause_active_goal(cx);
                    return true;
                }
                GoalCommand::Resume => {
                    self.resume_active_goal(cx);
                    return true;
                }
                GoalCommand::Cancel => {
                    self.cancel_active_goal(cx);
                    return true;
                }
                GoalCommand::Complete(summary) => {
                    if let Some(sid) = self.active_session_id.clone() {
                        if let Some(ref w) = self.goal_worker {
                            match w.complete_goal(&sid, summary) {
                                Ok(_) => self.show_toast("Goal completed", cx),
                                Err(e) => self.show_toast(e, cx),
                            }
                        }
                    }
                    cx.notify();
                    return true;
                }
                GoalCommand::Create {
                    objective,
                    max_rounds,
                    max_duration_secs,
                    max_tokens,
                } => {
                    if self.realtime.as_ref().is_none_or(|w| !w.is_connected()) {
                        self.show_toast("Connect to Hermes before starting a goal.", cx);
                        return false;
                    }
                    // Ensure session exists and mode is locked
                    let sid = self.active_session_id.clone().unwrap_or_else(|| {
                        let id = format!("session_{}", uuid::Uuid::new_v4().simple());
                        let cwd = self.active_work_cwd();
                        let sess = HermesSession {
                            id: id.clone(),
                            title: Some(format!("Goal: {}", objective.chars().take(40).collect::<String>())),
                            profile: Some(self.active_profile.clone()),
                            model: Some(self.active_model.clone()),
                            provider: Some(self.active_provider.clone()),
                            source: Some("desktop".into()),
                            cwd,
                            message_count: Some(0),
                            created_at: Some(json!(now_secs())),
                            updated_at: Some(json!(now_secs())),
                            ..Default::default()
                        };
                        self.sessions.insert(0, sess);
                        self.active_session_id = Some(id.clone());
                        id
                    });
                    let proj = self.active_work_project().cloned();
                    let cwd = self.active_work_cwd();
                    self.persist_mode_binding(SessionModeBinding {
                        session_id: sid.clone(),
                        mode: active_mode,
                        locked: true,
                        project_id: proj.as_ref().map(|p| p.id.clone()),
                        project_name: proj.as_ref().map(|p| p.name.clone()),
                        cwd: cwd.clone(),
                        server_id: Some(self.connection.id.clone()),
                        updated_at_ms: now_epoch_ms(),
                    });
                    if let Some(worker) = self.goal_worker.clone() {
                        match worker.create_goal(
                            &sid,
                            objective.clone(),
                            proj.as_ref().map(|p| p.id.clone()),
                            proj.as_ref().map(|p| p.name.clone()),
                            cwd,
                            max_rounds,
                            max_duration_secs,
                            max_tokens,
                        ) {
                            Ok(snap) => {
                                self.record_trajectory_event(
                                    &sid,
                                    format!("goal-create-{}", snap.goal.goal_id),
                                    TrajectoryEventKind::GoalChanged,
                                    "lead".into(),
                                    None,
                                    None,
                                    "Goal created".into(),
                                    objective,
                                    false,
                                    None,
                                    json!({
                                        "goalId": snap.goal.goal_id,
                                        "maxRounds": snap.goal.max_rounds,
                                        "maxDurationSecs": snap.goal.max_duration_secs,
                                        "maxTokens": snap.goal.max_tokens,
                                    }),
                                );
                                self.bump_sidebar();
                                self.bump_shell();
                                self.dispatch_next_goal_round(sid, cx);
                                return true;
                            }
                            Err(e) => {
                                self.show_toast(e, cx);
                                return false;
                            }
                        }
                    }
                }
            }
        }

        let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) else {
            self.show_toast(
                "Hermes is disconnected. Check Settings → Connections.",
                cx,
            );
            return false;
        };

        if self
            .active_session_id
            .as_deref()
            .and_then(|id| self.chats.get(id))
            .is_some_and(|c| c.is_streaming)
        {
            self.show_toast("Wait for the current response before sending another message.", cx);
            return false;
        }

        let attachments = std::mem::take(&mut self.attachments);

        // Ensure we have an active session ID synchronously
        let sid = match self.active_session_id.clone() {
            Some(id) => id,
            None => {
                let id = format!("session_{}", uuid::Uuid::new_v4().simple());
                let b_slug = if self.active_board_slug != "all" {
                    self.active_board_slug.clone()
                } else {
                    "default".into()
                };
                let title: String = text.chars().take(48).collect();
                let sess = HermesSession {
                    id: id.clone(),
                    title: Some(title),
                    profile: Some(self.active_profile.clone()),
                    model: Some(self.active_model.clone()),
                    provider: Some(self.active_provider.clone()),
                    source: Some("desktop".into()),
                    board_slug: Some(b_slug.clone()),
                    message_count: Some(0),
                    created_at: Some(json!(now_secs())),
                    updated_at: Some(json!(now_secs())),
                    ..Default::default()
                };
                self.draft_session_ids.insert(id.clone());
                self.sessions.insert(0, sess);
                self.session_boards.insert(id.clone(), b_slug);
                self.persist_board_state();
                self.active_session_id = Some(id.clone());
                id
            }
        };

        if self.chats.get(&sid).is_some_and(|c| c.is_streaming) {
            self.attachments = attachments;
            self.show_toast("Wait for the current response before sending another message.", cx);
            return false;
        }

        let user_msg = HermesMessage {
            id: format!("user-{}", uuid::Uuid::new_v4().simple()),
            role: MessageRole::User,
            content: text.clone(),
            thinking: None,
            tool_name: None,
            timestamp: Some(now_secs()),
            model: None,
            tool_calls: Vec::new(),
            approval: None,
            clarify: None,
            streaming: false,
        };
        let user_msg_id = user_msg.id.clone();

        // Lock conversation mode (Chat | Work) on the first message and persist across restarts.
        let active_proj = if active_mode == ConversationMode::Work {
            self.active_work_project().cloned()
        } else {
            None
        };
        let bound_cwd = if active_mode == ConversationMode::Work {
            self.active_work_cwd()
        } else {
            self.active_session().and_then(|s| s.cwd.clone())
        };
        self.persist_mode_binding(SessionModeBinding {
            session_id: sid.clone(),
            mode: active_mode,
            locked: true,
            project_id: active_proj.as_ref().map(|p| p.id.clone()),
            project_name: active_proj.as_ref().map(|p| p.name.clone()),
            cwd: bound_cwd.clone(),
            server_id: Some(self.connection.id.clone()),
            updated_at_ms: now_epoch_ms(),
        });
        if let Some(ref c) = bound_cwd {
            self.inspector_cwd = c.clone();
            if let Some(s) = self.sessions.iter_mut().find(|s| s.id == sid) {
                s.cwd = Some(c.clone());
            }
        }

        let chat = self.chats.entry(sid.clone()).or_default();
        chat.messages.push(user_msg);
        chat.is_streaming = true;
        chat.has_streamed_text = false;
        chat.active_tool_name = None;
        chat.last_settlement = None;
        chat.live_status_text = "Thinking...".into();
        self.session_metrics
            .entry(sid.clone())
            .or_default()
            .on_turn_start();
        self.record_trajectory_event(
            &sid,
            format!("turn-start-{user_msg_id}"),
            TrajectoryEventKind::TurnStart,
            "lead".into(),
            None,
            None,
            format!("{} turn", active_mode.label()),
            text.chars().take(120).collect(),
            false,
            None,
            json!({"mode": active_mode.as_str(), "cwd": bound_cwd}),
        );
        self.record_trajectory_event(
            &sid,
            format!("user-msg-{user_msg_id}"),
            TrajectoryEventKind::UserMessage,
            "user".into(),
            None,
            None,
            "User prompt".into(),
            text.chars().take(160).collect(),
            false,
            None,
            json!({"text": text}),
        );

        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == sid) {
            if s.title
                .as_deref()
                .map(|t| t.is_empty() || t == "New Chat")
                .unwrap_or(true)
            {
                s.title = Some(text.chars().take(52).collect());
            }
            s.preview = Some(text.chars().take(100).collect());
            s.updated_at = Some(json!(now_secs()));
            s.message_count = Some(s.message_count.unwrap_or(0) + 1);
        }
        cx.notify();

        let existing_live = self.live_ids.get(&sid).cloned();
        let prev_synced_cfg = existing_live
            .as_deref()
            .and_then(|l| self.synced_session_config.get(l).copied());
        let profile = self.active_profile.clone();
        let model = self.active_model.clone();
        let provider = self.active_provider.clone();
        let effort_enum = self.reasoning_effort;
        let effort = effort_enum.as_str().to_string();
        let fast = self.fast_mode;
        let yolo = self.yolo_mode;
        let sess_cwd = self.active_session().and_then(|s| s.cwd.clone());
        let sess_title = self.active_session().and_then(|s| s.title.clone());
        let rt = self.tokio_rt.clone();
        self.bot_finished_at.remove(&profile);
        self.bump_sidebar();

        cx.spawn(async move |this, cx| {
            let sid_inner = sid.clone();
            let res: Result<(String, Option<String>), String> = rt
                .spawn(async move {
                    let mut live_sid = existing_live;
                    if live_sid.is_none() && !sid_inner.starts_with("session_") {
                        if let Ok(resumed) = ws.resume_session(&sid_inner, &profile).await {
                            live_sid = resumed
                                .get("session_id")
                                .and_then(Value::as_str)
                                .map(String::from);
                        }
                    }
                    if live_sid.is_none() {
                        let mut params = json!({
                            "source": "desktop",
                            "profile": profile,
                            "model": model,
                            "reasoning_effort": effort,
                            "fast": fast,
                        });
                        if provider != "auto" && !provider.is_empty() {
                            params["provider"] = json!(provider);
                        }
                        if let Some(c) = sess_cwd {
                            params["cwd"] = json!(c);
                        }
                        if let Some(t) = sess_title {
                            params["title"] = json!(t);
                        }
                        let created = ws.create_session(params).await?;
                        live_sid = created
                            .get("session_id")
                            .and_then(Value::as_str)
                            .map(String::from);
                    }
                    let live = live_sid.unwrap_or_else(|| sid_inner.clone());

                    // Gateway slash command check (`command.dispatch` then `slash.exec` fallback)
                    if text.starts_with('/') {
                        let cmd_res = match ws.dispatch_command(&live, &text).await {
                            Ok(v) => Ok(v),
                            Err(_) => ws.slash_exec(&live, &text).await,
                        };
                        if let Ok(cmd_res) = cmd_res {
                            if cmd_res.get("type").and_then(Value::as_str) == Some("send") {
                                if let Some(msg) = cmd_res
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .filter(|s| !s.is_empty())
                                {
                                    ws.submit_prompt(&live, msg).await?;
                                    return Ok((live, None));
                                }
                            }
                            if let Some(out) = cmd_res
                                .get("output")
                                .or_else(|| cmd_res.get("notice"))
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                            {
                                return Ok((live, Some(out.to_string())));
                            }
                        }
                    }

                    // Only send config.set RPCs when session config actually changed, and send concurrently
                    let desired_cfg = (effort_enum, fast, yolo);
                    if prev_synced_cfg != Some(desired_cfg) {
                        let send_effort = prev_synced_cfg.map(|c| c.0) != Some(effort_enum);
                        let send_fast = prev_synced_cfg.map(|c| c.1) != Some(fast);
                        let send_yolo = prev_synced_cfg.map(|c| c.2) != Some(yolo);
                        let f_effort = async {
                            if send_effort {
                                let _ = ws.set_session_config(&live, "reasoning", &effort).await;
                            }
                        };
                        let f_fast = async {
                            if send_fast {
                                let _ = ws
                                    .set_session_config(&live, "fast", if fast { "on" } else { "off" })
                                    .await;
                            }
                        };
                        let f_yolo = async {
                            if send_yolo {
                                let _ = ws
                                    .set_session_config(&live, "yolo", if yolo { "on" } else { "off" })
                                    .await;
                            }
                        };
                        tokio::join!(f_effort, f_fast, f_yolo);
                    }

                    // Attachments
                    let mut prompt = text;
                    for att in attachments {
                        use base64::Engine;
                        let b64 =
                            base64::engine::general_purpose::STANDARD.encode(att.bytes.as_slice());
                        let data_url = format!("data:{};base64,{b64}", att.mime);
                        if att.mime.starts_with("image/") {
                            let _ = ws
                                .rpc(
                                    "image.attach_bytes",
                                    json!({
                                        "session_id": live,
                                        "content_base64": data_url,
                                        "filename": att.name,
                                    }),
                                    30_000,
                                )
                                .await;
                        } else if att.mime == "application/pdf" {
                            let _ = ws
                                .rpc(
                                    "pdf.attach",
                                    json!({
                                        "session_id": live,
                                        "content_base64": data_url,
                                        "filename": att.name,
                                    }),
                                    30_000,
                                )
                                .await;
                        } else if let Ok(r) = ws
                            .rpc(
                                "file.attach",
                                json!({
                                    "session_id": live,
                                    "data_url": data_url,
                                    "name": att.name,
                                }),
                                30_000,
                            )
                            .await
                        {
                            if let Some(ref_txt) = r.get("ref_text").and_then(Value::as_str) {
                                prompt.push('\n');
                                prompt.push_str(ref_txt);
                            }
                        }
                    }

                    ws.submit_prompt(&live, &prompt).await?;
                    Ok((live, None))
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let _ = this.update(cx, |s, cx| match res {
                Ok((live, immediate_output)) => {
                    s.synced_session_config
                        .insert(live.clone(), (effort_enum, fast, yolo));
                    s.live_ids.insert(sid.clone(), live.clone());
                    s.wire_to_stored.insert(live, sid.clone());
                    s.draft_session_ids.remove(&sid);
                    if let Some(out) = immediate_output {
                        let chat = s.chats.entry(sid.clone()).or_default();
                        chat.is_streaming = false;
                        chat.messages.push(HermesMessage {
                            id: format!("cmd-{}", uuid::Uuid::new_v4().simple()),
                            role: MessageRole::Assistant,
                            content: out,
                            thinking: None,
                            tool_name: None,
                            timestamp: Some(now_secs()),
                            model: None,
                            tool_calls: Vec::new(),
                            approval: None,
                            clarify: None,
                            streaming: false,
                        });
                        s.bump_sidebar();
                    }
                    cx.notify();
                }
                Err(err) => {
                    let chat = s.chats.entry(sid.clone()).or_default();
                    chat.is_streaming = false;
                    chat.last_settlement = Some((TurnSettlementOutcome::Error, Instant::now()));
                    chat.messages.push(HermesMessage {
                        id: format!("err-{}", uuid::Uuid::new_v4().simple()),
                        role: MessageRole::Assistant,
                        content: format!("Could not deliver message to Hermes backend: {err}"),
                        thinking: None,
                        tool_name: None,
                        timestamp: Some(now_secs()),
                        model: None,
                        tool_calls: Vec::new(),
                        approval: None,
                        clarify: None,
                        streaming: false,
                    });
                    s.bump_sidebar();
                    s.show_toast(err, cx);
                    cx.notify();
                }
            });
        })
        .detach();
        true
    }

    pub fn interrupt_stream(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(ws) = self.realtime.clone() else {
            return;
        };
        let live = self.live_ids.get(&sid).cloned().unwrap_or_else(|| sid.clone());
        let rt = self.tokio_rt.clone();
        if let Some(c) = self.chats.get_mut(&sid) {
            c.is_streaming = false;
            c.last_settlement = Some((TurnSettlementOutcome::Cancelled, Instant::now()));
            c.settle_stream_text(None, None);
        }
        // If an armed goal is active for this session, pause it so cancelling the turn
        // never auto-dispatches another round unexpectedly.
        if let Some(worker) = self.goal_worker.clone() {
            if let Some(snap) = worker.snapshot_for_session(&sid) {
                if snap.goal.phase == crate::harness::goal_worker::GoalPhase::Active
                    && snap.activation == crate::harness::goal_worker::GoalActivation::Armed
                {
                    if let Ok(paused) = worker.pause_goal(&sid, None, "Interrupted by user") {
                        self.record_trajectory_event(
                            &sid,
                            format!(
                                "goal-interrupt-{}-{}",
                                paused.goal.goal_id, paused.goal.revision
                            ),
                            TrajectoryEventKind::GoalChanged,
                            "lead".into(),
                            None,
                            None,
                            "Goal paused on interrupt".into(),
                            paused.goal.objective,
                            false,
                            None,
                            json!({"goalId": paused.goal.goal_id, "phase": "paused", "revision": paused.goal.revision}),
                        );
                    }
                }
            }
        }
        self.record_trajectory_event(
            &sid,
            format!("turn-cancel-{}", uuid::Uuid::new_v4().simple()),
            TrajectoryEventKind::TurnCompleted,
            "lead".into(),
            None,
            None,
            "Turn interrupted".into(),
            "Stopped by user".into(),
            false,
            None,
            json!({"cancelled": true}),
        );
        self.bump_sidebar();
        cx.notify();
        cx.spawn(async move |_, _| {
            let _ = rt.spawn(async move { ws.interrupt_session(&live).await }).await;
        })
        .detach();
    }

    fn handle_stream_event(&mut self, event: HermesStreamEvent, cx: &mut Context<Self>) {
        match event {
            HermesStreamEvent::Status { status, detail } => {
                match status.as_str() {
                    "connected" => {
                        self.connection_status = if self.connection.is_local() {
                            ConnectionStatus::Local
                        } else {
                            ConnectionStatus::Connected
                        };
                        self.connection_detail = "Live Hermes Gateway".into();
                        if let Some(ref gw) = self.goal_worker {
                            gw.set_worker_connected(true);
                        }
                        self.refresh_slash_commands(cx);
                        self.refresh_server_projects(cx);
                        if self.was_disconnected {
                            self.was_disconnected = false;
                            self.refresh_core_capabilities(false, cx);
                        }
                    }
                    "connecting" => {
                        self.connection_status = ConnectionStatus::Connecting;
                        self.connection_detail = "Connecting WebSocket...".into();
                    }
                    "disconnected" => {
                        self.was_disconnected = true;
                        self.connection_status = ConnectionStatus::Disconnected;
                        self.connection_detail = "Connection lost. Reconnecting…".into();
                        if let Some(ref gw) = self.goal_worker {
                            gw.set_worker_connected(false);
                        }
                        self.live_ids.clear();
                        self.wire_to_stored.clear();
                        self.synced_session_config.clear();
                        for chat in self.chats.values_mut() {
                            chat.is_streaming = false;
                            chat.settle_stream_text(None, None);
                        }
                    }
                    _ => {
                        self.connection_status = ConnectionStatus::Error;
                        self.connection_detail = detail.unwrap_or_else(|| "WebSocket error".into());
                    }
                }
                self.bump_sidebar();
                cx.notify();
                return;
            }
            HermesStreamEvent::GatewayReady => return,
            _ => {}
        }

        let wire_sid = match &event {
            HermesStreamEvent::SessionInfo { session_id, .. }
            | HermesStreamEvent::SessionUsage { session_id, .. }
            | HermesStreamEvent::StatusUpdate { session_id, .. }
            | HermesStreamEvent::SessionTitle { session_id, .. }
            | HermesStreamEvent::MessageStart { session_id }
            | HermesStreamEvent::TextDelta { session_id, .. }
            | HermesStreamEvent::ThinkingDelta { session_id, .. }
            | HermesStreamEvent::MessageInterim { session_id, .. }
            | HermesStreamEvent::ToolGenerating { session_id, .. }
            | HermesStreamEvent::ToolStart { session_id, .. }
            | HermesStreamEvent::ToolProgress { session_id, .. }
            | HermesStreamEvent::ToolEnd { session_id, .. }
            | HermesStreamEvent::SubagentStart { session_id, .. }
            | HermesStreamEvent::SubagentUpdate { session_id, .. }
            | HermesStreamEvent::SubagentComplete { session_id, .. }
            | HermesStreamEvent::TodoUpdated { session_id, .. }
            | HermesStreamEvent::ApprovalRequest { session_id, .. }
            | HermesStreamEvent::ApprovalSettled { session_id, .. }
            | HermesStreamEvent::ClarifyRequest { session_id, .. }
            | HermesStreamEvent::ClarifySettled { session_id, .. }
            | HermesStreamEvent::MessageComplete { session_id, .. }
            | HermesStreamEvent::Error { session_id, .. } => session_id.clone(),
            _ => None,
        };

        let target_sid = wire_sid
            .as_deref()
            .and_then(|w| self.wire_to_stored.get(w).cloned())
            .or(wire_sid)
            .or_else(|| self.active_session_id.clone())
            .unwrap_or_else(|| "draft".into());

        match event {
            HermesStreamEvent::SessionInfo { model, provider, .. } => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == target_sid) {
                    if let Some(m) = model {
                        s.model = Some(m);
                    }
                    if let Some(p) = provider {
                        s.provider = Some(p);
                    }
                }
            }
            HermesStreamEvent::SessionTitle { title, .. } => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == target_sid) {
                    s.title = Some(title);
                    self.bump_sidebar();
                }
            }
            HermesStreamEvent::SessionUsage { usage, .. } => {
                self.session_metrics
                    .entry(target_sid.clone())
                    .or_default()
                    .on_provider_usage(&usage);
                let chat = self.chats.entry(target_sid).or_default();
                chat.session_usage = Some(usage);
            }
            HermesStreamEvent::StatusUpdate { kind, text, .. } => {
                if kind.as_deref() == Some("goal") && !text.trim().is_empty() {
                    self.record_trajectory_event(
                        &target_sid,
                        format!("status-goal-{}", now_epoch_ms()),
                        TrajectoryEventKind::GoalChanged,
                        "lead".into(),
                        None,
                        None,
                        "Goal status update".into(),
                        text.clone(),
                        false,
                        None,
                        json!({"kind": "goal", "text": text}),
                    );
                }
                let chat = self.chats.entry(target_sid).or_default();
                chat.live_status_text = text;
            }
            HermesStreamEvent::TodoUpdated { todos, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.todos = todos;
            }
            HermesStreamEvent::MessageStart { .. } => {
                let starting_profile = self
                    .sessions
                    .iter()
                    .find(|s| s.id == target_sid)
                    .and_then(|s| s.profile.clone())
                    .filter(|p| !p.is_empty())
                    .unwrap_or_else(|| self.active_profile.clone());
                let cleared_finish = self.bot_finished_at.remove(&starting_profile).is_some();
                self.session_metrics
                    .entry(target_sid.clone())
                    .or_default()
                    .on_turn_start();
                let chat = self.chats.entry(target_sid).or_default();
                let was_streaming = chat.is_streaming;
                chat.is_streaming = true;
                chat.has_streamed_text = false;
                chat.active_tool_name = None;
                chat.last_settlement = None;
                if !chat.messages.iter().any(|m| m.streaming) {
                    chat.messages.push(HermesMessage::new_assistant(format!(
                        "asst-{}",
                        uuid::Uuid::new_v4().simple()
                    )));
                }
                if !was_streaming || cleared_finish {
                    self.bump_sidebar();
                }
            }
            HermesStreamEvent::TextDelta { delta, thinking, .. } => {
                if !delta.is_empty() {
                    self.session_metrics
                        .entry(target_sid.clone())
                        .or_default()
                        .on_first_token();
                }
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.append_stream_text(&delta, thinking.as_deref());
            }
            HermesStreamEvent::ThinkingDelta { delta, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.append_stream_text("", Some(&delta));
            }
            HermesStreamEvent::MessageInterim { content, thinking, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.apply_interim_snapshot(&content, thinking.as_deref());
            }
            HermesStreamEvent::ToolGenerating { name, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.live_status_text = format!("Preparing {name}…");
            }
            HermesStreamEvent::ToolStart { tool_call, .. } => {
                let tc_id = tool_call.id.clone();
                let tc_name = tool_call.name.clone();
                let tc_args = tool_call.arguments.clone().unwrap_or(Value::Null);
                let tc_preview = tool_call.preview.clone().unwrap_or_default();
                let kind = if is_ptc_tool(&tc_name) {
                    TrajectoryEventKind::PtcExecution
                } else {
                    TrajectoryEventKind::ToolCallStart
                };
                self.record_trajectory_event(
                    &target_sid,
                    format!("tc-start-{tc_id}"),
                    kind,
                    "lead".into(),
                    None,
                    Some(tc_id.clone()),
                    tc_name.clone(),
                    if tc_preview.is_empty() {
                        format!("Calling {tc_name}")
                    } else {
                        tc_preview
                    },
                    false,
                    None,
                    json!({"tool": tc_name, "arguments": tc_args}),
                );
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.has_streamed_text = false;
                chat.active_tool_name = Some(tool_call.name.clone());
                chat.live_status_text = format!("Running {}…", tool_call.name);
                if let Some(m) = chat.messages.iter_mut().rev().find(|m| m.streaming) {
                    if !m.tool_calls.iter().any(|existing| existing.id == tool_call.id) {
                        m.tool_calls.push(tool_call);
                    }
                } else {
                    let mut m =
                        HermesMessage::new_assistant(format!("asst-{}", uuid::Uuid::new_v4().simple()));
                    m.tool_calls.push(tool_call);
                    chat.messages.push(m);
                }
            }
            HermesStreamEvent::ToolProgress {
                tool_call_id,
                preview,
                ..
            } => {
                let chat = self.chats.entry(target_sid).or_default();
                for m in chat.messages.iter_mut().rev() {
                    if let Some(tc) = m.tool_calls.iter_mut().find(|t| {
                        (!tool_call_id.is_empty() && t.id == tool_call_id)
                            || t.status == ToolCallStatus::Running
                    }) {
                        tc.preview = Some(preview);
                        break;
                    }
                }
            }
            HermesStreamEvent::ToolEnd {
                tool_call_id,
                output,
                error,
                duration_ms,
                inline_diff,
                ..
            } => {
                if let Some(ref diff) = inline_diff {
                    self.session_metrics
                        .entry(target_sid.clone())
                        .or_default()
                        .record_agent_diff(diff);
                }
                let is_err = error.is_some();
                let mut matched_name = "tool".to_string();
                let mut matched_args = Value::Null;
                let chat = self.chats.entry(target_sid.clone()).or_default();
                chat.live_status_text.clear();
                chat.active_tool_name = None;
                for m in chat.messages.iter_mut().rev() {
                    if let Some(tc) = m.tool_calls.iter_mut().find(|t| {
                        (!tool_call_id.is_empty() && t.id == tool_call_id)
                            || t.status == ToolCallStatus::Running
                    }) {
                        matched_name = tc.name.clone();
                        matched_args = tc.arguments.clone().unwrap_or(Value::Null);
                        tc.waiting_approval = false;
                        tc.status = if is_err {
                            ToolCallStatus::Error
                        } else {
                            ToolCallStatus::Done
                        };
                        tc.output = error.clone().or(output.clone()).or(tc.output.take());
                        tc.duration_ms = duration_ms;
                        tc.inline_diff = inline_diff.clone();
                        break;
                    }
                }
                let summary = error
                    .as_deref()
                    .or(output.as_deref())
                    .unwrap_or("Completed")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(140)
                    .collect();
                self.record_trajectory_event(
                    &target_sid,
                    format!("tc-end-{tool_call_id}"),
                    TrajectoryEventKind::ToolCallResult,
                    "lead".into(),
                    None,
                    Some(tool_call_id),
                    format!("{matched_name} result"),
                    summary,
                    is_err,
                    duration_ms,
                    json!({
                        "tool": matched_name,
                        "arguments": matched_args,
                        "output": output,
                        "error": error,
                        "inlineDiff": inline_diff,
                    }),
                );
                if self.active_conversation_mode() == ConversationMode::Work
                    && matches!(matched_name.as_str(), "write_file" | "patch" | "edit" | "terminal")
                {
                    self.refresh_inspector_git(cx);
                }
            }
            HermesStreamEvent::SubagentStart {
                subagent_id,
                parent_id,
                child_session_id,
                depth,
                goal,
                model,
                ..
            } => {
                let now = now_epoch_ms();
                let parent = parent_id.unwrap_or_else(|| "lead".into());
                let node = SubagentNode {
                    subagent_id: subagent_id.clone(),
                    session_id: target_sid.clone(),
                    parent_agent_id: parent.clone(),
                    child_session_id,
                    depth,
                    goal: goal.clone(),
                    model,
                    status: SubagentStatus::Running,
                    current_operation: "Starting…".into(),
                    tool_count: 0,
                    input_tokens: None,
                    output_tokens: None,
                    duration_ms: None,
                    summary: None,
                    started_at_ms: now,
                    updated_at_ms: now,
                };
                let list = self.subagents.entry(target_sid.clone()).or_default();
                list.retain(|n| n.subagent_id != subagent_id);
                list.insert(0, node.clone());
                if let Some(store) = self.harness_store.clone() {
                    let rt = self.tokio_rt.clone();
                    rt.spawn_blocking(move || {
                        let _ = store.upsert_subagent(&node);
                    });
                }
                self.record_trajectory_event(
                    &target_sid,
                    format!("sub-start-{subagent_id}"),
                    TrajectoryEventKind::SubagentSpawned,
                    subagent_id,
                    Some(parent),
                    None,
                    "Subagent spawned".into(),
                    goal,
                    false,
                    None,
                    json!({"depth": depth}),
                );
            }
            HermesStreamEvent::SubagentUpdate {
                subagent_id,
                tool_name,
                operation,
                tool_count,
                input_tokens,
                output_tokens,
                ..
            } => {
                let now = now_epoch_ms();
                let op_display = match tool_name.as_deref() {
                    Some(t) if !t.is_empty() && !operation.contains(t) => {
                        format!("{t}: {operation}")
                    }
                    _ => operation.clone(),
                };
                if let Some(ref t) = tool_name {
                    if !t.is_empty() {
                        self.record_trajectory_event(
                            &target_sid,
                            format!("sub-tool-{subagent_id}-{t}-{}", tool_count.unwrap_or(0)),
                            TrajectoryEventKind::ToolCallStart,
                            subagent_id.clone(),
                            Some("lead".into()),
                            None,
                            format!("Subagent tool: {t}"),
                            op_display.clone(),
                            false,
                            None,
                            json!({"subagentId": subagent_id, "tool": t, "toolCount": tool_count}),
                        );
                    }
                }
                let list = self.subagents.entry(target_sid.clone()).or_default();
                if let Some(node) = list.iter_mut().find(|n| n.subagent_id == subagent_id) {
                    node.current_operation = op_display;
                    if let Some(tc) = tool_count {
                        node.tool_count = tc;
                    }
                    if input_tokens.is_some() {
                        node.input_tokens = input_tokens;
                    }
                    if output_tokens.is_some() {
                        node.output_tokens = output_tokens;
                    }
                    node.updated_at_ms = now;
                    if let Some(store) = self.harness_store.clone() {
                        let node_save = node.clone();
                        self.tokio_rt.spawn_blocking(move || {
                            let _ = store.upsert_subagent(&node_save);
                        });
                    }
                }
            }
            HermesStreamEvent::SubagentComplete {
                subagent_id,
                status,
                summary,
                duration_ms,
                input_tokens,
                output_tokens,
                ..
            } => {
                let now = now_epoch_ms();
                let parsed_status = SubagentStatus::from_str_lossy(&status);
                let is_err = parsed_status == SubagentStatus::Failed;
                let list = self.subagents.entry(target_sid.clone()).or_default();
                if let Some(node) = list.iter_mut().find(|n| n.subagent_id == subagent_id) {
                    node.status = parsed_status;
                    node.current_operation = summary
                        .clone()
                        .unwrap_or_else(|| status.clone());
                    node.summary = summary.clone();
                    node.duration_ms = duration_ms;
                    if input_tokens.is_some() {
                        node.input_tokens = input_tokens;
                    }
                    if output_tokens.is_some() {
                        node.output_tokens = output_tokens;
                    }
                    node.updated_at_ms = now;
                    if let Some(store) = self.harness_store.clone() {
                        let node_save = node.clone();
                        self.tokio_rt.spawn_blocking(move || {
                            let _ = store.upsert_subagent(&node_save);
                        });
                    }
                }
                self.record_trajectory_event(
                    &target_sid,
                    format!("sub-done-{subagent_id}"),
                    TrajectoryEventKind::SubagentCompleted,
                    subagent_id,
                    Some("lead".into()),
                    None,
                    format!("Subagent {status}"),
                    summary.unwrap_or(status),
                    is_err,
                    duration_ms,
                    json!({}),
                );
            }
            HermesStreamEvent::ApprovalRequest { approval, .. } => {
                let appr_id = approval.id.clone();
                let appr_title = approval.title.clone();
                let appr_cmd = approval.command.clone();
                self.record_trajectory_event(
                    &target_sid,
                    format!("appr-req-{appr_id}"),
                    TrajectoryEventKind::ApprovalRequested,
                    "lead".into(),
                    None,
                    None,
                    "Waiting for approval".into(),
                    appr_cmd.clone().unwrap_or_else(|| appr_title.clone()),
                    false,
                    None,
                    json!({"approvalId": appr_id, "command": appr_cmd}),
                );
                let chat = self.chats.entry(target_sid).or_default();
                let id = format!("appr-{}", approval.id);
                chat.awaiting_user_response = true;
                for m in chat.messages.iter_mut().rev() {
                    if let Some(tc) = m.tool_calls.iter_mut().rev().find(|t| t.status == ToolCallStatus::Running) {
                        tc.waiting_approval = true;
                        break;
                    }
                }
                if !chat.messages.iter().any(|m| m.id == id) {
                    let mut m = HermesMessage::new_assistant(id);
                    m.streaming = false;
                    m.approval = Some(approval);
                    chat.messages.push(m);
                }
            }
            HermesStreamEvent::ApprovalSettled { id, decision, .. } => {
                self.record_trajectory_event(
                    &target_sid,
                    format!("appr-dec-{id}"),
                    TrajectoryEventKind::ApprovalDecided,
                    "user".into(),
                    None,
                    None,
                    format!("Approval {decision:?}"),
                    id.clone(),
                    decision == ApprovalDecision::Denied,
                    None,
                    json!({"approvalId": id, "decision": decision}),
                );
                let chat = self.chats.entry(target_sid).or_default();
                for m in &mut chat.messages {
                    if let Some(ap) = &mut m.approval {
                        if ap.id == id {
                            ap.settled = Some(decision.clone());
                        }
                    }
                    for tc in &mut m.tool_calls {
                        tc.waiting_approval = false;
                    }
                }
                chat.awaiting_user_response = chat.messages.iter().any(|m| {
                    m.approval.as_ref().is_some_and(|a| a.settled.is_none())
                        || m.clarify.as_ref().is_some_and(|c| c.answered.is_none())
                });
            }
            HermesStreamEvent::ClarifyRequest { clarify, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                let id = format!("clarify-{}", clarify.id);
                chat.awaiting_user_response = true;
                if !chat.messages.iter().any(|m| m.id == id) {
                    let mut m = HermesMessage::new_assistant(id);
                    m.streaming = false;
                    m.clarify = Some(clarify);
                    chat.messages.push(m);
                }
            }
            HermesStreamEvent::ClarifySettled { id, answer, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                for m in &mut chat.messages {
                    if let Some(cl) = &mut m.clarify {
                        if cl.id == id {
                            cl.answered = Some(answer.clone());
                        }
                    }
                }
                chat.awaiting_user_response = chat.messages.iter().any(|m| {
                    m.approval.as_ref().is_some_and(|a| a.settled.is_none())
                        || m.clarify.as_ref().is_some_and(|c| c.answered.is_none())
                });
            }
            HermesStreamEvent::MessageComplete {
                content,
                thinking,
                status,
                error,
                usage,
                ..
            } => {
                if let Some(ref u) = usage {
                    self.session_metrics
                        .entry(target_sid.clone())
                        .or_default()
                        .on_provider_usage(u);
                }
                self.session_metrics
                    .entry(target_sid.clone())
                    .or_default()
                    .on_turn_end();
                let turn_elapsed_ms = self
                    .session_metrics
                    .get(&target_sid)
                    .and_then(|m| m.elapsed_ms());
                let total_tokens_now = self
                    .session_metrics
                    .get(&target_sid)
                    .and_then(|m| m.total_tokens().value);

                let is_failed = status.as_deref() == Some("error") || error.is_some();
                let is_interrupted = status.as_deref() == Some("interrupted");
                let finished_profile = self
                    .sessions
                    .iter()
                    .find(|s| s.id == target_sid)
                    .and_then(|s| s.profile.clone())
                    .filter(|p| !p.is_empty())
                    .unwrap_or_else(|| self.active_profile.clone());

                let (settled_text, settled_thinking) = {
                    let chat = self.chats.entry(target_sid.clone()).or_default();
                    chat.is_streaming = false;
                    chat.active_tool_name = None;
                    chat.live_status_text.clear();
                    if let Some(u) = usage {
                        chat.session_usage = Some(u);
                    }
                    let finished_at = Instant::now();
                    chat.last_settlement = Some((
                        if is_failed {
                            TurnSettlementOutcome::Error
                        } else if is_interrupted {
                            TurnSettlementOutcome::Cancelled
                        } else {
                            TurnSettlementOutcome::Success
                        },
                        finished_at,
                    ));
                    let mut settled_c = content.clone().unwrap_or_default();
                    let mut settled_t = thinking.clone();
                    if let Some(index) = chat.settle_stream_text(content.as_deref(), thinking.as_deref()) {
                        chat.messages[index]
                            .model
                            .get_or_insert_with(|| self.active_model.clone());
                        settled_c = chat.messages[index].content.clone();
                        settled_t = chat.messages[index].thinking.clone();
                    }
                    chat.has_streamed_text = false;
                    (settled_c, settled_t)
                };

                if let Some(thought) = settled_thinking.filter(|t| !t.trim().is_empty()) {
                    self.record_trajectory_event(
                        &target_sid,
                        format!("thought-{}", uuid::Uuid::new_v4().simple()),
                        TrajectoryEventKind::ReasoningSummary,
                        "lead".into(),
                        None,
                        None,
                        "Reasoning summary".into(),
                        thought.lines().next().unwrap_or("").chars().take(140).collect(),
                        false,
                        None,
                        json!({"thinking": thought}),
                    );
                }
                self.record_trajectory_event(
                    &target_sid,
                    format!("turn-done-{}", uuid::Uuid::new_v4().simple()),
                    if is_failed {
                        TrajectoryEventKind::Error
                    } else {
                        TrajectoryEventKind::TurnCompleted
                    },
                    "lead".into(),
                    None,
                    None,
                    if is_failed {
                        "Turn failed".into()
                    } else if is_interrupted {
                        "Turn interrupted".into()
                    } else {
                        "Turn completed".into()
                    },
                    error
                        .clone()
                        .unwrap_or_else(|| settled_text.lines().next().unwrap_or("").chars().take(140).collect()),
                    is_failed,
                    turn_elapsed_ms,
                    json!({"status": status, "error": error, "tokens": total_tokens_now}),
                );

                // Goal continuation driver (runs in background even if user switched conversations!)
                let should_continue_goal = if let Some(ref gw) = self.goal_worker {
                    let completed_marker = settled_text.contains("GOAL_COMPLETE");
                    gw.on_turn_settled(
                        &target_sid,
                        !is_failed && !is_interrupted,
                        Some(settled_text.chars().take(200).collect()),
                        total_tokens_now,
                        completed_marker,
                    )
                } else {
                    false
                };

                if should_continue_goal {
                    let next_sid = target_sid.clone();
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(600))
                            .await;
                        let _ = this.update(cx, |s, cx| {
                            s.dispatch_next_goal_round(next_sid, cx);
                        });
                    })
                    .detach();
                } else if !is_failed && !is_interrupted {
                    let finished_at = Instant::now();
                    self.bot_finished_at.insert(finished_profile.clone(), finished_at);
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(Duration::from_secs(8)).await;
                        let _ = this.update(cx, |s, cx| {
                            if s.bot_finished_at.get(&finished_profile) == Some(&finished_at) {
                                s.bot_finished_at.remove(&finished_profile);
                                s.bump_sidebar();
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                }
                if self.active_conversation_mode() == ConversationMode::Work {
                    self.refresh_inspector_git(cx);
                }
                self.bump_sidebar();
            }
            HermesStreamEvent::Error { message, .. } => {
                self.session_metrics
                    .entry(target_sid.clone())
                    .or_default()
                    .on_turn_end();
                self.record_trajectory_event(
                    &target_sid,
                    format!("err-{}", uuid::Uuid::new_v4().simple()),
                    TrajectoryEventKind::Error,
                    "lead".into(),
                    None,
                    None,
                    "Runtime error".into(),
                    message.clone(),
                    true,
                    None,
                    json!({"error": message}),
                );
                if let Some(ref gw) = self.goal_worker {
                    gw.on_turn_settled(&target_sid, false, Some(message.clone()), None, false);
                }
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = false;
                chat.last_settlement = Some((TurnSettlementOutcome::Error, Instant::now()));
                chat.settle_stream_text(None, None);
                let mut m =
                    HermesMessage::new_assistant(format!("err-{}", uuid::Uuid::new_v4().simple()));
                m.content = message.clone();
                m.streaming = false;
                chat.messages.push(m);
                self.bump_sidebar();
                self.show_toast(message, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn respond_approval(&mut self, approval_id: String, choice: &'static str, cx: &mut Context<Self>) {
        let sid = self.active_session_id.clone().unwrap_or_else(|| "draft".into());
        let decision = match choice {
            "deny" => ApprovalDecision::Denied,
            "always" => ApprovalDecision::Always,
            _ => ApprovalDecision::Allowed,
        };
        if let Some(chat) = self.chats.get_mut(&sid) {
            for m in &mut chat.messages {
                if let Some(ap) = &mut m.approval {
                    if ap.id == approval_id {
                        ap.settled = Some(decision.clone());
                    }
                }
            }
            chat.awaiting_user_response = chat.messages.iter().any(|m| {
                m.approval.as_ref().is_some_and(|a| a.settled.is_none())
                    || m.clarify.as_ref().is_some_and(|c| c.answered.is_none())
            });
        }
        cx.notify();
        if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
            let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { ws.respond_approval(&live, &approval_id, choice).await })
                    .await;
            })
            .detach();
        }
    }

    pub fn respond_clarify(&mut self, clarify_id: String, answer: String, cx: &mut Context<Self>) {
        let sid = self.active_session_id.clone().unwrap_or_else(|| "draft".into());
        if let Some(chat) = self.chats.get_mut(&sid) {
            for m in &mut chat.messages {
                if let Some(cl) = &mut m.clarify {
                    if cl.id == clarify_id {
                        cl.answered = Some(answer.clone());
                    }
                }
            }
            chat.awaiting_user_response = chat.messages.iter().any(|m| {
                m.approval.as_ref().is_some_and(|a| a.settled.is_none())
                    || m.clarify.as_ref().is_some_and(|c| c.answered.is_none())
            });
        }
        cx.notify();
        if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
            let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { ws.respond_clarify(&live, &clarify_id, &answer).await })
                    .await;
            })
            .detach();
        }
    }

    pub fn rename_session(&mut self, session_id: String, title: String, cx: &mut Context<Self>) {
        let clean = title.trim().to_string();
        if clean.is_empty() {
            return;
        }
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            s.title = Some(clean.clone());
        }
        self.bump_sidebar();
        cx.notify();
        let client = self.client.clone();
        let ws = self.realtime.clone();
        let live = self
            .live_ids
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| session_id.clone());
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |_, _| {
            let _ = rt
                .spawn(async move {
                    if let Some(c) = client {
                        let _ = c.rename_session(&session_id, &clean).await;
                    }
                    if let Some(w) = ws {
                        if w.is_connected() {
                            let _ = w.set_session_title(&live, &clean).await;
                        }
                    }
                })
                .await;
        })
        .detach();
    }

    pub fn toggle_pin_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        let next = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| !s.pinned)
            .unwrap_or(true);
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            s.pinned = next;
        }
        self.bump_sidebar();
        cx.notify();
        if let Some(c) = self.client.clone() {
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { c.pin_session(&session_id, next).await })
                    .await;
            })
            .detach();
        }
    }

    pub fn archive_session(&mut self, session_id: String, archived: bool, cx: &mut Context<Self>) {
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            s.archived = archived;
        }
        if archived && self.active_session_id.as_deref() == Some(session_id.as_str()) {
            self.active_session_id = None;
        }
        self.bump_sidebar();
        self.show_toast(
            if archived {
                "Session archived"
            } else {
                "Session restored"
            },
            cx,
        );
        if let Some(c) = self.client.clone() {
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { c.archive_session(&session_id, archived).await })
                    .await;
            })
            .detach();
        }
    }

    pub fn compress_active_session(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) else {
            self.show_toast("Connect to Hermes to compress session context", cx);
            return;
        };
        let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { ws.compress_session(&live).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| match res {
                Ok(_) => s.show_toast("Context compressed", cx),
                Err(e) => s.show_toast(e, cx),
            });
        })
        .detach();
    }

    pub fn branch_active_session(&mut self, cx: &mut Context<Self>) {
        let Some(sid) = self.active_session_id.clone() else {
            return;
        };
        if let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) {
            let live = self.live_ids.get(&sid).cloned().unwrap_or(sid);
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |this, cx| {
                let res = rt
                    .spawn(async move { ws.branch_session(&live).await })
                    .await
                    .ok()
                    .and_then(Result::ok);
                let _ = this.update(cx, |s, cx| {
                    if let Some(val) = res {
                        let new_sid = val
                            .get("stored_session_id")
                            .or_else(|| val.get("session_key"))
                            .or_else(|| val.get("session_id"))
                            .and_then(Value::as_str)
                            .map(String::from);
                        if let Some(ns) = new_sid {
                            if let Some(w) = val.get("session_id").and_then(Value::as_str) {
                                s.live_ids.insert(ns.clone(), w.to_string());
                                s.wire_to_stored.insert(w.to_string(), ns.clone());
                            }
                            s.refresh_core_capabilities(false, cx);
                            s.select_session(Some(ns), cx);
                            s.show_toast("Branched session", cx);
                            return;
                        }
                    }
                    let title = format!(
                        "{} (branch)",
                        s.active_session()
                            .map(|x| x.display_title())
                            .unwrap_or("Chat")
                    );
                    s.create_new_session(Some(title), None, None, None, None, cx);
                    s.show_toast("Branched session", cx);
                });
            })
            .detach();
        } else {
            let title = format!(
                "{} (branch)",
                self.active_session()
                    .map(|x| x.display_title())
                    .unwrap_or("Chat")
            );
            self.create_new_session(Some(title), None, None, None, None, cx);
            self.show_toast("Branched session", cx);
        }
    }

    pub fn select_model(
        &mut self,
        model: String,
        provider: Option<String>,
        save_as_default: bool,
        cx: &mut Context<Self>,
    ) {
        let prov = provider.unwrap_or_else(|| self.active_provider.clone());
        self.active_model = model.clone();
        self.active_provider = prov.clone();
        if let Some(sid) = self.active_session_id.clone() {
            if let Some(s) = self.sessions.iter_mut().find(|x| x.id == sid) {
                s.model = Some(model.clone());
                s.provider = Some(prov.clone());
            }
        }
        let ws = self.realtime.clone();
        let client = self.client.clone();
        let sid = self.active_session_id.clone();
        let live = sid
            .as_ref()
            .and_then(|s| self.live_ids.get(s).cloned())
            .or(sid);
        let rt = self.tokio_rt.clone();
        let m2 = model.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    if let (Some(w), Some(l)) = (ws, live) {
                        if w.is_connected() {
                            let val = if prov != "auto" && !prov.is_empty() {
                                format!("{m2} --provider {prov}")
                            } else {
                                m2.clone()
                            };
                            w.set_session_config(&l, "model", &val).await?;
                        }
                    }
                    if save_as_default {
                        if let Some(c) = client {
                            c.set_default_model(&m2, Some(&prov)).await?;
                        }
                    }
                    Ok::<(), String>(())
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| match res {
                Ok(()) => s.show_toast(format!("Model set to {model}"), cx),
                Err(e) => s.show_toast(e, cx),
            });
        })
        .detach();
        cx.notify();
    }

    pub fn update_bot_avatar(
        &mut self,
        seed: String,
        display_name: Option<String>,
        shape_id: Option<String>,
        color_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let entry = self.bot_avatars.entry(seed).or_default();
        if display_name.is_some() {
            entry.display_name = display_name;
        }
        if shape_id.is_some() {
            entry.shape_id = shape_id;
        }
        if color_id.is_some() {
            entry.color_id = color_id;
        }
        self.persist_board_state();
        self.bump_sidebar();
        cx.notify();
    }

    pub fn save_memory_file(&mut self, kind: &'static str, content: String, cx: &mut Context<Self>) {
        let profile = self.active_profile.clone();
        let is_local = self.connection.is_local();
        let client = self.client.clone();
        let home = self.supervisor.hermes_home().clone();
        let rt = self.tokio_rt.clone();
        let content_c = content.clone();

        cx.spawn(async move |this, cx| {
            let res: Result<(), String> = rt
                .spawn(async move {
                    if kind == "soul" {
                        if let Some(c) = client {
                            c.save_profile_soul(&profile, &content_c).await?;
                            return Ok(());
                        }
                    }
                    if is_local {
                        crate::backend::save_local_memory_file(
                            &home,
                            kind,
                            &content_c,
                            Some(&profile),
                        )
                    } else {
                        Err("Editing MEMORY.md and USER.md is available for local connections.".into())
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let _ = this.update(cx, |s, cx| match res {
                Ok(()) => {
                    match kind {
                        "soul" => s.soul_md = content,
                        "memory" => s.memory_md = content,
                        "user" => s.user_md = content,
                        _ => {}
                    }
                    s.show_toast(format!("Saved {}.md", kind.to_uppercase()), cx);
                }
                Err(e) => s.show_toast(e, cx),
            });
        })
        .detach();
    }

    // Boards & Kanban operations
    pub fn assign_session_to_board(
        &mut self,
        session_id: String,
        board_slug: String,
        cx: &mut Context<Self>,
    ) {
        self.session_boards.insert(session_id, board_slug);
        self.persist_board_state();
        self.bump_sidebar();
        cx.notify();
    }

    pub fn create_board(
        &mut self,
        name: String,
        description: String,
        color: String,
        workspace_path: String,
        cx: &mut Context<Self>,
    ) {
        let slug = slugify(&name);
        let board = HermesBoardSummary {
            slug: slug.clone(),
            name: name.trim().to_string(),
            description: Some(description.trim().to_string()),
            color: Some(color),
            workspace_path: Some(workspace_path.trim().to_string()),
            task_count: Some(0),
        };
        self.boards.retain(|b| b.slug != slug);
        self.boards.push(board.clone());
        self.active_board_slug = slug;
        self.persist_board_state();
        self.bump_sidebar();
        self.show_toast(format!("Created board \"{}\"", board.name), cx);

        let client = self.client.clone();
        let ws = self.realtime.clone();
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |_, _| {
            let _ = rt
                .spawn(async move {
                    if let Some(c) = client {
                        let _ = c.create_kanban_board(&board).await;
                    }
                    if let Some(w) = ws {
                        if w.is_connected() {
                            if let Some(wp) = board.workspace_path.as_deref().filter(|s| !s.is_empty()) {
                                let _ = w
                                    .create_project(
                                        &board.name,
                                        board.description.as_deref(),
                                        board.color.as_deref(),
                                        wp,
                                        &board.slug,
                                    )
                                    .await;
                            }
                        }
                    }
                })
                .await;
        })
        .detach();
    }

    pub fn delete_board(&mut self, slug: String, cx: &mut Context<Self>) {
        if slug == "default" {
            return;
        }
        self.boards.retain(|b| b.slug != slug);
        if self.active_board_slug == slug {
            self.active_board_slug = "all".into();
        }
        self.persist_board_state();
        self.bump_sidebar();
        self.show_toast(format!("Deleted board \"{slug}\""), cx);
        if let Some(c) = self.client.clone() {
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt.spawn(async move { c.delete_kanban_board(&slug).await }).await;
            })
            .detach();
        }
    }

    pub fn create_task(&mut self, mut task: HermesKanbanTask, cx: &mut Context<Self>) {
        let board_slug = task
            .board
            .clone()
            .filter(|b| b != "all" && !b.is_empty())
            .unwrap_or_else(|| {
                if self.active_board_slug != "all" {
                    self.active_board_slug.clone()
                } else {
                    "default".into()
                }
            });
        task.board = Some(board_slug.clone());
        let local_id = format!("task_{}", uuid::Uuid::new_v4().simple());
        task.id = local_id.clone();
        self.tasks.insert(0, task.clone());
        self.persist_board_state();
        self.show_toast(format!("Added task \"{}\"", task.title), cx);

        if let Some(c) = self.client.clone() {
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |this, cx| {
                let res = rt
                    .spawn(async move { c.create_kanban_task(&task, &board_slug).await })
                    .await
                    .ok()
                    .and_then(Result::ok);
                if let Some(server_id) = res
                    .as_ref()
                    .and_then(|v| v.get("task"))
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_str)
                {
                    let sid = server_id.to_string();
                    let _ = this.update(cx, |s, cx| {
                        if let Some(t) = s.tasks.iter_mut().find(|x| x.id == local_id) {
                            t.id = sid;
                        }
                        s.persist_board_state();
                        cx.notify();
                    });
                }
            })
            .detach();
        }
    }

    pub fn update_task(&mut self, task_id: String, updated: HermesKanbanTask, cx: &mut Context<Self>) {
        let board = updated.board.clone();
        if let Some(t) = self.tasks.iter_mut().find(|x| x.id == task_id) {
            *t = updated.clone();
        }
        self.persist_board_state();
        cx.notify();
        if let Some(c) = self.client.clone() {
            let rt = self.tokio_rt.clone();
            let patch = json!({
                "title": updated.title,
                "body": updated.body,
                "status": updated.status.as_str(),
                "priority": updated.priority_num(),
                "assignee": updated.assignee,
                "model_override": updated.model,
                "provider_override": updated.provider,
                "session_id": updated.session_id,
            });
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move {
                        c.update_kanban_task(&task_id, patch, board.as_deref())
                            .await
                    })
                    .await;
            })
            .detach();
        }
    }

    pub fn delete_task(&mut self, task_id: String, cx: &mut Context<Self>) {
        let board = self
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .and_then(|t| t.board.clone());
        self.tasks.retain(|t| t.id != task_id);
        self.persist_board_state();
        self.show_toast("Task deleted", cx);
        if let Some(c) = self.client.clone() {
            let rt = self.tokio_rt.clone();
            cx.spawn(async move |_, _| {
                let _ = rt
                    .spawn(async move { c.delete_kanban_task(&task_id, board.as_deref()).await })
                    .await;
            })
            .detach();
        }
    }

    pub fn dispatch_board_workers(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.client.clone() else {
            self.show_toast("Connect to Hermes backend to dispatch workers", cx);
            return;
        };
        let slug = self.active_board_slug.clone();
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { c.dispatch_kanban(Some(&slug)).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| match res {
                Ok(_) => {
                    s.show_toast("Dispatched Hermes board workers", cx);
                    s.refresh_boards(cx);
                    s.refresh_core_capabilities(false, cx);
                }
                Err(e) => s.show_toast(e, cx),
            });
        })
        .detach();
    }

    pub fn launch_session_for_task(&mut self, task: HermesKanbanTask, cx: &mut Context<Self>) {
        if let Some(sid) = task.session_id.clone() {
            if self.sessions.iter().any(|s| s.id == sid) {
                self.select_session(Some(sid), cx);
                return;
            }
        }
        self.create_new_session(
            Some(task.title.clone()),
            task.board.clone(),
            task.cwd.clone(),
            task.model.clone(),
            task.provider.clone(),
            cx,
        );
        let prompt = match task.body.as_deref().filter(|b| !b.trim().is_empty()) {
            Some(b) => format!("Work on task: **{}**\n\n{b}", task.title),
            None => format!("Work on task: **{}**", task.title),
        };
        let mut t2 = task.clone();
        t2.status = HermesKanbanStatus::Running;
        self.update_task(task.id.clone(), t2, cx);
        // Send after session initializes
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let _ = this.update(cx, |s, cx| {
                s.send_message(prompt, cx);
            });
        })
        .detach();
    }

    pub fn refresh_updater_status(&mut self, cx: &mut Context<Self>) {
        let updater = self.updater.clone();
        let rt = self.tokio_rt.clone();
        cx.spawn(async move |this, cx| {
            let Ok(st) = rt.spawn_blocking(move || updater.status()).await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                s.update_status = Some(st);
                cx.notify();
            });
        })
        .detach();
    }

    // Inspector helpers
    pub fn refresh_inspector_files(&mut self, cx: &mut Context<Self>) {
        let cwd = self.inspector_cwd.clone();
        let is_local = self.connection.is_local();
        let client = self.client.clone();
        let rt = self.tokio_rt.clone();
        self.inspector_files_gen = self.inspector_files_gen.wrapping_add(1);
        let gen = self.inspector_files_gen;
        self.inspector_file_error = None;
        cx.spawn(async move |this, cx| {
            let res = if is_local {
                rt.spawn_blocking(move || workspace::list_workspace_files(&cwd))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            } else {
                rt.spawn(async move {
                    if let Some(c) = client {
                        c.list_files(&cwd).await
                    } else {
                        Ok(Vec::new())
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            };
            let _ = this.update(cx, |s, cx| {
                if s.inspector_files_gen != gen {
                    return;
                }
                match res {
                    Ok(files) => {
                        s.inspector_files = files;
                        s.inspector_file_error = None;
                    }
                    Err(e) => s.inspector_file_error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn open_inspector_file(&mut self, entry: WorkspaceFileEntry, cx: &mut Context<Self>) {
        if entry.is_dir {
            self.inspector_cwd = entry.path;
            self.inspector_preview_path = None;
            self.inspector_preview_content.clear();
            self.inspector_preview_gen = self.inspector_preview_gen.wrapping_add(1);
            self.refresh_inspector_files(cx);
            cx.notify();
            return;
        }
        let path = entry.path.clone();
        self.inspector_preview_gen = self.inspector_preview_gen.wrapping_add(1);
        let gen = self.inspector_preview_gen;
        self.inspector_preview_path = Some(path.clone());
        self.inspector_preview_content = "Loading…".into();
        let is_local = self.connection.is_local();
        let client = self.client.clone();
        let rt = self.tokio_rt.clone();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let res = if is_local {
                rt.spawn_blocking(move || workspace::read_workspace_file(&path))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            } else {
                rt.spawn(async move {
                    if let Some(c) = client {
                        c.read_file(&path).await
                    } else {
                        Err("No connection".into())
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
            };
            let _ = this.update(cx, |s, cx| {
                if s.inspector_preview_gen != gen {
                    return;
                }
                s.inspector_preview_content = match res {
                    Ok(txt) => txt,
                    Err(e) => format!("Error reading file: {e}"),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn refresh_inspector_git(&mut self, cx: &mut Context<Self>) {
        let is_local = self.connection.is_local();
        let ssh = if is_local {
            None
        } else {
            self.connection.ssh.clone()
        };
        if !is_local && ssh.is_none() {
            self.inspector_git = None;
            self.inspector_git_error = None;
            cx.notify();
            return;
        }
        let cwd = self.inspector_cwd.clone();
        let rt = self.tokio_rt.clone();
        self.inspector_git_gen = self.inspector_git_gen.wrapping_add(1);
        let gen = self.inspector_git_gen;
        self.inspector_git_loading = true;
        self.inspector_git_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    workspace::git_workspace_status(
                        &cwd,
                        ssh.as_ref(),
                        workspace::GitDiffScope::Uncommitted,
                        None,
                    )
                    .await
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| {
                if s.inspector_git_gen != gen {
                    return;
                }
                s.inspector_git_loading = false;
                match res {
                    Ok(st) => s.inspector_git = Some(st),
                    Err(e) => {
                        s.inspector_git = None;
                        s.inspector_git_error = Some(e);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod stream_text_tests {
    use super::SessionChatState;

    #[test]
    fn split_delimiters_and_structured_deltas_do_not_leak_into_the_answer() {
        let mut chat = SessionChatState::default();
        chat.append_stream_text("<TH", Some("step"));
        chat.append_stream_text("INK>step one</th", Some(" one"));
        assert!(!chat.has_streamed_text);
        assert_eq!(chat.messages[0].thinking.as_deref(), Some("step one"));
        chat.append_stream_text("ink>\n\nこんにちは", None);
        assert!(chat.has_streamed_text);
        assert_eq!(chat.messages[0].content, "こんにちは");
        chat.settle_stream_text(None, None);
        assert_eq!(chat.messages[0].content, "こんにちは");
        assert!(!chat.messages[0].streaming);
    }

    #[test]
    fn final_snapshot_can_correct_visible_text_without_losing_live_thinking() {
        let mut chat = SessionChatState::default();
        chat.append_stream_text("<think>one</think>longer draft", None);
        chat.settle_stream_text(Some("<reasoning>two</reasoning>Final"), Some("one"));
        assert_eq!(chat.messages[0].content, "Final");
        assert_eq!(chat.messages[0].thinking.as_deref(), Some("one\n\ntwo"));
        chat.settle_stream_text(Some("Final"), Some("two"));
        assert_eq!(chat.messages.len(), 1);
    }

    #[test]
    fn interim_snapshot_can_end_mid_tag_and_final_snapshot_can_finish_it() {
        let mut chat = SessionChatState::default();
        chat.apply_interim_snapshot("Part one<", None);
        assert_eq!(chat.messages[0].content, "Part one");
        assert!(chat.messages[0].streaming);
        chat.append_stream_text("think>private dr", None);
        assert_eq!(chat.messages[0].content, "Part one");
        chat.settle_stream_text(Some("Part one<think>private draft</think> and two"), None);
        assert_eq!(chat.messages[0].content, "Part one and two");
        assert_eq!(chat.messages[0].thinking.as_deref(), Some("private draft"));
        assert_eq!(chat.messages.len(), 1);
    }
}
