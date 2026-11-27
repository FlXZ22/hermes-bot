use crate::harness::events::{ConversationMode, ExecutionState, SlashCommandInfo};
use crate::harness::goal_worker::{GoalActivation, GoalPhase, GoalRuntimeSnapshot};
use crate::harness::metrics::{format_elapsed_ms, MetricProvenance, SessionLiveMetrics};
use crate::harness::ptc::inspect_ptc_tool_call;
use crate::hermes::types::{
    ApprovalDecision, HermesApprovalRequest, HermesClarifyRequest, HermesMessage,
    HermesSessionUsage, HermesToolCall, MessageRole, ToolCallStatus,
};
use crate::state::app_state::{AppState, AttachmentFile, AvatarEditorTarget, RightPaneTabId};
use crate::theme::bot_face::{bot_face_with_options, describe_tool, BotFaceOptions, BotFaceState};
use crate::theme::components::{
    cursor_icon, format_tokens, gb_badge, gb_btn, gb_chip_btn, gb_dot, gb_mini_icon_btn,
    gb_section_card, gb_section_card_head, gb_window_action, short_model_name, BadgeTone,
    BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::{resolve_bot_color, SandTheme};
use crate::ui::markdown::{
    render_code_block, render_highlighted_code_block, render_markdown, render_thinking_markdown,
};
use crate::ui::motion::{
    gpu_spinner, live_pulse_dot, pill_enter, popover_enter, typing_dots, view_enter, EntranceKey,
    SpringValue,
};
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::Sizable;
use std::cell::{Cell, RefCell};
use std::collections::{hash_map::DefaultHasher, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

const STARTER_PROMPTS: &[(&str, &str, &str, &str)] = &[
    (
        "code",
        "Inspect workspace repo",
        "Summarize architecture, git status, and recent commits",
        "Inspect the current workspace directory, list key files, and summarize the architecture.",
    ),
    (
        "board-kanban",
        "Plan a Session Board",
        "Break down a feature into actionable Kanban tasks",
        "Help me break down my next project milestone into structured tasks for our Session Board.",
    ),
    (
        "sparkles",
        "Create a reusable Skill",
        "Author a new SKILL.md for Hermes Agent",
        "Help me design and write a new Hermes Agent skill (SKILL.md) for my daily Arch Linux workflow.",
    ),
    (
        "terminal",
        "Run diagnostics & check health",
        "Check system resources, tools, and active MCP servers",
        "Check the local Linux environment, active tools, and verify everything is running smoothly.",
    ),
];

// Six complete 21px lines fit inside Electron's 140px field with 4px padding on each edge.
const COMPOSER_MAX_ROWS: usize = 6;
const COMPOSER_MAX_HEIGHT: f32 = 140.0;

type ThoughtScrollHitboxes = Rc<RefCell<Vec<Hitbox>>>;

fn row_fingerprint(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn message_fingerprint(msg: &HermesMessage) -> u64 {
    row_fingerprint((
        &msg.id, &msg.role, &msg.content, &msg.thinking, &msg.tool_name, &msg.model,
        &msg.tool_calls, &msg.approval, &msg.clarify, msg.streaming,
    ))
}

fn message_has_body(msg: &HermesMessage) -> bool {
    matches!(msg.role, MessageRole::User | MessageRole::Tool)
        || thinking_is_live(msg)
        || !msg.content.trim().is_empty()
        || msg.thinking.as_ref().is_some_and(|s| !s.is_empty())
        || !msg.tool_calls.is_empty()
        || msg.approval.is_some()
        || msg.clarify.is_some()
}

fn thinking_is_live(msg: &HermesMessage) -> bool {
    msg.streaming
        && msg.content.trim().is_empty()
        && !msg.tool_calls.iter().any(|t| t.status == ToolCallStatus::Running)
}

/// Collapsed-by-default activity disclosure:
/// - Collapsed by default during streaming AND auto-collapses when the turn completes.
/// - User can expand while streaming (`expanded_live`), which clears automatically on completion.
/// - User can reopen any completed or failed turn (`expanded_done`) at any time.
#[derive(Default)]
struct ActivityDisclosure {
    expanded_live: HashSet<String>,
    expanded_done: HashSet<String>,
}

impl ActivityDisclosure {
    fn is_expanded(&self, msg: &HermesMessage) -> bool {
        if msg.streaming {
            self.expanded_live.contains(&msg.id)
        } else {
            self.expanded_done.contains(&msg.id)
        }
    }

    fn toggle(&mut self, id: &str, streaming: bool) {
        let target = if streaming {
            &mut self.expanded_live
        } else {
            &mut self.expanded_done
        };
        if !target.remove(id) {
            target.insert(id.to_owned());
        }
    }

    fn prune_completed(&mut self, messages: &[HermesMessage]) {
        self.expanded_live
            .retain(|id| messages.iter().any(|m| m.id == *id && m.streaming));
    }
}

fn chat_row() -> Div {
    div().w_full().min_w_0().overflow_hidden()
}

fn clipped_label(text: impl Into<SharedString>) -> Div {
    div().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(text.into())
}

pub struct ChatView {
    app: Entity<AppState>,
    composer: Entity<TextareaState>,
    composer_overflowing: bool,
    title_input: Entity<InputState>,
    editing_title: bool,
    attach_menu_open: bool,
    board_menu_open: bool,
    activity_disclosure: ActivityDisclosure,
    expanded_tools: HashSet<String>,
    ptc_json_mode: HashSet<String>,
    // Virtualized Zed ListState + 120Hz smooth scroller for chat transcript
    list_state: ListState,
    smooth_scroller: SmoothScroller,
    synced_session_id: Option<String>,
    synced_prefix_count: usize,
    synced_prefix_kind: (bool, bool),
    synced_first_msg_id: Option<String>,
    synced_msg_count: usize,
    synced_has_pending: bool,
    synced_last_fp: Option<u64>,
    synced_messages: Vec<(String, u64)>,
    synced_aux_fp: (u64, u64),
    measured_width: Rc<Cell<Option<Pixels>>>,
    empty_hero_entrance: EntranceKey<bool>,
    jump_pill_entrance: EntranceKey<bool>,
    usage_fill: SpringValue,
    usage_session: Option<String>,
}

impl ChatView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, COMPOSER_MAX_ROWS));
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Session title"));

        cx.observe(&app, |_, app_e, cx| {
            let s = app_e.read(cx);
            if !s.setup_open
                && !s.initializing
                && s.active_view == crate::state::app_state::AppViewId::Chat
            {
                cx.notify();
            }
        })
        .detach();

        cx.observe(&composer, |this, composer, cx| {
            let overflowing = composer.read(cx).display_map().wrap_row_count() > COMPOSER_MAX_ROWS;
            if this.composer_overflowing != overflowing {
                this.composer_overflowing = overflowing;
                cx.notify();
            }
        })
        .detach();

        cx.subscribe_in(&composer, window, |this, _state, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    if !*shift && !this.app.read(cx).active_is_streaming() {
                        this.submit_composer(window, cx);
                    }
                }
                _ => {}
            }
        })
        .detach();

        cx.subscribe_in(&title_input, window, |this, inp, ev: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                if let Some(sid) = this.app.read(cx).active_session_id.clone() {
                    let val = inp.read(cx).value().to_string();
                    this.app.update(cx, |s, cx| s.rename_session(sid, val, cx));
                }
                this.editing_title = false;
                cx.notify();
            }
        })
        .detach();

        let list_state = ListState::new(0, ListAlignment::Top, px(400.0));
        list_state.set_follow_mode(FollowMode::Tail);
        let weak_self = cx.weak_entity();
        list_state.set_scroll_handler(move |_, _, cx| {
            let weak_self = weak_self.clone();
            cx.defer(move |cx| {
                let _ = weak_self.update(cx, |_, cx| cx.notify());
            });
        });
        let smooth_scroller = SmoothScroller::for_list(list_state.clone());

        Self {
            app,
            composer,
            composer_overflowing: false,
            title_input,
            editing_title: false,
            attach_menu_open: false,
            board_menu_open: false,
            activity_disclosure: ActivityDisclosure::default(),
            expanded_tools: HashSet::new(),
            ptc_json_mode: HashSet::new(),
            list_state,
            smooth_scroller,
            synced_session_id: None,
            synced_prefix_count: 0,
            synced_prefix_kind: (false, false),
            synced_first_msg_id: None,
            synced_msg_count: 0,
            synced_has_pending: false,
            synced_last_fp: None,
            synced_messages: Vec::new(),
            synced_aux_fp: (0, 0),
            measured_width: Rc::new(Cell::new(None)),
            empty_hero_entrance: EntranceKey::new(),
            jump_pill_entrance: EntranceKey::new(),
            usage_fill: SpringValue::new(0.0).with_precision(0.05),
            usage_session: None,
        }
    }

    fn submit_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty() && self.app.read(cx).attachments.is_empty() {
            return;
        }
        let accepted = self.app.update(cx, |s, cx| s.send_message(text, cx));
        if accepted {
            self.composer
                .update(cx, |st, cx| st.set_value("", window, cx));
            self.smooth_scroller.cancel();
            self.list_state.set_follow_mode(FollowMode::Tail);
            self.list_state.scroll_to_end();
        }
        cx.notify();
    }

    fn pick_attachment_files(&mut self, cx: &mut Context<Self>) {
        self.attach_menu_open = false;
        let rt = self.app.read(cx).tokio_rt.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files to Hermes message".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                let loaded = rt
                    .spawn_blocking(move || {
                        let mut loaded = Vec::new();
                        for path in paths {
                            if let Ok(meta) = std::fs::metadata(&path) {
                                if meta.len() > 50 * 1024 * 1024 {
                                    continue;
                                }
                                if let Ok(bytes) = std::fs::read(&path) {
                                    let name = path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "file".into());
                                    let ext = path
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    let mime = match ext.as_str() {
                                        "png" => "image/png",
                                        "jpg" | "jpeg" => "image/jpeg",
                                        "webp" => "image/webp",
                                        "gif" => "image/gif",
                                        "pdf" => "application/pdf",
                                        _ => "application/octet-stream",
                                    }
                                    .to_string();
                                    loaded.push(AttachmentFile {
                                        name,
                                        path: path.to_string_lossy().into_owned(),
                                        mime,
                                        bytes: std::sync::Arc::new(bytes),
                                    });
                                }
                            }
                        }
                        loaded
                    })
                    .await
                    .unwrap_or_default();
                if !loaded.is_empty() {
                    let _ = this.update(cx, |v, cx| {
                        v.app.update(cx, |s, cx| {
                            s.attachments.extend(loaded);
                            cx.notify();
                        });
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::face::FaceLayer::begin_pane();
        let s = self.app.read(cx);
        let theme = s.sand_theme();
        let active_session = s.active_session().cloned();
        let active_session_id = s.active_session_id.clone();
        let active_profile = s.active_profile.clone();
        let bot_seed = s.active_bot_seed();
        let bot_av = s.bot_avatars.get(&bot_seed).cloned();
        let bot_name = bot_av
            .as_ref()
            .and_then(|a| a.display_name.clone())
            .unwrap_or_else(|| {
                if active_profile == "default" {
                    "Hermes".into()
                } else {
                    active_profile.clone()
                }
            });
        let active_model = s.active_model.clone();
        let effort = s.reasoning_effort;
        let fast = s.fast_mode;
        let yolo = s.yolo_mode;
        let right_open = s.right_pane_open;
        let boards = s.boards.clone();
        let attachments = s.attachments.clone();

        let current_board_slug = active_session_id
            .as_ref()
            .and_then(|id| s.session_boards.get(id).cloned())
            .or_else(|| active_session.as_ref().and_then(|x| x.board_slug.clone()))
            .unwrap_or_else(|| {
                if s.active_board_slug != "all" {
                    s.active_board_slug.clone()
                } else {
                    "default".into()
                }
            });
        let current_board = boards
            .iter()
            .find(|b| b.slug == current_board_slug)
            .or_else(|| boards.first())
            .cloned();

        let chat = s.active_chat();
        let exec_state = chat.execution_state();
        let bot_state = chat.bot_state();
        let reduced_motion = s.reduced_motion();
        let conv_mode = s.active_conversation_mode();
        let mode_locked = s.is_active_mode_locked();
        let active_project = s.active_work_project().cloned();
        let active_work_cwd = s.active_work_cwd();
        let active_goal = s.active_goal_snapshot();
        let metrics = s.active_metrics();
        let git_status = s.inspector_git.clone();
        let subagents = s.active_subagents();
        let all_slash_commands = s.slash_commands.clone();
        let has_messages = !chat.messages.is_empty();
        let loading_history = chat.loading_history;
        let is_streaming = chat.is_streaming;
        let empty_id = self.empty_hero_entrance.id("chat-empty-hero", !has_messages && !loading_history);
        if self.usage_session != active_session_id {
            self.usage_session.clone_from(&active_session_id);
            self.usage_fill.snap(0.0);
        }
        let usage_bar = render_usage_bar(
            &theme,
            chat.session_usage.as_ref(),
            active_session.as_ref().and_then(|x| x.context_limit),
            &mut self.usage_fill,
            window,
        );

        let draft = self.composer.read(cx).value().to_string();
        let placeholder = if conv_mode == ConversationMode::Work {
            format!("Work with {bot_name}… (type / for commands or /goal)")
        } else {
            format!("Message {bot_name}… (type / for commands)")
        };
        if self.composer.read(cx).presentation().placeholder().as_ref() != placeholder {
            self.composer
                .update(cx, |state, cx| state.set_placeholder(placeholder, window, cx));
        }
        let matching_slash: Vec<SlashCommandInfo> = if draft.starts_with('/') && !draft.contains('\n') {
            let token = draft.split_whitespace().next().unwrap_or("/").to_lowercase();
            if draft.contains(' ') {
                Vec::new()
            } else {
                let q = token.trim_start_matches('/');
                all_slash_commands
                    .into_iter()
                    .filter(|c| {
                        q.is_empty()
                            || c.name.to_lowercase().contains(q)
                            || c.description.to_lowercase().contains(q)
                            || c.category.to_lowercase().contains(q)
                            || c.aliases.iter().any(|a| a.to_lowercase().contains(q))
                    })
                    .take(14)
                    .collect()
            }
        } else {
            Vec::new()
        };

        let header_title = active_session
            .as_ref()
            .and_then(|x| x.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| bot_name.clone());

        let bot_seed_hdr = bot_seed.clone();
        let bot_name_hdr = bot_name.clone();
        let hdr_title_edit = header_title.clone();
        let scale = window.scale_factor();
        let composer_border = px((0.5 * scale).floor().max(1.0) / scale);

        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(theme.bg_pane)
            .text_color(theme.text_primary)
            .font_family(FONT_SANS)
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            // Top Chat Header (52px, floating Grok Bot frosted title pill + right window actions)
            .child(
                div()
                    .h(px(52.0))
                    .w_full()
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_shrink_0()
                    .min_w_0()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .child(if self.editing_title && active_session_id.is_some() {
                                div()
                                    .h(px(32.0))
                                    .max_w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .px(px(10.0))
                                    .rounded_full()
                                    .bg(theme.bg_chrome)
                                    .border_1()
                                    .border_color(theme.border_default)
                                    .flex()
                                    .items_center()
                                    .gap(px(7.0))
                                    .child(bot_face_with_options(
                                        &theme,
                                        &bot_seed,
                                        bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                                        bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                                        24.0,
                                        bot_state,
                                        BotFaceOptions {
                                            slot_key: "chat-title",
                                            is_static: bot_state == BotFaceState::Idle,
                                            ..Default::default()
                                        },
                                    ))
                                    .child(div().w(px(220.0)).max_w_full().min_w_0().child(Input::new(&self.title_input).h(px(24.0))))
                                    .into_any_element()
                            } else {
                                let has_sid = active_session_id.is_some();
                                let seed_face = bot_seed_hdr.clone();
                                let name_face = bot_name_hdr.clone();
                                div()
                                    .id("chat-title-pill")
                                    .h(px(32.0))
                                    .pl(px(8.0))
                                    .pr(px(12.0))
                                    .rounded_full()
                                    .bg(theme.bg_chrome)
                                    .border_1()
                                    .border_color(theme.border_default)
                                    .flex()
                                    .items_center()
                                    .gap(px(7.0))
                                    .max_w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .cursor_pointer()
                                    .hover(|st| st.bg(theme.bg_elevated))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if has_sid {
                                            let t = hdr_title_edit.clone();
                                            this.title_input
                                                .update(cx, |st, cx| st.set_value(t, window, cx));
                                            this.editing_title = true;
                                            cx.notify();
                                        } else {
                                            this.app.update(cx, |s, cx| {
                                                s.avatar_editor_target = Some(AvatarEditorTarget {
                                                    seed: bot_seed_hdr.clone(),
                                                    title: bot_name_hdr.clone(),
                                                });
                                                cx.notify();
                                            });
                                        }
                                    }))
                                    .child(
                                        div()
                                            .id("chat-title-avatar")
                                            .flex_shrink_0()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.app.update(cx, |s, cx| {
                                                    s.avatar_editor_target = Some(AvatarEditorTarget {
                                                        seed: seed_face.clone(),
                                                        title: name_face.clone(),
                                                    });
                                                    cx.notify();
                                                });
                                            }))
                                            .child(bot_face_with_options(
                                                &theme,
                                                &bot_seed,
                                                bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                                                bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                                                24.0,
                                                bot_state,
                                                BotFaceOptions {
                                                    slot_key: "chat-title",
                                                    is_static: bot_state == BotFaceState::Idle,
                                                    ..Default::default()
                                                },
                                            )),
                                    )
                                    .child(
                                        clipped_label(header_title)
                                            .text_size(px(13.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary),
                                    )
                                    .child(
                                        clipped_label(format!("· {}", short_model_name(&active_model)))
                                            .max_w(px(128.0))
                                            .text_size(px(12.0))
                                            .text_color(theme.text_tertiary),
                                    )
                                    .into_any_element()
                            }),
                    )
                    // Centered Chat | Work Segmented Toggle (unlocked before first message; locked badge after)
                    .child(self.render_mode_toggle(&theme, conv_mode, mode_locked, active_project.as_ref(), cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .justify_end()
                            .items_center()
                            .gap(px(6.0))
                            .when(active_session_id.is_some(), |d| {
                                d.child(
                                    gb_window_action("chat-branch-btn", &theme, false)
                                        .child(cursor_icon("git-branch", 14.0, theme.text_secondary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| s.branch_active_session(cx));
                                        })),
                                )
                                .child(
                                    gb_window_action("chat-compress-btn", &theme, false)
                                        .child(cursor_icon(
                                            "arrows-contract-simple",
                                            14.0,
                                            theme.text_secondary,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app
                                                .update(cx, |s, cx| s.compress_active_session(cx));
                                        })),
                                )
                            })
                            .child(
                                gb_window_action("chat-new-btn", &theme, false)
                                    .child(cursor_icon("chat-bubble-plus", 14.0, theme.text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| {
                                            s.create_new_session(None, None, None, None, None, cx);
                                        });
                                    })),
                            )
                            .child(
                                gb_window_action("chat-inspector-btn", &theme, right_open)
                                    .child(cursor_icon(
                                        if right_open {
                                            "layout-sidebar-right-on"
                                        } else {
                                            "layout-sidebar-right"
                                        },
                                        14.0,
                                        if right_open {
                                            theme.agent_accent
                                        } else {
                                            theme.text_secondary
                                        },
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| {
                                            s.right_pane_open = !s.right_pane_open;
                                            if s.right_pane_open {
                                                match s.right_pane_tab {
                                                    crate::state::app_state::RightPaneTabId::Files => {
                                                        s.refresh_inspector_files(cx);
                                                    }
                                                    crate::state::app_state::RightPaneTabId::Git => {
                                                        s.refresh_inspector_git(cx);
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            s.bump_shell();
                                            cx.notify();
                                        });
                                    })),
                            ),
                    ),
            )
            // Work Mode Live Context & Metrics Bar (project, dir, git branch/+/-, elapsed, tokens/sec)
            .when(conv_mode == ConversationMode::Work, |d| {
                d.child(self.render_work_metrics_bar(
                    &theme,
                    active_project.as_ref(),
                    active_work_cwd.as_deref(),
                    git_status.as_ref(),
                    &metrics,
                    exec_state,
                    subagents.len(),
                    cx,
                ))
            })
            // Center Body: Loading | Empty Hero | Transcript
            .child(if !has_messages && loading_history {
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child(gpu_spinner(18.0, theme.text_tertiary))
                    .child("Loading conversation…")
                    .into_any_element()
            } else if !has_messages {
                // Exact Grok Bot Empty Chat Hero (matches verification-chat.jpg!)
                view_enter(empty_id, div()
                    .flex_1()
                    .w_full()
                    .px(px(24.0))
                    .pb(px(40.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(14.0))
                    .child(
                        div()
                            .id("chat-hero-bot")
                            .p(px(6.0))
                            .rounded(px(28.0))
                            .cursor_pointer()
                            .child(bot_face_with_options(
                                &theme,
                                &bot_seed,
                                bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                                bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                                72.0,
                                bot_state,
                                BotFaceOptions {
                                    slot_key: "chat-hero",
                                    follow_pointer: true,
                                    clickable_spin: true,
                                    ..Default::default()
                                },
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .line_height(px(30.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(if conv_mode == ConversationMode::Work {
                                "What should we work on?".to_string()
                            } else {
                                bot_name
                            }),
                    )
                    .child(
                        div()
                            .max_w(px(460.0))
                            .text_center()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(theme.text_secondary)
                            .child(if conv_mode == ConversationMode::Work {
                                if let Some(ref proj) = active_project {
                                    format!("Working in {} ({}) · tools, Git diffs, trajectory & /goal ready.", proj.name, proj.cwd)
                                } else {
                                    "Select a project directory on your Hermes server to unlock tools, diffs, trajectory, and /goal.".to_string()
                                }
                            } else {
                                "Your agent, ready to work. Start a conversation or pick up where you left off.".to_string()
                            }),
                    )
                    // 2x2 starter prompt cards
                    .child(
                        div()
                            .w(px(560.0))
                            .max_w_full()
                            .mt(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .children(STARTER_PROMPTS.chunks(2).enumerate().map(|(r_idx, row)| {
                                div()
                                    .w_full()
                                    .flex()
                                    .gap(px(8.0))
                                    .children(row.iter().enumerate().map(|(c_idx, &(icon, title, desc, prompt))| {
                                        let prompt_s = prompt.to_string();
                                        div()
                                            .id(SharedString::from(format!("starter-{r_idx}-{c_idx}")))
                                            .flex_1()
                                            .min_w_0()
                                            .px(px(13.0))
                                            .py(px(11.0))
                                            .rounded(px(14.0))
                                            .bg(theme.bg_elevated)
                                            .border_1()
                                            .border_color(theme.border_default)
                                            .overflow_hidden()
                                            .flex()
                                            .flex_col()
                                            .gap(px(3.0))
                                            .cursor_pointer()
                                            .hover(|st| {
                                                st.bg(theme.bg_elevated_hover)
                                                    .border_color(theme.border_primary)
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.app
                                                    .update(cx, |s, cx| s.send_message(prompt_s.clone(), cx));
                                            }))
                                            .child(
                                                div()
                                                    .w_full()
                                                    .min_w_0()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(theme.text_primary)
                                                    .child(cursor_icon(icon, 14.0, theme.text_primary))
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .overflow_hidden()
                                                            .text_ellipsis()
                                                            .whitespace_nowrap()
                                                            .child(title),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .line_height(px(17.0))
                                                    .text_color(theme.text_tertiary)
                                                    .child(desc),
                                            )
                                    }))
                            })),
                    ))
                    .into_any_element()
            } else {
                // Transcript scroller
                self.render_transcript(
                    &theme,
                    chat,
                    &bot_seed,
                    bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                    bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                    bot_state,
                    exec_state,
                    reduced_motion,
                    cx,
                )
                .into_any_element()
            })
            // Bottom Composer Area (controls strip + UsageBar + attachments + floating pill)
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px(px(20.0))
                    .pt(px(20.0))
                    .pb(px(16.0))
                    .bg(theme.bg_pane)
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(740.0))
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .relative()
                            // Persistent Goal Bar docked above composer
                            .when_some(active_goal.as_ref(), |d, snap| {
                                d.child(self.render_goal_bar(&theme, snap, cx))
                            })
                            // Searchable Hermes slash command discovery popover
                            .when(!matching_slash.is_empty(), |d| {
                                d.child(popover_enter("slash-completions-enter",
                                    div()
                                        .id("slash-completions-popover")
                                        .mb(px(8.0))
                                        .w(px(480.0))
                                        .max_w_full()
                                        .max_h(px(280.0))
                                        .overflow_y_scroll()
                                        .p(px(5.0))
                                        .rounded(px(14.0))
                                        .bg(theme.bg_elevated)
                                        .border_1()
                                        .border_color(theme.border_default)
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .children(matching_slash.into_iter().enumerate().map(|(idx, cmd)| {
                                            let cmd_name = cmd.name.clone();
                                            let has_args = !cmd.args_hint.trim().is_empty();
                                            let cap_badge = cmd.capability.badge();
                                            let cap_tone = match cmd.capability {
                                                crate::harness::events::CommandCapability::Available => BadgeTone::Muted,
                                                crate::harness::events::CommandCapability::RequiresTerminal => BadgeTone::Warning,
                                                crate::harness::events::CommandCapability::RequiresConfig => BadgeTone::Info,
                                                crate::harness::events::CommandCapability::Unavailable => BadgeTone::Danger,
                                            };
                                            div()
                                                .id(SharedString::from(format!("slash-{idx}-{}", cmd.name)))
                                                .w_full()
                                                .flex_shrink_0()
                                                .px(px(9.0))
                                                .py(px(7.0))
                                                .rounded(px(9.0))
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .gap(px(10.0))
                                                .cursor_pointer()
                                                .hover(|st| st.bg(theme.bg_active))
                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                    if has_args {
                                                        let prefill = format!("{cmd_name} ");
                                                        this.composer.update(cx, |st, cx| {
                                                            st.set_value(prefill, window, cx);
                                                        });
                                                    } else {
                                                        let accepted = this.app.update(cx, |s, cx| {
                                                            s.send_message(cmd_name.clone(), cx)
                                                        });
                                                        if accepted {
                                                            this.composer.update(cx, |st, cx| {
                                                                st.set_value("", window, cx);
                                                            });
                                                        }
                                                    }
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(1.0))
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap(px(6.0))
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(12.5))
                                                                        .font_weight(FontWeight::SEMIBOLD)
                                                                        .text_color(theme.text_primary)
                                                                        .child(cmd.name),
                                                                )
                                                                .when(!cmd.args_hint.is_empty(), |d| {
                                                                    d.child(
                                                                        clipped_label(cmd.args_hint)
                                                                            .font_family(FONT_MONO)
                                                                            .text_size(px(11.0))
                                                                            .text_color(theme.text_tertiary),
                                                                    )
                                                                }),
                                                        )
                                                        .child(
                                                            clipped_label(cmd.description)
                                                                .text_size(px(11.5))
                                                                .text_color(theme.text_secondary),
                                                        ),
                                                )
                                                .when_some(cap_badge, |d, b| {
                                                    d.child(gb_badge(&theme, cap_tone, b))
                                                })
                                        })),
                                ))
                            })
                            // Board assignment popover
                            .when(self.board_menu_open, |d| {
                                let sid_assign = active_session_id.clone();
                                d.child(popover_enter("board-assignment-enter",
                                    div()
                                        .id("board-assignment-popover")
                                        .mb(px(8.0))
                                        .self_end()
                                        .w(px(240.0))
                                        .max_w_full()
                                        .max_h(px(260.0))
                                        .overflow_y_scroll()
                                        .min_w_0()
                                        .p(px(5.0))
                                        .rounded(px(14.0))
                                        .bg(theme.bg_elevated)
                                        .border_1()
                                        .border_color(theme.border_default)
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .child(
                                            div()
                                                .px(px(9.0))
                                                .py(px(5.0))
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme.text_tertiary)
                                                .child("Assign Session Board"),
                                        )
                                        .children(boards.iter().map(|b| {
                                            let b_slug = b.slug.clone();
                                            let sid_c = sid_assign.clone();
                                            let is_cur = current_board
                                                .as_ref()
                                                .is_some_and(|cb| cb.slug == b.slug);
                                            let preset = resolve_bot_color(&b.slug, b.color.as_deref());
                                            div()
                                                .id(SharedString::from(format!("assign-board-{}", b.slug)))
                                                .px(px(9.0))
                                                .py(px(7.0))
                                                .rounded(px(9.0))
                                                .bg(if is_cur {
                                                    theme.bg_active
                                                } else {
                                                    SandTheme::transparent()
                                                })
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .cursor_pointer()
                                                .hover(|st| st.bg(theme.bg_active))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if let Some(ref sid) = sid_c {
                                                        this.app.update(cx, |s, cx| {
                                                            s.assign_session_to_board(
                                                                sid.clone(),
                                                                b_slug.clone(),
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                    this.board_menu_open = false;
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(8.0))
                                                        .text_size(px(13.0))
                                                        .child(gb_dot(preset.hex, 7.0))
                                                        .child(b.name.clone()),
                                                )
                                                .when(is_cur, |d| {
                                                    d.child(cursor_icon("check", 13.0, theme.text_primary))
                                                })
                                        })),
                                ))
                            })
                            // Controls strip (model, effort, fast, ask/yolo, board)
                            .child(
                                div()
                                    .min_w_0()
                                    .px(px(10.0))
                                    .pb(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .flex_wrap()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .max_w_full()
                                            .flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap(px(8.0))
                                            .child(
                                                gb_chip_btn("chip-model", &theme, false)
                                                    .min_w_0()
                                                    .max_w_full()
                                                    .child(cursor_icon("sparkles", 12.0, theme.text_secondary))
                                                    .child(clipped_label(short_model_name(&active_model)))
                                                    .child(cursor_icon(
                                                        "chevron-down-small",
                                                        11.0,
                                                        theme.text_tertiary,
                                                    ))
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            s.model_picker_open = true;
                                                            cx.notify();
                                                        });
                                                    })),
                                            )
                                            .child(
                                                gb_chip_btn(
                                                    "chip-effort",
                                                    &theme,
                                                    effort != crate::state::app_state::ReasoningEffort::None,
                                                )
                                                .child(cursor_icon(
                                                    "brain",
                                                    12.0,
                                                    if effort != crate::state::app_state::ReasoningEffort::None {
                                                        theme.agent_accent
                                                    } else {
                                                        theme.text_secondary
                                                    },
                                                ))
                                                .child(effort.as_str())
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.app.update(cx, |s, cx| {
                                                        s.reasoning_effort = s.reasoning_effort.next();
                                                        cx.notify();
                                                    });
                                                })),
                                            )
                                            .child(
                                                gb_chip_btn("chip-fast", &theme, fast)
                                                    .child(cursor_icon(
                                                        "zap",
                                                        12.0,
                                                        if fast {
                                                            theme.agent_accent
                                                        } else {
                                                            theme.text_secondary
                                                        },
                                                    ))
                                                    .child("Fast")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            s.fast_mode = !s.fast_mode;
                                                            cx.notify();
                                                        });
                                                    })),
                                            )
                                            .child(
                                                gb_chip_btn("chip-yolo", &theme, yolo)
                                                    .child(cursor_icon(
                                                        if yolo { "shield-check" } else { "shield" },
                                                        12.0,
                                                        if yolo {
                                                            theme.agent_accent
                                                        } else {
                                                            theme.text_secondary
                                                        },
                                                    ))
                                                    .child(if yolo { "YOLO" } else { "Ask" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            s.yolo_mode = !s.yolo_mode;
                                                            cx.notify();
                                                        });
                                                    })),
                                            ),
                                    )
                                    .when_some(current_board, |d, b| {
                                        let preset = resolve_bot_color(&b.slug, b.color.as_deref());
                                        d.child(
                                            gb_chip_btn("chip-board", &theme, false)
                                                .min_w_0()
                                                .max_w_full()
                                                .child(gb_dot(preset.hex, 7.0))
                                                .child(clipped_label(b.name))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.board_menu_open = !this.board_menu_open;
                                                    cx.notify();
                                                })),
                                        )
                                    }),
                            )
                            // Context UsageBar
                            .child(usage_bar)
                            // Attachments row
                            .when(!attachments.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("chat-attachments-strip")
                                        .max_h(px(84.0))
                                        .overflow_y_scroll()
                                        .px(px(12.0))
                                        .pb(px(8.0))
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(6.0))
                                        .children(attachments.iter().enumerate().map(|(i, att)| {
                                            div()
                                                .min_w_0()
                                                .max_w_full()
                                                .px(px(9.0))
                                                .py(px(5.0))
                                                .rounded(px(10.0))
                                                .bg(theme.bg_elevated)
                                                .border_1()
                                                .border_color(theme.border_default)
                                                .text_size(px(12.0))
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(cursor_icon("file", 13.0, theme.text_secondary))
                                                .child(clipped_label(att.name.clone()))
                                                .child(
                                                    gb_mini_icon_btn(
                                                        SharedString::from(format!("rm-att-{i}")),
                                                        &theme,
                                                        false,
                                                    )
                                                    .child(cursor_icon("close", 11.0, theme.text_tertiary))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            if i < s.attachments.len() {
                                                                s.attachments.remove(i);
                                                            }
                                                            cx.notify();
                                                        });
                                                    })),
                                                )
                                        })),
                                )
                            })
                            // Attach menu
                            .when(self.attach_menu_open, |d| {
                                d.child(popover_enter("attach-menu-enter",
                                    div()
                                        .mx(px(12.0))
                                        .mb(px(8.0))
                                        .p(px(6.0))
                                        .rounded(px(10.0))
                                        .bg(theme.bg_elevated)
                                        .border_1()
                                        .border_color(theme.border_default)
                                        .flex()
                                        .items_center()
                                        .flex_wrap()
                                        .gap(px(8.0))
                                        .child(
                                            gb_btn("menu-attach-files", &theme, BtnVariant::Ghost, true, false)
                                                .child(cursor_icon("file", 14.0, theme.text_secondary))
                                                .child("Attach files")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.pick_attachment_files(cx);
                                                })),
                                        )
                                        .child(
                                            gb_btn("menu-slash-cmds", &theme, BtnVariant::Ghost, true, false)
                                                .child("Slash commands")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    let cur = this.composer.read(cx).value().to_string();
                                                    let next = if cur.starts_with('/') {
                                                        cur
                                                    } else {
                                                        format!("/{cur}")
                                                    };
                                                    this.composer
                                                        .update(cx, |st, cx| st.set_value(next, window, cx));
                                                    this.attach_menu_open = false;
                                                    cx.notify();
                                                })),
                                        ),
                                ))
                            })
                            // Match the Electron composer and keep the controls at the bottom of multiline drafts.
                            .child(
                                div()
                                    .id("chat-composer")
                                    .w_full()
                                    .min_w_0()
                                    .px(px(8.0))
                                    .py(px(7.0))
                                    .rounded(px(26.0))
                                    .bg(theme.bg_chrome)
                                    .border(composer_border)
                                    .border_color(theme.border_default)
                                    .shadow(vec![
                                        BoxShadow::new(px(0.0), px(4.0), rgba(0x00000024).into())
                                            // GPUI uses sigma, half the CSS blur radius.
                                            .blur_radius(px(10.0))
                                            .spread_radius(px(-6.0)),
                                    ])
                                    .flex()
                                    .items_end()
                                    // GPUI's xsmall editor adds 4px horizontally. A 6px gap
                                    // matches Electron's 8px gap plus 2px of textarea padding.
                                    .gap(px(6.0))
                                    .child(
                                        composer_button("composer-plus", "Attachments and commands")
                                            .bg(theme.bg_secondary)
                                            .cursor_pointer()
                                            .hover(|st| st.bg(theme.bg_active))
                                            .child(cursor_icon("plus", 15.0, theme.text_secondary))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.attach_menu_open = !this.attach_menu_open;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Textarea::new(&self.composer)
                                            .appearance(false)
                                            .xsmall()
                                            .flex_1()
                                            .min_w_0()
                                            .min_h(px(24.0))
                                            .when(self.composer_overflowing, |field| {
                                                field.h(px(COMPOSER_MAX_HEIGHT))
                                            })
                                            .py(px(4.0))
                                            .text_size(px(14.5))
                                            .line_height(px(21.0))
                                            .text_color(theme.text_primary),
                                    )
                                    .child(if is_streaming {
                                        composer_button("composer-stop", "Stop generating")
                                            .bg(theme.danger_primary)
                                            .cursor_pointer()
                                            .hover(|st| st.opacity(0.88))
                                            .child(cursor_icon("stop", 13.0, rgb(0xffffff).into()))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |s, cx| s.interrupt_stream(cx));
                                            }))
                                    } else {
                                        let can_send = !draft.trim().is_empty() || !attachments.is_empty();
                                        composer_button("composer-send", "Send message (Enter)")
                                            .bg(theme.bg_primary)
                                            .child(composer_send_icon(theme.bubble_user_ink))
                                            .when(can_send, |d| {
                                                d.cursor_pointer()
                                                    .hover(|st| st.opacity(0.88))
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.submit_composer(window, cx);
                                                    }))
                                            })
                                    }),
                            )
                            // Work Mode attached bottom action strip (matches reference screenshot: Choose project · Files · Git · Agents · Trajectory · /goal)
                            .when(conv_mode == ConversationMode::Work, |d| {
                                d.child(self.render_work_composer_footer(
                                    &theme,
                                    active_project.as_ref(),
                                    active_work_cwd.as_deref(),
                                    cx,
                                ))
                            }),
                    ),
            )
    }
}

impl ChatView {
    fn render_mode_toggle(
        &self,
        theme: &SandTheme,
        mode: ConversationMode,
        locked: bool,
        project: Option<&crate::harness::events::WorkProject>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if locked {
            let label = if mode == ConversationMode::Work {
                if let Some(p) = project {
                    format!("Work · {}", p.name)
                } else {
                    "Work".to_string()
                }
            } else {
                "Chat".to_string()
            };
            return div()
                .id("chat-mode-locked-pill")
                .flex_shrink_0()
                .h(px(30.0))
                .px(px(11.0))
                .rounded_full()
                .bg(theme.bg_chrome)
                .border_1()
                .border_color(theme.border_default)
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .hover(|st| st.bg(theme.bg_elevated))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.app.update(cx, |s, cx| {
                        s.show_toast(
                            "Mode is locked after the first message. Click + to start a new conversation in another mode.",
                            cx,
                        );
                    });
                }))
                .child(cursor_icon(
                    if mode == ConversationMode::Work { "terminal" } else { "chat-bubble" },
                    12.5,
                    if mode == ConversationMode::Work { theme.agent_accent } else { theme.text_secondary },
                ))
                .child(clipped_label(label).max_w(px(180.0)))
                .child(cursor_icon("lock", 10.5, theme.text_tertiary));
        }

        // Centered Chat | Work segmented pill toggle (matches Grok Bot dark/light styling)
        div()
            .id("chat-work-mode-toggle")
            .flex_shrink_0()
            .h(px(34.0))
            .p(px(3.0))
            .rounded_full()
            .bg(theme.bg_chrome)
            .border_1()
            .border_color(theme.border_default)
            .flex()
            .items_center()
            .gap(px(2.0))
            .children([ConversationMode::Chat, ConversationMode::Work].into_iter().map(|opt| {
                let active = mode == opt;
                div()
                    .id(SharedString::from(format!("mode-seg-{}", opt.as_str())))
                    .h_full()
                    .px(px(18.0))
                    .rounded_full()
                    .bg(if active {
                        theme.bg_elevated
                    } else {
                        SandTheme::transparent()
                    })
                    .text_size(px(12.5))
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if active {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|st| st.text_color(theme.text_primary))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.app.update(cx, |s, cx| s.set_draft_mode(opt, cx));
                    }))
                    .child(opt.label())
            }))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_work_metrics_bar(
        &self,
        theme: &SandTheme,
        project: Option<&crate::harness::events::WorkProject>,
        cwd: Option<&str>,
        git: Option<&crate::backend::workspace::GitWorkspaceStatus>,
        metrics: &SessionLiveMetrics,
        exec_state: ExecutionState,
        subagent_count: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let total_tok = metrics.total_tokens();
        let tps = metrics.output_tps();
        let elapsed = metrics.elapsed_ms().map(format_elapsed_ms);

        let tok_label = match (total_tok.value, total_tok.provenance) {
            (Some(v), MetricProvenance::Measured) => format!("{} tok", format_tokens(v)),
            (Some(v), MetricProvenance::Estimated) => format!("~{} tok (est.)", format_tokens(v)),
            _ => "tok: unavailable".to_string(),
        };
        let tps_label = match (tps.tokens_per_sec, tps.provenance) {
            (Some(r), MetricProvenance::Measured) => format!("{:.1} tok/s ({:.0}s)", r, tps.window_secs),
            (Some(r), MetricProvenance::Estimated) => format!("~{:.1} tok/s (est.)", r),
            _ => "tok/s: unavailable".to_string(),
        };

        div()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .px(px(16.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(theme.border_hairline)
            .bg(theme.bg_pane)
            .flex()
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap(px(8.0))
            .text_size(px(11.5))
            .text_color(theme.text_secondary)
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("work-bar-project")
                            .px(px(8.0))
                            .py(px(2.0))
                            .rounded(px(7.0))
                            .bg(theme.bg_card)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .cursor_pointer()
                            .hover(|st| st.bg(theme.bg_hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| s.open_project_picker(cx));
                            }))
                            .child(cursor_icon("folder", 12.0, theme.agent_accent))
                            .child(clipped_label(
                                project
                                    .map(|p| p.name.clone())
                                    .unwrap_or_else(|| "Select project".into()),
                            ).max_w(px(140.0)).text_color(theme.text_primary))
                            .when_some(cwd, |d, c| {
                                d.child(
                                    clipped_label(c.to_string())
                                        .max_w(px(220.0))
                                        .font_family(FONT_MONO)
                                        .text_size(px(10.5))
                                        .text_color(theme.text_tertiary),
                                )
                            }),
                    )
                    .when_some(git.filter(|g| g.is_repo), |d, g| {
                        d.child(
                            div()
                                .id("work-bar-git")
                                .px(px(7.0))
                                .py(px(2.0))
                                .rounded(px(7.0))
                                .bg(theme.bg_card)
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .cursor_pointer()
                                .hover(|st| st.bg(theme.bg_hover))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.app.update(cx, |s, cx| {
                                        s.right_pane_open = true;
                                        s.right_pane_tab = RightPaneTabId::Git;
                                        s.refresh_inspector_git(cx);
                                        s.bump_shell();
                                        cx.notify();
                                    });
                                }))
                                .child(cursor_icon("git-branch", 11.5, theme.text_tertiary))
                                .child(
                                    clipped_label(if g.branch.is_empty() {
                                        "HEAD".into()
                                    } else {
                                        g.branch.clone()
                                    })
                                    .max_w(px(120.0))
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.5)),
                                )
                                .when(!g.changed_files.is_empty() || g.total_added > 0 || g.total_removed > 0, |d| {
                                    d.child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(10.5))
                                            .text_color(theme.success_text)
                                            .child(format!("+{}", g.total_added)),
                                    )
                                    .child(
                                        div()
                                            .font_family(FONT_MONO)
                                            .text_size(px(10.5))
                                            .text_color(theme.danger_text)
                                            .child(format!("-{}", g.total_removed)),
                                    )
                                }),
                        )
                    })
                    .when(subagent_count > 0, |d| {
                        d.child(
                            div()
                                .id("work-bar-subagents")
                                .px(px(7.0))
                                .py(px(2.0))
                                .rounded(px(7.0))
                                .bg(theme.bg_card)
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .cursor_pointer()
                                .hover(|st| st.bg(theme.bg_hover))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.app.update(cx, |s, cx| {
                                        s.right_pane_open = true;
                                        s.right_pane_tab = RightPaneTabId::Agents;
                                        s.bump_shell();
                                        cx.notify();
                                    });
                                }))
                                .child(cursor_icon("layers", 11.0, theme.agent_accent))
                                .child(format!("{subagent_count} agents")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(10.0))
                    .font_family(FONT_MONO)
                    .text_size(px(10.5))
                    .text_color(theme.text_tertiary)
                    .when(exec_state != ExecutionState::Idle, |d| {
                        d.child(div().text_color(theme.agent_accent).child(exec_state.label()))
                    })
                    .when_some(elapsed, |d, el| d.child(el))
                    .child(tok_label)
                    .child(tps_label),
            )
    }

    fn render_work_composer_footer(
        &self,
        theme: &SandTheme,
        project: Option<&crate::harness::events::WorkProject>,
        cwd: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let proj_lbl = project
            .map(|p| p.name.clone())
            .or_else(|| {
                cwd.and_then(|c| std::path::Path::new(c).file_name().map(|n| n.to_string_lossy().to_string()))
            })
            .unwrap_or_else(|| "Choose project".into());

        div()
            .w_full()
            .mt(px(6.0))
            .px(px(12.0))
            .py(px(7.0))
            .rounded(px(14.0))
            .bg(theme.bg_card)
            .border_1()
            .border_color(theme.border_subtle)
            .flex()
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap(px(8.0))
            .text_size(px(12.0))
            .text_color(theme.text_secondary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(
                        div()
                            .id("work-footer-project")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .hover(|st| st.text_color(theme.text_primary))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| s.open_project_picker(cx));
                            }))
                            .child(cursor_icon("folder", 13.0, theme.text_secondary))
                            .child(clipped_label(proj_lbl).max_w(px(180.0))),
                    )
                    .child(
                        div()
                            .id("work-footer-files")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .hover(|st| st.text_color(theme.text_primary))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.right_pane_open = true;
                                    s.right_pane_tab = RightPaneTabId::Files;
                                    s.refresh_inspector_files(cx);
                                    s.bump_shell();
                                    cx.notify();
                                });
                            }))
                            .child(cursor_icon("file", 13.0, theme.text_secondary))
                            .child("Files"),
                    )
                    .child(
                        div()
                            .id("work-footer-agents")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .hover(|st| st.text_color(theme.text_primary))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.right_pane_open = true;
                                    s.right_pane_tab = RightPaneTabId::Agents;
                                    s.bump_shell();
                                    cx.notify();
                                });
                            }))
                            .child(cursor_icon("layers", 13.0, theme.text_secondary))
                            .child("Agents"),
                    )
                    .child(
                        div()
                            .id("work-footer-trajectory")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .hover(|st| st.text_color(theme.text_primary))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.inspect_tool_in_trajectory(None, cx);
                                });
                            }))
                            .child(cursor_icon("activity", 13.0, theme.text_secondary))
                            .child("Trajectory"),
                    ),
            )
            .child(
                div()
                    .id("work-footer-goal")
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .cursor_pointer()
                    .text_color(theme.text_tertiary)
                    .hover(|st| st.text_color(theme.text_primary))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.composer
                            .update(cx, |st, cx| st.set_value("/goal ", window, cx));
                        cx.notify();
                    }))
                    .child(cursor_icon("target", 12.5, theme.agent_accent))
                    .child("/goal"),
            )
    }

    fn render_goal_bar(
        &self,
        theme: &SandTheme,
        snap: &GoalRuntimeSnapshot,
        cx: &mut Context<Self>,
    ) -> Div {
        let g = &snap.goal;
        let phase_tone = match g.phase {
            GoalPhase::Active => BadgeTone::Accent,
            GoalPhase::Paused => BadgeTone::Warning,
            GoalPhase::Blocked | GoalPhase::Failed => BadgeTone::Danger,
            GoalPhase::Complete => BadgeTone::Success,
            GoalPhase::Cancelled => BadgeTone::Muted,
        };
        let worker_label = if !snap.worker_connected {
            "Worker offline"
        } else if snap.recovered_from_restart && snap.activation == GoalActivation::Disarmed {
            "Recovered · disarmed (Resume to continue)"
        } else if snap.activation == GoalActivation::Armed {
            "Worker armed"
        } else {
            "Worker idle"
        };

        div()
            .w_full()
            .mb(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(14.0))
            .bg(theme.bg_elevated)
            .border_1()
            .border_color(theme.border_default)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .child(cursor_icon("target", 13.0, theme.agent_accent))
                            .child(gb_badge(theme, phase_tone, g.phase.label()))
                            .child(
                                clipped_label(g.objective.clone())
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .when(g.phase == GoalPhase::Active && snap.activation == GoalActivation::Armed, |d| {
                                d.child(
                                    gb_btn("goal-pause-btn", theme, BtnVariant::Secondary, true, false)
                                        .child("Pause")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| s.pause_active_goal(cx));
                                        })),
                                )
                            })
                            .when(
                                matches!(g.phase, GoalPhase::Paused | GoalPhase::Blocked)
                                    || (g.phase == GoalPhase::Active && snap.activation == GoalActivation::Disarmed),
                                |d| {
                                    d.child(
                                        gb_btn("goal-resume-btn", theme, BtnVariant::Primary, true, !snap.worker_connected)
                                            .child("Resume")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |s, cx| s.resume_active_goal(cx));
                                            })),
                                    )
                                },
                            )
                            .when(!g.phase.is_terminal(), |d| {
                                d.child(
                                    gb_btn("goal-complete-btn", theme, BtnVariant::Ghost, true, false)
                                        .child("Complete")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| s.complete_active_goal(cx));
                                        })),
                                )
                                .child(
                                    gb_btn("goal-cancel-btn", theme, BtnVariant::Danger, true, false)
                                        .child("Cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| s.cancel_active_goal(cx));
                                        })),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(10.0))
                    .font_family(FONT_MONO)
                    .text_size(px(10.5))
                    .text_color(theme.text_tertiary)
                    .child(format!("Round {}/{}", g.rounds_started, g.max_rounds))
                    .child(worker_label)
                    .when_some(g.max_tokens, |d, mt| {
                        d.child(format!("Tokens {}/{}", format_tokens(g.tokens_used), format_tokens(mt)))
                    })
                    .when_some(g.stop_reason.as_ref().or(g.blocked_message.as_ref()), |d, reason| {
                        d.child(clipped_label(reason.clone()).text_color(theme.text_secondary))
                    }),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_transcript(
        &mut self,
        theme: &SandTheme,
        chat: crate::state::app_state::SessionChatState,
        bot_seed: &str,
        shape_id: Option<&str>,
        color_id: Option<&str>,
        bot_state: BotFaceState,
        exec_state: ExecutionState,
        reduced_motion: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let has_inline_activity = chat.messages.iter().rev().any(|m| m.streaming && message_has_body(m));

        let has_older = chat.older_offset.is_some();
        let has_todos = !chat.todos.is_empty();
        let has_pending = chat.is_streaming && !chat.has_streamed_text && !has_inline_activity;
        let msg_start = usize::from(has_older) + usize::from(has_todos);
        let msg_count = chat.messages.len();
        let total_items = msg_start + msg_count + usize::from(has_pending);

        let active_sid = self.app.read(cx).active_session_id.clone();
        let messages: Vec<_> = chat.messages.iter().map(|m| (m.id.clone(), message_fingerprint(m))).collect();
        let first_msg_id = chat.messages.first().map(|m| m.id.clone());
        let last_fp = messages.last().map(|(_, fp)| *fp);
        let aux_fp = (row_fingerprint(&chat.todos), row_fingerprint(&chat.live_status_text));
        let switched = self.synced_session_id != active_sid;
        let prefix_changed = self.synced_prefix_count != msg_start || self.synced_prefix_kind != (has_older, has_todos);
        if switched || prefix_changed || self.synced_first_msg_id != first_msg_id || msg_count < self.synced_msg_count {
            // A prefix change shifts the cached heights, even if the total row count is unchanged.
            let was_following = switched || self.list_state.is_following_tail();
            let anchor = self.list_state.logical_scroll_top();
            let anchor_id = anchor.item_ix.checked_sub(self.synced_prefix_count)
                .and_then(|index| self.synced_messages.get(index))
                .map(|(id, _)| id);
            self.smooth_scroller.cancel();
            self.list_state.reset(total_items);
            self.measured_width.set(None);
            if was_following {
                self.list_state.set_follow_mode(FollowMode::Tail);
                self.list_state.scroll_to_end();
            } else if let Some(id) = anchor_id {
                if let Some(index) = messages.iter().position(|(candidate, _)| candidate == id) {
                    self.list_state.scroll_to(ListOffset { item_ix: msg_start + index, ..anchor });
                }
            }
        } else {
            if msg_count != self.synced_msg_count || has_pending != self.synced_has_pending {
                let tail_start = msg_start + self.synced_msg_count;
                self.list_state.splice(
                    tail_start..tail_start + usize::from(self.synced_has_pending),
                    msg_count - self.synced_msg_count + usize::from(has_pending),
                );
            }
            for (index, (old, current)) in self.synced_messages.iter().zip(&messages).enumerate() {
                // Tool/approval events can update a row behind the streaming tail.
                if old != current && index + 1 != msg_count {
                    self.list_state.remeasure_items(msg_start + index..msg_start + index + 1);
                }
            }
            if last_fp != self.synced_last_fp && msg_count > 0 {
                self.list_state.remeasure_items(msg_start + msg_count - 1..msg_start + msg_count);
            }
            if has_todos && aux_fp.0 != self.synced_aux_fp.0 {
                let index = usize::from(has_older);
                self.list_state.remeasure_items(index..index + 1);
            }
            if has_pending && aux_fp.1 != self.synced_aux_fp.1 {
                let index = msg_start + msg_count;
                self.list_state.remeasure_items(index..index + 1);
            }
        }
        self.synced_session_id = active_sid;
        self.synced_prefix_count = msg_start;
        self.synced_prefix_kind = (has_older, has_todos);
        self.synced_first_msg_id = first_msg_id;
        self.synced_msg_count = msg_count;
        self.synced_has_pending = has_pending;
        self.synced_last_fp = last_fp;
        self.synced_messages = messages;
        self.synced_aux_fp = aux_fp;
        self.activity_disclosure.prune_completed(&chat.messages);

        let is_scrolled_up = self.list_state.max_offset_for_scrollbar().y > px(40.0)
            && !self.list_state.is_following_tail()
            && !self.list_state.is_scrolled_to_end().unwrap_or(true);
        let jump_id = self.jump_pill_entrance.id("chat-jump-enter", is_scrolled_up);

        let theme_copy = *theme;
        let bot_seed_owned = bot_seed.to_string();
        let shape_owned = shape_id.map(str::to_string);
        let color_owned = color_id.map(str::to_string);
        let messages = chat.messages;
        let todos = chat.todos;
        let loading_history = chat.loading_history;
        let live_status_text = chat.live_status_text;
        let is_streaming = chat.is_streaming;
        let _measured_width = &self.measured_width;
        let thought_scroll = ThoughtScrollHitboxes::default();
        let row_scroll = thought_scroll.clone();
        let clear_scroll = thought_scroll.clone();

        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .flex()
            .flex_col()
            .relative()
            // Clear nested thought scroll hitboxes before list prepaint; GPUI List already
            // invalidates cached row heights automatically whenever its layout width changes.
            .child(canvas(move |_, _, _| {
                clear_scroll.borrow_mut().clear();
            }, |_, _, _, _| {}).absolute().w_full().h(px(0.0)))
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(move |this, idx: usize, _window, cx| {
                        let inner: AnyElement = if has_older && idx == 0 {
                            chat_row()
                                .pb(px(14.0))
                                .flex()
                                .justify_center()
                                .child(
                                    gb_btn(
                                        "load-older-msgs",
                                        &theme_copy,
                                        BtnVariant::Secondary,
                                        true,
                                        loading_history,
                                    )
                                    .when(loading_history, |d| d.child(gpu_spinner(12.0, theme_copy.text_primary)))
                                    .child(if loading_history {
                                        "Loading…"
                                    } else {
                                        "Load older messages"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| s.load_older_messages(cx));
                                    })),
                                )
                                .into_any_element()
                        } else if has_todos && idx == usize::from(has_older) {
                            let done_count =
                                todos.iter().filter(|t| t.status == "completed").count();
                            chat_row()
                                .pb(px(14.0))
                                .child(
                                    gb_section_card(&theme_copy)
                                        .child(
                                            gb_section_card_head(&theme_copy)
                                                .py(px(9.0))
                                                .px(px(13.0))
                                                .child(
                                                    chat_row()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .text_size(px(12.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child(cursor_icon(
                                                            "list-todo",
                                                            13.0,
                                                            theme_copy.text_primary,
                                                        ))
                                                        .child(format!(
                                                            "Agent Plan ({done_count}/{})",
                                                            todos.len()
                                                        )),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .px(px(13.0))
                                                .py(px(8.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(5.0))
                                                .children(todos.iter().map(|t| {
                                                    let indicator = match t.status.as_str() {
                                                        "completed" => cursor_icon("check-circle", 13.0, theme_copy.success_text).into_any_element(),
                                                        "in_progress" if is_streaming => gpu_spinner(13.0, theme_copy.accent_text).into_any_element(),
                                                        "in_progress" => cursor_icon("circle", 13.0, theme_copy.accent_text).into_any_element(),
                                                        _ => cursor_icon("circle", 13.0, theme_copy.text_tertiary).into_any_element(),
                                                    };
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(7.0))
                                                        .text_size(px(12.5))
                                                        .text_color(if t.status == "completed" {
                                                            theme_copy.text_tertiary
                                                        } else {
                                                            theme_copy.text_primary
                                                        })
                                                        .child(indicator)
                                                        .child(div().flex_1().min_w_0().child(StyledText::new(t.content.clone())))
                                                })),
                                        ),
                                )
                                .into_any_element()
                        } else if idx >= msg_start && idx < msg_start + msg_count {
                            let msg = &messages[idx - msg_start];
                            chat_row()
                                .when(message_has_body(msg), |d| d.pb(px(14.0)))
                                .child(this.render_message(
                                    msg,
                                    &theme_copy,
                                    &bot_seed_owned,
                                    shape_owned.as_deref(),
                                    color_owned.as_deref(),
                                    bot_state,
                                    exec_state,
                                    reduced_motion,
                                    is_streaming,
                                    &row_scroll,
                                    cx,
                                ))
                                .into_any_element()
                        } else if has_pending && idx == msg_start + msg_count {
                            let status_lbl = if live_status_text.is_empty() {
                                format!("{}…", exec_state.label())
                            } else {
                                live_status_text.clone()
                            };
                            // Bare inline bot face directly on page background (no card/border/capsule)
                            chat_row()
                                .pb(px(14.0))
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .py(px(4.0))
                                .child(bot_face_with_options(
                                    &theme_copy,
                                    &bot_seed_owned,
                                    shape_owned.as_deref(),
                                    color_owned.as_deref(),
                                    28.0,
                                    bot_state,
                                    BotFaceOptions {
                                        slot_key: "chat-pending",
                                        reduced_motion,
                                        pop_on_activate: !reduced_motion,
                                        ..Default::default()
                                    },
                                ))
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_size(px(13.0))
                                        .text_color(theme_copy.text_tertiary)
                                        .child(StyledText::new(status_lbl)),
                                )
                                .when(!reduced_motion, |d| d.child(typing_dots(theme_copy.text_tertiary)))
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        };

                        div()
                            .w_full()
                            .min_w_0()
                            .px(px(24.0))
                            .flex()
                            .justify_center()
                            .child(chat_row().max_w(px(740.0)).flex().flex_col().child(inner))
                            .into_any_element()
                    }),
                )
                .flex_1()
                .min_h_0()
                .w_full()
                .py(px(14.0)),
            )
            .vertical_scrollbar(&self.list_state)
            .child(smooth_scroll_layer(&self.smooth_scroller).filter_events(move |event, window| {
                // Let GPUI's nested scroll panes handle their own wheel events first.
                !event.modifiers.shift && !thought_scroll.borrow().iter().any(|hitbox| hitbox.should_handle_scroll(window))
            }))
            // Floating Jump-to-Latest pill when scrolled up away from bottom
            .when(is_scrolled_up, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(14.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(pill_enter(jump_id,
                            gb_btn("chat-jump-bottom", theme, BtnVariant::Secondary, true, false)
                                .shadow_md()
                                .child(cursor_icon("arrow-down", 12.0, theme.text_primary))
                                .child("Latest")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.smooth_scroller.cancel();
                                    this.list_state.set_follow_mode(FollowMode::Tail);
                                    this.list_state.scroll_to_end();
                                    cx.notify();
                                })),
                        )),
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn render_message(
        &self,
        msg: &HermesMessage,
        theme: &SandTheme,
        bot_seed: &str,
        shape_id: Option<&str>,
        color_id: Option<&str>,
        live_bot_state: BotFaceState,
        exec_state: ExecutionState,
        reduced_motion: bool,
        session_streaming: bool,
        thought_scroll: &ThoughtScrollHitboxes,
        cx: &mut Context<Self>,
    ) -> Div {
        if msg.role == MessageRole::Tool {
            let synth = HermesToolCall {
                id: msg.id.clone(),
                name: msg.tool_name.clone().unwrap_or_else(|| "tool".into()),
                arguments: None,
                output: Some(msg.content.clone()),
                status: ToolCallStatus::Done,
                duration_ms: None,
                preview: None,
                inline_diff: None,
                waiting_approval: false,
            };
            return chat_row()
                .flex()
                .flex_col()
                .items_start()
                .child(self.render_tool_card(&synth, theme, session_streaming && msg.streaming, cx));
        }

        if msg.role == MessageRole::User {
            return chat_row()
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .max_w(px(560.0))
                        .min_w_0()
                        .overflow_hidden()
                        .px(px(14.0))
                        .py(px(9.0))
                        .rounded(px(19.0))
                        .bg(theme.bubble_user)
                        .text_color(theme.bubble_user_ink)
                        .text_size(px(14.5))
                        .line_height(px(21.0))
                        .child(StyledText::new(msg.content.clone())),
                );
        }

        let msg_id = msg.id.clone();
        let copy_content = msg.content.clone();
        let thinking = msg.thinking.as_deref().filter(|t| !t.is_empty());
        let has_answer = !msg.content.trim().is_empty();
        let has_errors = msg.tool_calls.iter().any(|t| t.status == ToolCallStatus::Error);
        let has_activity = msg.streaming || thinking.is_some() || !msg.tool_calls.is_empty();
        let activity_open = self.activity_disclosure.is_expanded(msg);
        let active_tool = msg
            .tool_calls
            .iter()
            .rev()
            .find(|t| t.status == ToolCallStatus::Running);

        let activity_label = if msg.streaming {
            if let Some(t) = active_tool {
                if t.waiting_approval {
                    format!("Waiting for approval · {}", t.name)
                } else {
                    let (_, lbl, detail, _) =
                        describe_tool(&t.name, t.arguments.as_ref(), t.preview.as_deref(), true);
                    format!("{lbl} {detail}")
                }
            } else {
                format!("{}…", exec_state.label())
            }
        } else {
            let mut parts: Vec<String> = Vec::new();
            if thinking.is_some() {
                parts.push("Reasoning".into());
            }
            if !msg.tool_calls.is_empty() {
                let n = msg.tool_calls.len();
                parts.push(format!("{n} tool{}", if n == 1 { "" } else { "s" }));
            }
            if has_errors {
                parts.push("diagnostics available".into());
            }
            if parts.is_empty() {
                "Activity".into()
            } else {
                parts.join(" · ")
            }
        };

        let inline_face_state = if msg.streaming {
            if let Some(t) = active_tool {
                if t.waiting_approval {
                    BotFaceState::Blocked
                } else {
                    describe_tool(&t.name, t.arguments.as_ref(), t.preview.as_deref(), true).3
                }
            } else {
                live_bot_state
            }
        } else if has_errors {
            BotFaceState::Failed
        } else {
            BotFaceState::Finished
        };

        chat_row()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(6.0))
            // Bare inline bot face + compact collapsible activity disclosure (collapsed by default, auto-collapses on completion)
            .when(has_activity, |d| {
                let aid = msg_id.clone();
                let is_live = msg.streaming;
                d.child(
                    chat_row()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(4.0))
                        .child(
                            div()
                                .id(SharedString::from(format!("activity-head-{msg_id}")))
                                .max_w_full()
                                .min_w_0()
                                .overflow_hidden()
                                .min_h(px(26.0))
                                .py(px(2.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if has_errors && !msg.streaming {
                                    theme.danger_text
                                } else {
                                    theme.text_tertiary
                                })
                                .cursor_pointer()
                                .hover(|st| st.text_color(theme.text_secondary))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.activity_disclosure.toggle(&aid, is_live);
                                    this.list_state.remeasure();
                                    cx.notify();
                                }))
                                // Bare inline face popped directly onto page background when working (no card/border/capsule)
                                .when(msg.streaming, |row| {
                                    row.child(bot_face_with_options(
                                        theme,
                                        bot_seed,
                                        shape_id,
                                        color_id,
                                        26.0,
                                        inline_face_state,
                                        BotFaceOptions {
                                            slot_key: "chat-inline-work",
                                            reduced_motion,
                                            pop_on_activate: !reduced_motion,
                                            ..Default::default()
                                        },
                                    ))
                                })
                                .when(!msg.streaming, |row| {
                                    row.child(cursor_icon(
                                        if has_errors { "alert-circle" } else { "activity" },
                                        13.5,
                                        if has_errors { theme.danger_text } else { theme.text_tertiary },
                                    ))
                                })
                                .child(clipped_label(activity_label))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(3.0))
                                        .text_size(px(11.0))
                                        .text_color(theme.text_tertiary)
                                        .child(if activity_open { "Hide" } else { "Details" })
                                        .child(cursor_icon(
                                            if activity_open { "chevron-down" } else { "chevron-right" },
                                            11.5,
                                            theme.text_tertiary,
                                        )),
                                ),
                        )
                        .when(activity_open, |d| {
                            let thought_scroll = thought_scroll.clone();
                            d.child(popover_enter(
                                SharedString::from(format!("activity-body-{msg_id}")),
                                chat_row()
                                    .pl(px(12.0))
                                    .py(px(4.0))
                                    .border_l_1()
                                    .border_color(theme.border_subtle)
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .when_some(thinking, |d, thought| {
                                        let thought_scroll = thought_scroll.clone();
                                        d.child(
                                            chat_row()
                                                .relative()
                                                .child(
                                                    canvas(
                                                        move |bounds, window, _| {
                                                            let hitbox = window.insert_hitbox(
                                                                bounds,
                                                                HitboxBehavior::Normal,
                                                            );
                                                            thought_scroll.borrow_mut().push(hitbox);
                                                        },
                                                        |_, _, _, _| {},
                                                    )
                                                    .absolute()
                                                    .size_full(),
                                                )
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "think-scroll-{msg_id}"
                                                        )))
                                                        .w_full()
                                                        .min_w_0()
                                                        .max_h(px(240.0))
                                                        .overflow_y_scroll()
                                                        .py(px(3.0))
                                                        .child(render_thinking_markdown(
                                                            &format!("thought-{msg_id}"),
                                                            thought,
                                                            theme,
                                                        )),
                                                ),
                                        )
                                    })
                                    .children(msg.tool_calls.iter().map(|tc| {
                                        self.render_tool_card(
                                            tc,
                                            theme,
                                            session_streaming && msg.streaming,
                                            cx,
                                        )
                                    })),
                            ))
                        }),
                )
            })
            // Approval Card
            .when_some(msg.approval.as_ref(), |d, ap| {
                d.child(self.render_approval_card(ap, theme, cx))
            })
            // Clarify Card
            .when_some(msg.clarify.as_ref(), |d, cl| {
                d.child(self.render_clarify_card(cl, theme, cx))
            })
            // Minimal card-free assistant response directly on the page background (no bubble/card/border)
            .when(has_answer, |d| {
                d.child(
                    chat_row()
                        .py(px(2.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(render_markdown(&msg.id, &msg.content, theme)),
                )
            })
            // Footer meta (model + verbatim Copy available BOTH during streaming AND after completion)
            .when(has_answer, |d| {
                d.child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(11.0))
                        .text_color(theme.text_tertiary)
                        .when_some(msg.model.as_ref(), |d, m| {
                            d.child(clipped_label(short_model_name(m)))
                        })
                        .child(
                            gb_mini_icon_btn(
                                SharedString::from(format!("copy-msg-{}", msg.id)),
                                theme,
                                false,
                            )
                            .child(cursor_icon("copy", 11.0, theme.text_tertiary))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy_content.clone(),
                                ));
                            }),
                        ),
                )
            })
    }

    fn render_tool_card(
        &self,
        tool: &HermesToolCall,
        theme: &SandTheme,
        streaming: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.expanded_tools.contains(&tool.id);
        let tid = tool.id.clone();
        let tid_inspect = tool.id.clone();
        let running = tool.status == ToolCallStatus::Running;
        let ptc = inspect_ptc_tool_call(tool, None, None, Vec::new());
        let (icon, label, default_detail, _) =
            describe_tool(&tool.name, tool.arguments.as_ref(), tool.preview.as_deref(), running);
        let detail = ptc
            .as_ref()
            .map(|p| p.summary.clone())
            .unwrap_or(default_detail);
        let status_badge = if tool.waiting_approval {
            Some((BadgeTone::Warning, "Waiting for approval"))
        } else if running {
            None
        } else if tool.status == ToolCallStatus::Error {
            Some((BadgeTone::Danger, "Failed"))
        } else {
            Some((BadgeTone::Muted, "Completed"))
        };

        chat_row()
            .flex()
            .flex_col()
            .child(
                chat_row()
                    .id(SharedString::from(format!("tool-head-{}", tool.id)))
                    .min_h(px(24.0))
                    .py(px(2.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .text_size(px(12.5))
                    .text_color(theme.text_tertiary)
                    .cursor_pointer()
                    .hover(|st| st.text_color(theme.text_secondary))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded_tools.remove(&tid) {
                            this.expanded_tools.insert(tid.clone());
                        }
                        this.list_state.remeasure();
                        cx.notify();
                    }))
                    .child(cursor_icon(icon, 14.0, theme.text_tertiary))
                    .child(clipped_label(label).flex_shrink_0())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(detail),
                    )
                    .when(running && !tool.waiting_approval, |d| {
                        d.child(live_pulse_dot(theme.accent_text, 5.0, streaming))
                            .child(clipped_label("Running").flex_shrink_0().text_size(px(10.5)).text_color(theme.text_tertiary))
                    })
                    .when_some(status_badge, |d, (tone, txt)| {
                        d.child(gb_badge(theme, tone, txt))
                    })
                    .when_some(tool.duration_ms, |d, ms| {
                        d.child(
                            clipped_label(format_elapsed_ms(ms))
                                .flex_shrink_0()
                                .text_size(px(10.5))
                                .text_color(theme.text_tertiary),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("tool-inspect-{}", tool.id)))
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(5.0))
                            .text_size(px(10.5))
                            .text_color(theme.text_tertiary)
                            .hover(|st| st.bg(theme.bg_hover).text_color(theme.text_primary))
                            .child("Inspect")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.app.update(cx, |s, cx| {
                                    s.inspect_tool_in_trajectory(Some(tid_inspect.clone()), cx);
                                });
                            })),
                    )
                    .child(cursor_icon(
                        if open { "chevron-down" } else { "chevron-right" },
                        11.5,
                        theme.text_tertiary,
                    )),
            )
            .when(open, |d| {
                let ptc_json_on = self.ptc_json_mode.contains(&tool.id);
                let tid_json = tool.id.clone();
                d.child(popover_enter(
                    SharedString::from(format!("tool-body-{}", tool.id)),
                    chat_row()
                        .my(px(3.0))
                        .pl(px(10.0))
                        .py(px(3.0))
                        .border_l_1()
                        .border_color(theme.border_default)
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .when_some(ptc.as_ref(), |d, p| {
                            let lang_opt = if ptc_json_on {
                                Some("json")
                            } else {
                                p.language.map(|l| l.as_str())
                            };
                            let copy_val = if ptc_json_on {
                                p.raw_args_json.clone()
                            } else {
                                p.source.clone()
                            };
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .font_family(FONT_MONO)
                                            .text_size(px(10.5))
                                            .text_color(theme.text_tertiary)
                                            .child(if ptc_json_on {
                                                "ORIGINAL JSON".to_string()
                                            } else {
                                                format!(
                                                    "PTC PROGRAM · {}",
                                                    p.language.map(|l| l.display_label()).unwrap_or("Plain text")
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(
                                                gb_btn(
                                                    SharedString::from(format!("ptc-json-{}", tool.id)),
                                                    theme,
                                                    if ptc_json_on { BtnVariant::Primary } else { BtnVariant::Ghost },
                                                    true,
                                                    false,
                                                )
                                                .child("{}")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if !this.ptc_json_mode.remove(&tid_json) {
                                                        this.ptc_json_mode.insert(tid_json.clone());
                                                    }
                                                    cx.notify();
                                                })),
                                            )
                                            .child(
                                                gb_btn(
                                                    SharedString::from(format!("ptc-copy-{}", tool.id)),
                                                    theme,
                                                    BtnVariant::Ghost,
                                                    true,
                                                    false,
                                                )
                                                .child("Copy")
                                                .on_click(move |_, _, cx| {
                                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                                        copy_val.clone(),
                                                    ));
                                                }),
                                            ),
                                    ),
                            )
                            .child(render_highlighted_code_block(
                                SharedString::from(format!("ptc-src-{}", tool.id)),
                                lang_opt,
                                if ptc_json_on { &p.raw_args_json } else { &p.source },
                                true,
                                false,
                                theme,
                            ))
                        })
                        .when(ptc.is_none(), |d| {
                            d.when_some(tool.arguments.as_ref(), |d, args| {
                                let pretty = serde_json::to_string_pretty(args)
                                    .unwrap_or_else(|_| args.to_string());
                                d.child(
                                    chat_row()
                                        .font_family(FONT_MONO)
                                        .text_size(px(10.5))
                                        .text_color(theme.text_tertiary)
                                        .child("INPUT"),
                                )
                                .child(render_code_block(
                                    SharedString::from(format!("tool-in-{}", tool.id)),
                                    "json",
                                    &pretty,
                                    theme,
                                ))
                            })
                        })
                        .when_some(tool.inline_diff.as_ref().filter(|s| !s.is_empty()), |d, diff| {
                            d.child(
                                chat_row()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.5))
                                    .text_color(theme.text_tertiary)
                                    .child("DIFF"),
                            )
                            .child(render_code_block(
                                SharedString::from(format!("tool-diff-{}", tool.id)),
                                "diff",
                                diff,
                                theme,
                            ))
                        })
                        .when_some(tool.output.as_ref(), |d, out| {
                            let out_lang = if tool.status == ToolCallStatus::Error {
                                "text"
                            } else if ptc.as_ref().is_some_and(|p| p.output_is_json_container) {
                                "json"
                            } else {
                                "text"
                            };
                            d.child(
                                chat_row()
                                    .font_family(FONT_MONO)
                                    .text_size(px(10.5))
                                    .text_color(if tool.status == ToolCallStatus::Error {
                                        theme.danger_text
                                    } else {
                                        theme.text_tertiary
                                    })
                                    .child(if tool.status == ToolCallStatus::Error {
                                        "ERROR"
                                    } else {
                                        "OUTPUT"
                                    }),
                            )
                            .child(render_code_block(
                                SharedString::from(format!("tool-out-{}", tool.id)),
                                out_lang,
                                out,
                                theme,
                            ))
                        })
                        .when(tool.arguments.is_none() && tool.output.is_none(), |d| {
                            d.child(
                                chat_row()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_tertiary)
                                    .child("No payload available yet."),
                            )
                        }),
                ))
            })
    }

    fn render_approval_card(
        &self,
        ap: &HermesApprovalRequest,
        theme: &SandTheme,
        cx: &mut Context<Self>,
    ) -> Div {
        let id1 = ap.id.clone();
        let id2 = ap.id.clone();
        let id3 = ap.id.clone();
        chat_row()
            .max_w(px(520.0))
            .p(px(15.0))
            .rounded(px(18.0))
            .bg(theme.bg_elevated)
            .border_1()
            .border_color(theme.border_default)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                chat_row()
                    .text_size(px(12.0))
                    .text_color(theme.text_tertiary)
                    .child("Needs your OK"),
            )
            .child(
                chat_row()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(StyledText::new(ap.title.clone())),
            )
            .when_some(ap.detail.as_ref().filter(|s| !s.is_empty()).or(ap.command.as_ref()), |d, detail| {
                d.child(
                    chat_row()
                        .p(px(10.0))
                        .rounded(px(10.0))
                        .bg(theme.bg_card)
                        .font_family(FONT_MONO)
                        .text_size(px(12.0))
                        .text_color(theme.text_secondary)
                        .child(StyledText::new(detail.clone())),
                )
            })
            .child(if let Some(dec) = &ap.settled {
                let (tone, txt) = match dec {
                    ApprovalDecision::Denied => (BadgeTone::Danger, "Denied"),
                    ApprovalDecision::Always => (BadgeTone::Success, "Always allowed"),
                    ApprovalDecision::Allowed => (BadgeTone::Success, "Allowed once"),
                };
                chat_row().mt(px(4.0)).child(gb_badge(theme, tone, txt))
            } else {
                chat_row()
                    .mt(px(6.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        gb_btn(
                            SharedString::from(format!("appr-once-{}", ap.id)),
                            theme,
                            BtnVariant::Primary,
                            true,
                            false,
                        )
                        .child("Allow once")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.app
                                .update(cx, |s, cx| s.respond_approval(id1.clone(), "once", cx));
                        })),
                    )
                    .child(
                        gb_btn(
                            SharedString::from(format!("appr-always-{}", ap.id)),
                            theme,
                            BtnVariant::Secondary,
                            true,
                            false,
                        )
                        .child("Always allow")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.app
                                .update(cx, |s, cx| s.respond_approval(id2.clone(), "always", cx));
                        })),
                    )
                    .child(
                        gb_btn(
                            SharedString::from(format!("appr-deny-{}", ap.id)),
                            theme,
                            BtnVariant::Danger,
                            true,
                            false,
                        )
                        .child("Deny")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.app
                                .update(cx, |s, cx| s.respond_approval(id3.clone(), "deny", cx));
                        })),
                    )
            })
    }

    fn render_clarify_card(
        &self,
        cl: &HermesClarifyRequest,
        theme: &SandTheme,
        cx: &mut Context<Self>,
    ) -> Div {
        chat_row()
            .max_w(px(560.0))
            .rounded(px(22.0))
            .bg(theme.bg_elevated)
            .border_1()
            .border_color(theme.border_default)
            .flex()
            .flex_col()
            .child(
                chat_row()
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(8.0))
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(StyledText::new(cl.question.clone())),
            )
            .child(if let Some(ans) = &cl.answered {
                chat_row()
                    .px(px(16.0))
                    .pb(px(14.0))
                    .child(gb_badge(theme, BadgeTone::Success, "Answered"))
                    .child(chat_row().mt(px(4.0)).text_size(px(13.0)).child(StyledText::new(ans.clone())))
            } else {
                let choices = cl.choices.clone().unwrap_or_default();
                let cid = cl.id.clone();
                chat_row()
                    .px(px(12.0))
                    .pb(px(14.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(choices.into_iter().enumerate().map(|(i, choice)| {
                        let cid2 = cid.clone();
                        let ans = choice.clone();
                        chat_row()
                            .id(SharedString::from(format!("clarify-{cid}-{i}")))
                            .px(px(10.0))
                            .py(px(8.0))
                            .rounded(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .cursor_pointer()
                            .hover(|st| st.bg(theme.bg_secondary))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.respond_clarify(cid2.clone(), ans.clone(), cx);
                                });
                            }))
                            .child(
                                div()
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .flex_shrink_0()
                                    .rounded(px(7.0))
                                    .bg(theme.bg_secondary)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child((i + 1).to_string()),
                            )
                            .child(div().flex_1().min_w_0().overflow_hidden().text_size(px(13.5)).child(StyledText::new(choice)))
                    }))
            })
    }
}

fn composer_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .size(px(30.0))
        .rounded_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
}

fn composer_send_icon(ink: Hsla) -> impl IntoElement {
    const SIZE: f32 = 15.0;
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            // Center the Cursor Icons arrow-up outline on its ink bounds. Painting
            // the outline as a path avoids LCD color fringing at fractional scales.
            let center = bounds.center();
            let scale = SIZE / 300.0;
            let glyph_point = |x: f32, y: f32| {
                center + point(px((x - 105.0) * scale), px((150.0 - y) * scale))
            };
            let mut arrow = PathBuilder::fill();
            arrow.move_to(glyph_point(99.0, 272.0));
            arrow.curve_to(glyph_point(106.0, 274.5), glyph_point(102.0, 275.0));
            arrow.curve_to(glyph_point(113.0, 271.0), glyph_point(110.0, 274.0));
            arrow.line_to(glyph_point(207.0, 177.0));
            arrow.curve_to(glyph_point(210.0, 169.0), glyph_point(210.0, 174.0));
            arrow.curve_to(glyph_point(207.0, 160.5), glyph_point(210.0, 164.0));
            arrow.curve_to(glyph_point(199.0, 157.0), glyph_point(204.0, 157.0));
            arrow.curve_to(glyph_point(190.0, 160.0), glyph_point(194.0, 157.0));
            arrow.line_to(glyph_point(117.0, 234.0));
            arrow.line_to(glyph_point(117.0, 37.0));
            arrow.curve_to(glyph_point(113.5, 29.5), glyph_point(117.0, 33.0));
            arrow.curve_to(glyph_point(105.0, 26.0), glyph_point(110.0, 26.0));
            arrow.curve_to(glyph_point(96.5, 29.5), glyph_point(100.0, 26.0));
            arrow.curve_to(glyph_point(93.0, 37.0), glyph_point(93.0, 33.0));
            arrow.line_to(glyph_point(93.0, 234.0));
            arrow.line_to(glyph_point(20.0, 160.0));
            arrow.curve_to(glyph_point(11.0, 157.0), glyph_point(16.0, 157.0));
            arrow.curve_to(glyph_point(3.0, 160.5), glyph_point(6.0, 157.0));
            arrow.curve_to(glyph_point(0.0, 169.0), glyph_point(0.0, 164.0));
            arrow.curve_to(glyph_point(3.0, 177.0), glyph_point(0.0, 174.0));
            arrow.close();
            if let Ok(path) = arrow.build() {
                window.paint_path(path, ink);
            }
        },
    )
    .size(px(SIZE))
}

fn render_usage_bar(
    theme: &SandTheme,
    usage: Option<&HermesSessionUsage>,
    context_limit: Option<u64>,
    fill: &mut SpringValue,
    window: &mut Window,
) -> Div {
    let used = usage.and_then(|u| u.context_used);
    let limit = usage.and_then(|u| u.context_limit).or(context_limit);
    let known = matches!((used, limit), (Some(_), Some(l)) if l > 0);
    let pct = if let (Some(u), Some(l)) = (used, limit) {
        if l > 0 {
            Some(((u as f32 / l as f32) * 100.0).min(100.0))
        } else {
            None
        }
    } else {
        None
    };

    let label = if let (Some(u), Some(l)) = (used, limit) {
        if l > 0 {
            format!("Context {} / {}", format_tokens(u), format_tokens(l))
        } else {
            "Usage unavailable".into()
        }
    } else if let Some(tot) = usage.and_then(|u| u.total_tokens) {
        format!("{} tokens · context unavailable", format_tokens(tot))
    } else {
        "Usage unavailable".into()
    };

    let warning = pct.is_some_and(|p| p >= 85.0);
    let fill_color = if warning {
        rgb(0xe9bd86).into()
    } else {
        theme.accent_text
    };
    if let Some(pct) = pct {
        fill.set_target(pct * 0.64);
    } else {
        fill.snap(0.0);
    }
    let fill_w = fill.advance(window);

    div()
        .w_full()
        .min_w_0()
        .px(px(12.0))
        .pb(px(7.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(10.0))
        .line_height(px(20.0))
        .text_color(theme.text_tertiary)
        .child(
            div()
                .w(px(64.0))
                .h(px(3.0))
                .rounded(px(4.0))
                .bg(theme.border_default)
                .overflow_hidden()
                .flex_shrink_0()
                .child(div().h_full().w(px(fill_w)).bg(fill_color)),
        )
        .child(clipped_label(label))
        .when(known, |d| {
            d.child(
                div()
                    .ml_auto()
                    .child(format!("{}%", pct.unwrap_or(0.0).round() as u32)),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{message_fingerprint, ActivityDisclosure, HermesMessage, HermesToolCall, ToolCallStatus};

    #[test]
    fn activity_disclosure_is_collapsed_by_default_auto_collapses_on_completion_and_reopens() {
        let mut msg = HermesMessage::new_assistant("test".into());
        let mut disclosure = ActivityDisclosure::default();
        // Collapsed by default while streaming
        assert!(!disclosure.is_expanded(&msg));
        // User expands during streaming
        disclosure.toggle(&msg.id, true);
        assert!(disclosure.is_expanded(&msg));
        // Turn completes -> auto-collapses!
        msg.content = "Final answer".into();
        msg.streaming = false;
        disclosure.prune_completed(std::slice::from_ref(&msg));
        assert!(!disclosure.is_expanded(&msg));
        // User can reopen recorded details after completion
        disclosure.toggle(&msg.id, false);
        assert!(disclosure.is_expanded(&msg));
        disclosure.toggle(&msg.id, false);
        assert!(!disclosure.is_expanded(&msg));
    }

    #[test]
    fn failed_turn_diagnostics_remain_reopenable_after_completion() {
        let mut msg = HermesMessage::new_assistant("fail-1".into());
        let mut disclosure = ActivityDisclosure::default();
        msg.tool_calls.push(HermesToolCall {
            id: "tc-err".into(),
            name: "terminal".into(),
            status: ToolCallStatus::Error,
            output: Some("exit code 1".into()),
            ..Default::default()
        });
        msg.streaming = false;
        assert!(!disclosure.is_expanded(&msg));
        disclosure.toggle(&msg.id, false);
        assert!(disclosure.is_expanded(&msg));
    }

    #[test]
    fn same_length_tool_updates_and_stream_completion_invalidate_row_measurements() {
        let mut msg = HermesMessage::new_assistant("test".into());
        msg.tool_calls.push(HermesToolCall {
            status: ToolCallStatus::Running,
            output: Some("one".into()),
            ..Default::default()
        });
        let before = message_fingerprint(&msg);
        msg.tool_calls[0].output = Some("two".into());
        let changed_output = message_fingerprint(&msg);
        assert_ne!(before, changed_output);
        msg.tool_calls[0].status = ToolCallStatus::Done;
        let changed_status = message_fingerprint(&msg);
        assert_ne!(changed_output, changed_status);
        msg.streaming = false;
        assert_ne!(changed_status, message_fingerprint(&msg));
    }
}
