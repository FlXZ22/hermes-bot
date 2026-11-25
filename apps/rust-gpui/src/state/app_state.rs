use crate::backend::hermes_backend::{BackendInfo, HermesSupervisor};
use crate::backend::remote::{self, RemoteLogin};
use crate::backend::sqlite_store;
use crate::backend::terminal::TerminalManager;
use crate::backend::updater::{AppUpdater, UpdateStatus as AppUpdateStatus};
use crate::backend::workspace::{self, GitWorkspaceStatus, LinuxSystemStats, WorkspaceFileEntry};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        }
    }
}

impl SessionChatState {
    pub fn bot_state(&self) -> BotFaceState {
        if self.awaiting_user_response {
            return BotFaceState::Blocked;
        }
        if self.is_streaming {
            if let Some(tool) = self.active_tool_name.as_deref() {
                return classify_hermes_activity(Some(tool), Some(&self.live_status_text));
            }
            if self.has_streamed_text {
                return BotFaceState::Streaming;
            }
            return classify_hermes_activity(None, Some(&self.live_status_text));
        }
        if let Some((outcome, settled_at)) = self.last_settlement {
            if settled_at.elapsed() < Duration::from_secs(8) {
                return match outcome {
                    TurnSettlementOutcome::Success => BotFaceState::Finished,
                    TurnSettlementOutcome::Error => BotFaceState::Failed,
                    TurnSettlementOutcome::Cancelled => BotFaceState::Idle,
                };
            }
        }
        BotFaceState::Idle
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
    pub path: String,
    pub mime: String,
    pub bytes: Vec<u8>,
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
    pub anim_tick: u64,

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
    pub usage_points: Vec<HermesUsagePoint>,
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
}

const HISTORY_PAGE_SIZE: usize = 48;

impl AppState {
    pub fn new(tokio_rt: Arc<tokio::runtime::Runtime>, cx: &mut Context<Self>) -> Self {
        let prefs = load_ui_prefs();
        let supervisor = HermesSupervisor::new();
        let home_dir = dirs::home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/home".into());

        let state = Self {
            tokio_rt,
            supervisor,
            remote_login: RemoteLogin::default(),
            terminals: TerminalManager::default(),
            updater: AppUpdater::new(),
            client: None,
            realtime: None,
            ws_generation: 0,
            prefs,
            active_view: AppViewId::Chat,
            sidebar_collapsed: false,
            right_pane_open: false,
            right_pane_tab: RightPaneTabId::Files,
            command_palette_open: false,
            model_picker_open: false,
            avatar_editor_target: None,
            toast: None,
            anim_tick: 0,
            setup_open: true,
            initializing: true,
            connection: HermesConnection::default(),
            connection_status: ConnectionStatus::Connecting,
            connection_detail: "Choose where Hermes runs".into(),
            rust_backend: None,
            hermes_status: None,
            system_stats: Some(workspace::get_linux_system_stats()),
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
            usage_points: Vec::new(),
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
        };

        // Start animation ticker (only notifies when animations or toast are active to keep idle CPU/RAM ~0)
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(260))
                    .await;
                let ok = this.update(cx, |s, cx| {
                    let mut changed = false;
                    if let Some((_, at)) = &s.toast {
                        if at.elapsed() > Duration::from_secs(3) {
                            s.toast = None;
                            changed = true;
                        }
                    }
                    let has_active_anim = s.setup_open
                        || s.initializing
                        || s.active_chat().is_streaming
                        || s.restarting_backend
                        || s.refreshing_models;
                    if has_active_anim {
                        s.anim_tick = s.anim_tick.wrapping_add(1);
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();

        state
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

    pub fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.prefs.theme = mode;
        save_ui_prefs(&self.prefs);
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
        cx.notify();
    }

    pub fn persist_board_state(&self) {
        let state = PersistedBoardState {
            boards: self.boards.clone(),
            session_boards: self.session_boards.clone(),
            tasks: self.tasks.clone(),
            bot_avatars: self.bot_avatars.clone(),
        };
        if self.connection.is_local() {
            if let Ok(val) = serde_json::to_value(&state) {
                let _ = sqlite_store::save_board_state(self.supervisor.hermes_home(), &val);
            }
        }
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
            cx.notify();
            return;
        };

        self.connection = saved.clone();
        self.initializing = true;
        self.connection_status = ConnectionStatus::Connecting;
        self.connection_detail = format!("Connecting to {}…", saved.name);
        cx.notify();

        if saved.is_local() {
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
        cx.notify();

        cx.spawn(async move |this, cx| {
            let boot_res: Result<(String, String, BackendInfo), String> = rt
                .spawn(async move {
                    sup.start_in_background().await;
                    for _ in 0..80 {
                        let info = sup.info().await;
                        if info.status == "ready" {
                            if let (Some(url), Some(token)) = (info.url.clone(), info.token.clone())
                            {
                                return Ok((url, token, info));
                            }
                        }
                        if info.status == "error" {
                            return Err(info.error.unwrap_or_else(|| {
                                "Hermes could not start. Install Hermes Agent, or connect to a server."
                                    .into()
                            }));
                        }
                        tokio::time::sleep(Duration::from_millis(350)).await;
                    }
                    Err("Hermes took too long to start. Check the runtime in Settings, or connect to a server.".into())
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
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result: Result<(HermesClient, HermesRealtimeSession, HermesStatus), String> = rt
                .spawn(async move {
                    let client = HermesClient::new(conn_clone.clone(), Some(profile.clone()));
                    let status = client.test_connection().await?;
                    let ws = HermesRealtimeSession::new(client.clone());
                    ws.connect(Some(profile)).await?;
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
                    Ok((client, ws, status))
                })
                .await
                .unwrap_or_else(|e| Err(format!("Connection task failed: {e}")));

            let _ = this.update(cx, |s, cx| match result {
                Ok((client, ws, status)) => {
                    s.ws_generation = s.ws_generation.wrapping_add(1);
                    let gen = s.ws_generation;
                    let mut rx = ws.subscribe();

                    s.connection = conn;
                    s.client = Some(client);
                    s.realtime = Some(ws);
                    s.hermes_status = Some(status);
                    s.connection_status = ConnectionStatus::Connected;
                    s.connection_detail = s.connection.name.clone();
                    s.initializing = false;
                    if auto_close_setup {
                        s.setup_open = false;
                    }
                    s.live_ids.clear();
                    s.wire_to_stored.clear();

                    if s.connection.is_local() {
                        s.hydrate_local(None);
                    }
                    s.refresh_capabilities(false, cx);
                    s.refresh_inspector_files(cx);

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
                        while let Some(ev) = ev_rx.recv().await {
                            let keep = this.update(cx, |s, cx| {
                                if s.ws_generation != gen {
                                    return false;
                                }
                                s.handle_stream_event(ev, cx);
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
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn hydrate_local(&mut self, profile: Option<&str>) {
        let home = self.supervisor.hermes_home().clone();
        let target_home =
            crate::backend::profile_home(&home, profile).unwrap_or_else(|_| home.clone());
        let boot = sqlite_store::load_local_bootstrap(&target_home);
        self.bootstrap_ms = Some(boot.elapsed_ms);
        self.system_stats = Some(workspace::get_linux_system_stats());

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
        self.soul_md = boot.soul_md;
        self.memory_md = boot.memory_md;
        self.user_md = boot.user_md;

        if let Ok(bs) = serde_json::from_value::<PersistedBoardState>(boot.board_state) {
            if !bs.boards.is_empty() {
                self.boards = bs.boards;
            }
            self.session_boards = bs.session_boards;
            self.tasks = bs.tasks;
            self.bot_avatars = bs.bot_avatars;
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

    pub fn refresh_capabilities(&mut self, refresh_models: bool, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let rt = self.tokio_rt.clone();
        let active_prof = self.active_profile.clone();

        cx.spawn(async move |this, cx| {
            let (
                status,
                profiles,
                sessions,
                models,
                model_info,
                skills,
                toolsets,
                mcp_servers,
                cron_jobs,
                platforms,
                webhooks,
                usage,
                boards,
                soul,
                raw_cfg,
                env_list,
                logs_list,
            ) = rt
                .spawn(async move {
                    tokio::join!(
                        client.get_status(),
                        client.list_profiles(),
                        client.list_sessions(120),
                        client.get_model_options(refresh_models),
                        client.get_model_info(),
                        client.list_skills(),
                        client.list_toolsets(),
                        client.list_mcp_servers(),
                        client.list_cron_jobs(),
                        client.list_messaging_platforms(),
                        client.list_webhooks(),
                        client.get_usage_analytics(14),
                        client.list_kanban_boards(),
                        client.get_profile_soul(&active_prof),
                        client.get_raw_config(),
                        client.list_env(),
                        client.get_logs(160),
                    )
                })
                .await
                .unwrap();

            // Also fetch tasks for each kanban board
            let board_tasks = if let Ok(ref b_list) = boards {
                if let Some(c2) = this
                    .read_with(cx, |s, _| s.client.clone())
                    .ok()
                    .flatten()
                {
                    let slugs: Vec<String> = b_list.iter().map(|b| b.slug.clone()).collect();
                    rt.spawn(async move {
                        let mut all = Vec::new();
                        for slug in slugs {
                            if let Ok(ts) = c2.get_kanban_board(&slug).await {
                                all.extend(ts);
                            }
                        }
                        all
                    })
                    .await
                    .unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let _ = this.update(cx, |s, cx| {
                if let Ok(st) = status {
                    s.hermes_status = Some(st);
                }
                if let Ok(ps) = profiles {
                    if !ps.is_empty() {
                        s.profiles = ps;
                    }
                }
                if let Ok(rows) = sessions {
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
                }
                if let Ok(ms) = models {
                    if !ms.is_empty() {
                        s.models = ms;
                    }
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
                if let Ok(sk) = skills {
                    s.skills = sk;
                }
                if let Ok(ts) = toolsets {
                    s.toolsets = ts;
                }
                if let Ok(mcps) = mcp_servers {
                    s.mcp_servers = mcps;
                }
                if let Ok(crons) = cron_jobs {
                    s.cron_jobs = crons;
                }
                if let Ok(plats) = platforms {
                    s.messaging_platforms = plats;
                }
                if let Ok(hooks) = webhooks {
                    s.webhooks = hooks;
                }
                if let Ok(pts) = usage {
                    s.usage_points = pts;
                }
                if let Ok(b_list) = boards {
                    for b in b_list {
                        if let Some(existing) = s.boards.iter_mut().find(|x| x.slug == b.slug) {
                            *existing = b;
                        } else {
                            s.boards.push(b);
                        }
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
                if let Ok(soul_txt) = soul {
                    if !soul_txt.is_empty() || s.soul_md.is_empty() {
                        s.soul_md = soul_txt;
                    }
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
        if self.profile_switching || name == self.active_profile {
            return;
        }
        let Some(client) = self.client.clone() else {
            self.show_toast("Connect to Hermes to switch bots", cx);
            return;
        };
        let rt = self.tokio_rt.clone();
        self.profile_switching = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let name_clone = name.clone();
            let res = rt
                .spawn(async move {
                    client.set_active_profile(&name_clone).await?;
                    client.set_profile(Some(name_clone.clone()));
                    let soul = client.get_profile_soul(&name_clone).await.unwrap_or_default();
                    Ok::<String, String>(soul)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let _ = this.update(cx, |s, cx| {
                s.profile_switching = false;
                match res {
                    Ok(soul) => {
                        s.active_profile = name.clone();
                        s.active_session_id = None;
                        s.active_view = AppViewId::Chat;
                        s.sessions.clear();
                        s.chats.clear();
                        s.live_ids.clear();
                        s.wire_to_stored.clear();
                        s.soul_md = soul;
                        for p in &mut s.profiles {
                            p.active = p.name == name;
                        }
                        if s.connection.is_local() {
                            s.hydrate_local(Some(&name));
                        }
                        s.refresh_capabilities(false, cx);
                        let label = if name == "default" { "Hermes" } else { &name };
                        s.show_toast(format!("Switched to {label}"), cx);
                    }
                    Err(err) => s.show_toast(err, cx),
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
        }
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
        let client = self.client.clone();
        let realtime = self.realtime.clone();
        let already_live = self.live_ids.contains_key(&sid);
        let rt = self.tokio_rt.clone();

        cx.spawn(async move |this, cx| {
            // Instant local SQLite read first
            if is_local && !had_messages {
                let target_home =
                    crate::backend::profile_home(&home, Some(&profile)).unwrap_or(home);
                let sid_l = sid.clone();
                let local_rows = rt
                    .spawn(async move {
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
                                    thinking: None,
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

            // Authoritative gateway history + resume WS session
            let sid_r = sid.clone();
            let prof_r = profile.clone();
            let (remote_msgs, wire_id) = rt
                .spawn(async move {
                    let msgs = if let Some(c) = client {
                        c.get_session_messages(&sid_r, HISTORY_PAGE_SIZE, 0)
                            .await
                            .ok()
                    } else {
                        None
                    };
                    let wid = if !already_live {
                        if let Some(ws) = realtime {
                            if ws.is_connected() {
                                ws.resume_session(&sid_r, &prof_r)
                                    .await
                                    .ok()
                                    .and_then(|v| {
                                        v.get("session_id").and_then(Value::as_str).map(String::from)
                                    })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    (msgs, wid)
                })
                .await
                .unwrap_or((None, None));

            let _ = this.update(cx, |s, cx| {
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

        cx.spawn(async move |this, cx| {
            let fallback_id = format!("session_{}", uuid::Uuid::new_v4().simple());
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
                .spawn(async move {
                    if let Some(ws) = realtime {
                        if ws.is_connected() {
                            return ws.create_session(params).await.ok();
                        }
                    }
                    None
                })
                .await
                .unwrap_or(None);

            let (new_id, wire_id) = if let Some(res) = created {
                let stored = res
                    .get("stored_session_id")
                    .or_else(|| res.get("session_key"))
                    .or_else(|| res.get("session_id"))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .unwrap_or(fallback_id);
                let wire = res
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(String::from);
                (stored, wire)
            } else {
                (fallback_id, None)
            };

            let _ = this.update(cx, |s, cx| {
                if let Some(wid) = wire_id {
                    s.live_ids.insert(new_id.clone(), wid.clone());
                    s.wire_to_stored.insert(wid, new_id.clone());
                }
                let sess = HermesSession {
                    id: new_id.clone(),
                    title: Some(title.unwrap_or_else(|| "New Chat".into())),
                    profile: Some(s.active_profile.clone()),
                    model: Some(resolved_model.clone()),
                    provider: Some(resolved_provider.clone()),
                    source: Some("desktop".into()),
                    cwd: resolved_cwd,
                    board_slug: Some(b_slug.clone()),
                    message_count: Some(0),
                    created_at: Some(json!(now_secs())),
                    updated_at: Some(json!(now_secs())),
                    ..Default::default()
                };
                s.draft_session_ids.insert(new_id.clone());
                s.sessions.retain(|x| x.id != new_id);
                s.sessions.insert(0, sess);
                s.session_boards.insert(new_id.clone(), b_slug);
                s.persist_board_state();
                s.active_model = resolved_model;
                s.active_provider = resolved_provider;
                s.active_session_id = Some(new_id);
                s.active_view = AppViewId::Chat;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn send_message(&mut self, raw_text: String, cx: &mut Context<Self>) {
        let attachments = std::mem::take(&mut self.attachments);
        let text = if raw_text.trim().is_empty() && !attachments.is_empty() {
            "Please review the attached files.".to_string()
        } else {
            raw_text.trim().to_string()
        };
        if text.is_empty() {
            return;
        }

        // Client-side UI shortcut slash commands
        match text.as_str() {
            "/model" => {
                self.model_picker_open = true;
                cx.notify();
                return;
            }
            "/restart" => {
                self.restart_backend(cx);
                return;
            }
            "/boards" => {
                self.active_view = AppViewId::Boards;
                cx.notify();
                return;
            }
            "/skills" => {
                self.active_view = AppViewId::Skills;
                cx.notify();
                return;
            }
            "/cron" => {
                self.active_view = AppViewId::Cron;
                cx.notify();
                return;
            }
            "/memory" => {
                self.active_view = AppViewId::Memory;
                cx.notify();
                return;
            }
            "/clear" => {
                self.create_new_session(None, None, None, None, None, cx);
                return;
            }
            "/compress" => {
                self.compress_active_session(cx);
                return;
            }
            "/branch" => {
                self.branch_active_session(cx);
                return;
            }
            _ => {}
        }

        let Some(ws) = self.realtime.clone().filter(|w| w.is_connected()) else {
            self.show_toast(
                "Hermes is disconnected. Check Settings → Connections.",
                cx,
            );
            return;
        };

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
            self.show_toast("Wait for the current response before sending another message.", cx);
            return;
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
        let chat = self.chats.entry(sid.clone()).or_default();
        chat.messages.push(user_msg);
        chat.is_streaming = true;
        chat.has_streamed_text = false;
        chat.active_tool_name = None;
        chat.last_settlement = None;
        chat.live_status_text = "Thinking...".into();

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
        let profile = self.active_profile.clone();
        let model = self.active_model.clone();
        let provider = self.active_provider.clone();
        let effort = self.reasoning_effort.as_str().to_string();
        let fast = self.fast_mode;
        let yolo = self.yolo_mode;
        let sess_cwd = self.active_session().and_then(|s| s.cwd.clone());
        let sess_title = self.active_session().and_then(|s| s.title.clone());
        let rt = self.tokio_rt.clone();

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

                    // Gateway slash command check
                    if text.starts_with('/') {
                        if let Ok(cmd_res) = ws.dispatch_command(&live, &text).await {
                            if let Some(out) = cmd_res
                                .get("output")
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                            {
                                return Ok((live, Some(out.to_string())));
                            }
                        }
                    }

                    let _ = ws.set_session_config(&live, "reasoning", &effort).await;
                    let _ = ws
                        .set_session_config(&live, "fast", if fast { "on" } else { "off" })
                        .await;
                    let _ = ws
                        .set_session_config(&live, "yolo", if yolo { "on" } else { "off" })
                        .await;

                    // Attachments
                    let mut prompt = text;
                    for att in attachments {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&att.bytes);
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
                    s.show_toast(err, cx);
                    cx.notify();
                }
            });
        })
        .detach();
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
            for m in &mut c.messages {
                m.streaming = false;
            }
        }
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
                        self.connection_status = ConnectionStatus::Connected;
                        self.connection_detail = "Live Hermes Gateway".into();
                        self.refresh_capabilities(false, cx);
                    }
                    "connecting" => {
                        self.connection_status = ConnectionStatus::Connecting;
                        self.connection_detail = "Connecting WebSocket...".into();
                    }
                    "disconnected" => {
                        self.connection_status = ConnectionStatus::Disconnected;
                        self.connection_detail = "Connection lost. Reconnecting…".into();
                        self.live_ids.clear();
                        self.wire_to_stored.clear();
                        for chat in self.chats.values_mut() {
                            chat.is_streaming = false;
                        }
                    }
                    _ => {
                        self.connection_status = ConnectionStatus::Error;
                        self.connection_detail = detail.unwrap_or_else(|| "WebSocket error".into());
                    }
                }
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
            | HermesStreamEvent::MessageStart { session_id }
            | HermesStreamEvent::TextDelta { session_id, .. }
            | HermesStreamEvent::ThinkingDelta { session_id, .. }
            | HermesStreamEvent::MessageInterim { session_id, .. }
            | HermesStreamEvent::ToolStart { session_id, .. }
            | HermesStreamEvent::ToolProgress { session_id, .. }
            | HermesStreamEvent::ToolEnd { session_id, .. }
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
            HermesStreamEvent::SessionUsage { usage, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.session_usage = Some(usage);
            }
            HermesStreamEvent::StatusUpdate { text, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.live_status_text = text;
            }
            HermesStreamEvent::TodoUpdated { todos, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.todos = todos;
            }
            HermesStreamEvent::MessageStart { .. } => {
                let chat = self.chats.entry(target_sid).or_default();
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
            }
            HermesStreamEvent::TextDelta { delta, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.has_streamed_text = true;
                if let Some(m) = chat.messages.iter_mut().rev().find(|m| m.streaming) {
                    m.content.push_str(&delta);
                } else {
                    let mut m =
                        HermesMessage::new_assistant(format!("asst-{}", uuid::Uuid::new_v4().simple()));
                    m.content.push_str(&delta);
                    chat.messages.push(m);
                }
            }
            HermesStreamEvent::ThinkingDelta { delta, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.has_streamed_text = false;
                chat.active_tool_name = None;
                if let Some(m) = chat.messages.iter_mut().rev().find(|m| m.streaming) {
                    m.thinking.get_or_insert_with(String::new).push_str(&delta);
                } else {
                    let mut m =
                        HermesMessage::new_assistant(format!("asst-{}", uuid::Uuid::new_v4().simple()));
                    m.thinking = Some(delta);
                    chat.messages.push(m);
                }
            }
            HermesStreamEvent::MessageInterim { content, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                for m in &mut chat.messages {
                    if m.streaming {
                        m.content = content.clone();
                        m.streaming = false;
                    }
                }
            }
            HermesStreamEvent::ToolStart { tool_call, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = true;
                chat.has_streamed_text = false;
                chat.active_tool_name = Some(tool_call.name.clone());
                chat.live_status_text = format!("Running {}…", tool_call.name);
                if let Some(m) = chat.messages.iter_mut().rev().find(|m| m.streaming) {
                    m.tool_calls.push(tool_call);
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
                ..
            } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.live_status_text.clear();
                chat.active_tool_name = None;
                for m in chat.messages.iter_mut().rev() {
                    if let Some(tc) = m.tool_calls.iter_mut().find(|t| {
                        (!tool_call_id.is_empty() && t.id == tool_call_id)
                            || t.status == ToolCallStatus::Running
                    }) {
                        tc.status = if error.is_some() {
                            ToolCallStatus::Error
                        } else {
                            ToolCallStatus::Done
                        };
                        tc.output = error.or(output).or(tc.output.take());
                        tc.duration_ms = duration_ms;
                        break;
                    }
                }
            }
            HermesStreamEvent::ApprovalRequest { approval, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                let id = format!("appr-{}", approval.id);
                chat.awaiting_user_response = true;
                if !chat.messages.iter().any(|m| m.id == id) {
                    let mut m = HermesMessage::new_assistant(id);
                    m.streaming = false;
                    m.approval = Some(approval);
                    chat.messages.push(m);
                }
            }
            HermesStreamEvent::ApprovalSettled { id, decision, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                for m in &mut chat.messages {
                    if let Some(ap) = &mut m.approval {
                        if ap.id == id {
                            ap.settled = Some(decision.clone());
                        }
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
            HermesStreamEvent::MessageComplete { content, .. } => {
                let chat = self.chats.entry(target_sid.clone()).or_default();
                chat.is_streaming = false;
                chat.has_streamed_text = false;
                chat.active_tool_name = None;
                chat.live_status_text.clear();
                chat.last_settlement = Some((TurnSettlementOutcome::Success, Instant::now()));
                let mut found_streaming = false;
                for m in chat.messages.iter_mut().rev() {
                    if m.streaming {
                        if let Some(ref c) = content {
                            if m.content.is_empty() || c.len() >= m.content.len() {
                                m.content = c.clone();
                            }
                        }
                        m.streaming = false;
                        if m.model.is_none() {
                            m.model = Some(self.active_model.clone());
                        }
                        found_streaming = true;
                        break;
                    }
                }
                if !found_streaming {
                    if let Some(c) = content.filter(|s| !s.is_empty()) {
                        let mut m = HermesMessage::new_assistant(format!(
                            "asst-{}",
                            uuid::Uuid::new_v4().simple()
                        ));
                        m.content = c;
                        m.streaming = false;
                        m.model = Some(self.active_model.clone());
                        chat.messages.push(m);
                    }
                }
            }
            HermesStreamEvent::Error { message, .. } => {
                let chat = self.chats.entry(target_sid).or_default();
                chat.is_streaming = false;
                chat.last_settlement = Some((TurnSettlementOutcome::Error, Instant::now()));
                for m in &mut chat.messages {
                    m.streaming = false;
                }
                let mut m =
                    HermesMessage::new_assistant(format!("err-{}", uuid::Uuid::new_v4().simple()));
                m.content = message.clone();
                m.streaming = false;
                chat.messages.push(m);
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
                            s.refresh_capabilities(false, cx);
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
                    s.refresh_capabilities(false, cx);
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

    // Inspector helpers
    pub fn refresh_inspector_files(&mut self, cx: &mut Context<Self>) {
        let cwd = self.inspector_cwd.clone();
        let is_local = self.connection.is_local();
        let client = self.client.clone();
        let rt = self.tokio_rt.clone();
        self.inspector_file_error = None;
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    if is_local {
                        workspace::list_workspace_files(&cwd)
                    } else if let Some(c) = client {
                        c.list_files(&cwd).await
                    } else {
                        Ok(Vec::new())
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| {
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
            self.refresh_inspector_files(cx);
            cx.notify();
            return;
        }
        let path = entry.path.clone();
        self.inspector_preview_path = Some(path.clone());
        self.inspector_preview_content = "Loading…".into();
        let is_local = self.connection.is_local();
        let client = self.client.clone();
        let rt = self.tokio_rt.clone();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    if is_local {
                        workspace::read_workspace_file(&path)
                    } else if let Some(c) = client {
                        c.read_file(&path).await
                    } else {
                        Err("No connection".into())
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |s, cx| {
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
