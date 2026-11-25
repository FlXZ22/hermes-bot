use crate::hermes::types::{
    ApprovalDecision, HermesApprovalRequest, HermesClarifyRequest, HermesMessage,
    HermesSessionUsage, HermesToolCall, MessageRole, ToolCallStatus,
};
use crate::state::app_state::{AppState, AttachmentFile, AvatarEditorTarget};
use crate::theme::bot_face::{bot_face, describe_tool, BotFaceState};
use crate::theme::components::{
    cursor_icon, format_tokens, gb_badge, gb_btn, gb_chip_btn, gb_dot, gb_mini_icon_btn,
    gb_section_card, gb_section_card_head, gb_window_action, short_model_name, BadgeTone,
    BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::{resolve_bot_color, SandTheme};
use crate::ui::markdown::{render_code_block, render_markdown};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use std::collections::HashSet;

const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/model", "Open model & provider picker"),
    ("/restart", "Restart Hermes backend & reload providers"),
    ("/boards", "Switch to Session Boards (Kanban)"),
    ("/compress", "Compress conversation context"),
    ("/branch", "Branch this conversation"),
    ("/skills", "Manage Hermes skills & MCP"),
    ("/cron", "Manage scheduled automations"),
    ("/memory", "Inspect SOUL.md & memories"),
    ("/clear", "Start a fresh chat session"),
];

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

pub struct ChatView {
    app: Entity<AppState>,
    composer: Entity<TextareaState>,
    title_input: Entity<InputState>,
    editing_title: bool,
    attach_menu_open: bool,
    board_menu_open: bool,
    expanded_thinking: HashSet<String>,
    expanded_tools: HashSet<String>,
    collapsed_groups: HashSet<String>,
    scroll_handle: ScrollHandle,
}

impl ChatView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            let mut st =
                TextareaState::new(window, cx).placeholder("Message Hermes... (type / for commands)");
            st.set_auto_grow(1, 5, cx);
            st
        });
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Session title"));

        cx.observe(&app, |_, _, cx| cx.notify()).detach();

        cx.subscribe_in(&composer, window, |this, state, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    if !*shift {
                        let text = state.read(cx).value().to_string();
                        if !text.trim().is_empty() || !this.app.read(cx).attachments.is_empty() {
                            state.update(cx, |st, cx| st.set_value("", window, cx));
                            this.app.update(cx, |s, cx| s.send_message(text, cx));
                            cx.notify();
                        }
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

        Self {
            app,
            composer,
            title_input,
            editing_title: false,
            attach_menu_open: false,
            board_menu_open: false,
            expanded_thinking: HashSet::new(),
            expanded_tools: HashSet::new(),
            collapsed_groups: HashSet::new(),
            scroll_handle: ScrollHandle::new(),
        }
    }

    fn submit_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty() && self.app.read(cx).attachments.is_empty() {
            return;
        }
        self.composer
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.app.update(cx, |s, cx| s.send_message(text, cx));
        self.scroll_handle.scroll_to_bottom();
        cx.notify();
    }

    fn pick_attachment_files(&mut self, cx: &mut Context<Self>) {
        self.attach_menu_open = false;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files to Hermes message".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
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
                                bytes,
                            });
                        }
                    }
                }
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let tick = s.anim_tick;
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
        let bot_state = chat.bot_state();
        let has_messages = !chat.messages.is_empty();
        let loading_history = chat.loading_history;
        let is_streaming = chat.is_streaming;

        let draft = self.composer.read(cx).value().to_string();
        let matching_slash: Vec<(&str, &str)> = if draft.starts_with('/') && !draft.contains(' ') {
            let q = draft.to_lowercase();
            SLASH_COMMANDS
                .iter()
                .copied()
                .filter(|(cmd, _)| cmd.starts_with(&q))
                .collect()
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
        let bot_seed_hero = bot_seed.clone();
        let bot_name_hero = bot_name.clone();
        let hdr_title_edit = header_title.clone();

        div()
            .flex_1()
            .min_w_0()
            .h_full()
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
                    .child(if self.editing_title && active_session_id.is_some() {
                        div()
                            .h(px(32.0))
                            .px(px(10.0))
                            .rounded_full()
                            .bg(theme.bg_chrome)
                            .border_1()
                            .border_color(theme.border_default)
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .child(bot_face(
                                &theme,
                                &bot_seed,
                                bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                                bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                                24.0,
                                bot_state,
                                tick,
                            ))
                            .child(div().w(px(220.0)).child(Input::new(&self.title_input).h(px(24.0))))
                            .into_any_element()
                    } else {
                        let has_sid = active_session_id.is_some();
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
                            .max_w(px(440.0))
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
                            .child(bot_face(
                                &theme,
                                &bot_seed,
                                bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                                bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                                24.0,
                                bot_state,
                                tick,
                            ))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(header_title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_tertiary)
                                    .whitespace_nowrap()
                                    .child(format!("· {}", short_model_name(&active_model))),
                            )
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex()
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
                                                s.refresh_inspector_files(cx);
                                            }
                                            cx.notify();
                                        });
                                    })),
                            ),
                    ),
            )
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
                    .child(cursor_icon("spinner", 18.0, theme.text_tertiary))
                    .child("Loading conversation…")
                    .into_any_element()
            } else if !has_messages {
                // Exact Grok Bot Empty Chat Hero (matches verification-chat.jpg!)
                div()
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
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.avatar_editor_target = Some(AvatarEditorTarget {
                                        seed: bot_seed_hero.clone(),
                                        title: bot_name_hero.clone(),
                                    });
                                    cx.notify();
                                });
                            }))
                            .child(bot_face(
                                &theme,
                                &bot_seed,
                                bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                                bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                                72.0,
                                bot_state,
                                tick,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .line_height(px(30.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(bot_name),
                    )
                    .child(
                        div()
                            .max_w(px(440.0))
                            .text_center()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(theme.text_secondary)
                            .child("Your agent, ready to work. Start a conversation or pick up where you left off."),
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
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(theme.text_primary)
                                                    .child(cursor_icon(icon, 14.0, theme.text_primary))
                                                    .child(title),
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
                    )
                    .into_any_element()
            } else {
                // Transcript scroller
                self.render_transcript(
                    &theme,
                    &chat,
                    &bot_seed,
                    bot_av.as_ref().and_then(|a| a.shape_id.as_deref()),
                    bot_av.as_ref().and_then(|a| a.color_id.as_deref()),
                    bot_state,
                    tick,
                    cx,
                )
                .into_any_element()
            })
            // Bottom Composer Area (controls strip + UsageBar + attachments + floating pill)
            .child(
                div()
                    .w_full()
                    .px(px(20.0))
                    .pt(px(8.0))
                    .pb(px(16.0))
                    .bg(theme.bg_pane)
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(740.0))
                            .max_w_full()
                            .flex()
                            .flex_col()
                            .relative()
                            // Slash command autocomplete popover
                            .when(!matching_slash.is_empty(), |d| {
                                d.child(
                                    div()
                                        .mb(px(8.0))
                                        .w(px(360.0))
                                        .max_w_full()
                                        .p(px(5.0))
                                        .rounded(px(14.0))
                                        .bg(theme.bg_elevated)
                                        .border_1()
                                        .border_color(theme.border_default)
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .children(matching_slash.into_iter().map(|(cmd, desc)| {
                                            let cmd_s = cmd.to_string();
                                            div()
                                                .id(SharedString::from(format!("slash-{cmd}")))
                                                .w_full()
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
                                                    this.composer.update(cx, |st, cx| {
                                                        st.set_value("", window, cx);
                                                    });
                                                    this.app.update(cx, |s, cx| {
                                                        s.send_message(cmd_s.clone(), cx);
                                                    });
                                                }))
                                                .child(
                                                    div()
                                                        .font_family(FONT_MONO)
                                                        .text_size(px(12.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child(cmd),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(theme.text_tertiary)
                                                        .child(desc),
                                                )
                                        })),
                                )
                            })
                            // Board assignment popover
                            .when(self.board_menu_open, |d| {
                                let sid_assign = active_session_id.clone();
                                d.child(
                                    div()
                                        .mb(px(8.0))
                                        .self_end()
                                        .w(px(240.0))
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
                                )
                            })
                            // Controls strip (model, effort, fast, ask/yolo, board)
                            .child(
                                div()
                                    .px(px(10.0))
                                    .pb(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .flex_wrap()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .flex_wrap()
                                            .gap(px(6.0))
                                            .child(
                                                gb_chip_btn("chip-model", &theme, false)
                                                    .child(cursor_icon("sparkles", 12.0, theme.text_secondary))
                                                    .child(short_model_name(&active_model))
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
                                                .child(gb_dot(preset.hex, 7.0))
                                                .child(b.name)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.board_menu_open = !this.board_menu_open;
                                                    cx.notify();
                                                })),
                                        )
                                    }),
                            )
                            // Context UsageBar
                            .child(render_usage_bar(
                                &theme,
                                chat.session_usage.as_ref(),
                                active_session.as_ref().and_then(|x| x.context_limit),
                            ))
                            // Attachments row
                            .when(!attachments.is_empty(), |d| {
                                d.child(
                                    div()
                                        .px(px(12.0))
                                        .pb(px(8.0))
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(6.0))
                                        .children(attachments.iter().enumerate().map(|(i, att)| {
                                            div()
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
                                                .child(att.name.clone())
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
                                d.child(
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
                                )
                            })
                            // Floating Grok Bot Pill Composer (rounded 26px)
                            .child(
                                div()
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(7.0))
                                    .rounded(px(26.0))
                                    .bg(theme.bg_chrome)
                                    .border_1()
                                    .border_color(theme.border_default)
                                    .flex()
                                    .items_end()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .id("composer-plus")
                                            .w(px(30.0))
                                            .h(px(30.0))
                                            .rounded_full()
                                            .bg(theme.bg_secondary)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .flex_shrink_0()
                                            .cursor_pointer()
                                            .hover(|st| st.bg(theme.bg_active))
                                            .child(cursor_icon("plus", 15.0, theme.text_secondary))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.attach_menu_open = !this.attach_menu_open;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                Textarea::new(&self.composer)
                                                    .appearance(false)
                                                    .bordered(false),
                                            ),
                                    )
                                    .child(if is_streaming {
                                        div()
                                            .id("composer-stop")
                                            .w(px(30.0))
                                            .h(px(30.0))
                                            .rounded_full()
                                            .bg(theme.danger_primary)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .flex_shrink_0()
                                            .cursor_pointer()
                                            .child(cursor_icon("stop", 13.0, rgb(0xffffff).into()))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |s, cx| s.interrupt_stream(cx));
                                            }))
                                    } else {
                                        let can_send = !draft.trim().is_empty() || !attachments.is_empty();
                                        div()
                                            .id("composer-send")
                                            .w(px(30.0))
                                            .h(px(30.0))
                                            .rounded_full()
                                            .bg(theme.bg_primary)
                                            .opacity(if can_send { 1.0 } else { 0.4 })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .flex_shrink_0()
                                            .when(can_send, |d| d.cursor_pointer())
                                            .child(cursor_icon("arrow-up", 15.0, theme.text_on_primary))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.submit_composer(window, cx);
                                            }))
                                    }),
                            ),
                    ),
            )
    }
}

impl ChatView {
    #[allow(clippy::too_many_arguments)]
    fn render_transcript(
        &self,
        theme: &SandTheme,
        chat: &crate::state::app_state::SessionChatState,
        bot_seed: &str,
        shape_id: Option<&str>,
        color_id: Option<&str>,
        bot_state: BotFaceState,
        tick: u64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let has_inline_activity = chat.messages.last().is_some_and(|m| {
            m.streaming
                && ((m.thinking.is_some() && m.content.is_empty())
                    || m.tool_calls
                        .iter()
                        .any(|t| t.status == ToolCallStatus::Running))
        });

        div()
            .id("chat-transcript-scroll")
            .track_scroll(&self.scroll_handle)
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .px(px(24.0))
            .py(px(14.0))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(740.0))
                    .max_w_full()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    // Load older messages button
                    .when(chat.older_offset.is_some(), |d| {
                        d.child(
                            div()
                                .w_full()
                                .flex()
                                .justify_center()
                                .child(
                                    gb_btn(
                                        "load-older-msgs",
                                        theme,
                                        BtnVariant::Secondary,
                                        true,
                                        chat.loading_history,
                                    )
                                    .child(if chat.loading_history {
                                        "Loading…"
                                    } else {
                                        "Load older messages"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| s.load_older_messages(cx));
                                    })),
                                ),
                        )
                    })
                    // Live Agent Plan Todo Checklist
                    .when(!chat.todos.is_empty(), |d| {
                        let done_count = chat
                            .todos
                            .iter()
                            .filter(|t| t.status == "completed")
                            .count();
                        d.child(
                            gb_section_card(theme)
                                .child(
                                    gb_section_card_head(theme).py(px(9.0)).px(px(13.0)).child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(cursor_icon("list-todo", 13.0, theme.text_primary))
                                            .child(format!(
                                                "Agent Plan ({done_count}/{})",
                                                chat.todos.len()
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
                                        .children(chat.todos.iter().map(|t| {
                                            let (icon, col) = match t.status.as_str() {
                                                "completed" => ("check-circle", theme.success_text),
                                                "in_progress" => ("spinner", theme.accent_text),
                                                _ => ("circle", theme.text_tertiary),
                                            };
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(7.0))
                                                .text_size(px(12.5))
                                                .text_color(if t.status == "completed" {
                                                    theme.text_tertiary
                                                } else {
                                                    theme.text_primary
                                                })
                                                .child(cursor_icon(icon, 13.0, col))
                                                .child(t.content.clone())
                                        })),
                                ),
                        )
                    })
                    // Messages
                    .children(chat.messages.iter().map(|msg| {
                        self.render_message(msg, theme, bot_seed, shape_id, color_id, tick, cx)
                    }))
                    // Pending indicator
                    .when(chat.is_streaming && !has_inline_activity, |d| {
                        let status_lbl = if chat.live_status_text.is_empty() {
                            "Hermes is thinking".to_string()
                        } else {
                            chat.live_status_text.clone()
                        };
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .py(px(4.0))
                                .child(bot_face(
                                    theme,
                                    bot_seed,
                                    shape_id,
                                    color_id,
                                    28.0,
                                    bot_state,
                                    tick,
                                ))
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .text_color(theme.text_tertiary)
                                        .child(format!("{status_lbl} …")),
                                ),
                        )
                    })
                    .child(div().h(px(24.0))),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_message(
        &self,
        msg: &HermesMessage,
        theme: &SandTheme,
        bot_seed: &str,
        shape_id: Option<&str>,
        color_id: Option<&str>,
        tick: u64,
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
            };
            return div()
                .w_full()
                .flex()
                .flex_col()
                .items_start()
                .child(self.render_tool_card(&synth, theme, cx));
        }

        if msg.role == MessageRole::User {
            return div()
                .w_full()
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .max_w(px(560.0))
                        .px(px(14.0))
                        .py(px(9.0))
                        .rounded(px(19.0))
                        .bg(theme.bubble_user)
                        .text_color(theme.bubble_user_ink)
                        .text_size(px(14.5))
                        .line_height(px(21.0))
                        .child(msg.content.clone()),
                );
        }

        let msg_id = msg.id.clone();
        let copy_content = msg.content.clone();
        let think_open = self.expanded_thinking.contains(&msg.id);
        let think_streaming = msg.streaming
            && msg.content.is_empty()
            && !msg
                .tool_calls
                .iter()
                .any(|t| t.status == ToolCallStatus::Running);

        div()
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.0))
            // Collapsible Thought Process
            .when_some(msg.thinking.as_ref().filter(|t| !t.is_empty()), |d, thinking| {
                let tid = msg_id.clone();
                d.child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id(SharedString::from(format!("think-head-{msg_id}")))
                                .min_h(px(28.0))
                                .py(px(3.0))
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_tertiary)
                                .cursor_pointer()
                                .hover(|st| st.text_color(theme.text_secondary))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.expanded_thinking.remove(&tid) {
                                        this.expanded_thinking.insert(tid.clone());
                                    }
                                    cx.notify();
                                }))
                                .child(if think_streaming {
                                    bot_face(
                                        theme,
                                        bot_seed,
                                        shape_id,
                                        color_id,
                                        22.0,
                                        BotFaceState::Thinking,
                                        tick,
                                    )
                                    .into_any_element()
                                } else {
                                    cursor_icon("brain", 16.0, theme.text_tertiary).into_any_element()
                                })
                                .child(if think_streaming {
                                    "Thinking…"
                                } else {
                                    "Thought process"
                                })
                                .child(cursor_icon(
                                    if think_open {
                                        "chevron-down"
                                    } else {
                                        "chevron-right"
                                    },
                                    12.0,
                                    theme.text_tertiary,
                                )),
                        )
                        .when(think_open, |d| {
                            d.child(
                                div()
                                    .ml(px(8.0))
                                    .my(px(4.0))
                                    .pl(px(14.0))
                                    .py(px(4.0))
                                    .border_l_1()
                                    .border_color(theme.border_default)
                                    .text_size(px(12.5))
                                    .line_height(px(19.0))
                                    .text_color(theme.text_secondary)
                                    .child(thinking.clone()),
                            )
                        }),
                )
            })
            // Tool activity group
            .when(!msg.tool_calls.is_empty(), |d| {
                let grp_id = format!("grp-{}", msg.id);
                let grp_open = !self.collapsed_groups.contains(&grp_id);
                let active_tool = if msg.streaming {
                    msg.tool_calls
                        .iter()
                        .rev()
                        .find(|t| t.status == ToolCallStatus::Running)
                } else {
                    None
                };
                let summary_text = if let Some(t) = active_tool {
                    let (_, lbl, detail, _) =
                        describe_tool(&t.name, t.arguments.as_ref(), t.preview.as_deref(), true);
                    format!("{lbl} {detail}")
                } else {
                    format!("Used {} tool{}", msg.tool_calls.len(), if msg.tool_calls.len() == 1 { "" } else { "s" })
                };
                let gid_click = grp_id.clone();
                d.child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id(SharedString::from(grp_id))
                                .min_h(px(28.0))
                                .py(px(3.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(13.0))
                                .text_color(theme.text_tertiary)
                                .cursor_pointer()
                                .hover(|st| st.text_color(theme.text_secondary))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.collapsed_groups.remove(&gid_click) {
                                        this.collapsed_groups.insert(gid_click.clone());
                                    }
                                    cx.notify();
                                }))
                                .child(if let Some(t) = active_tool {
                                    let (_, _, _, st) = describe_tool(
                                        &t.name,
                                        t.arguments.as_ref(),
                                        t.preview.as_deref(),
                                        true,
                                    );
                                    bot_face(theme, bot_seed, shape_id, color_id, 22.0, st, tick)
                                        .into_any_element()
                                } else {
                                    cursor_icon("layers", 16.0, theme.text_tertiary)
                                        .into_any_element()
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(summary_text),
                                )
                                .child(cursor_icon(
                                    if grp_open {
                                        "chevron-down"
                                    } else {
                                        "chevron-right"
                                    },
                                    12.0,
                                    theme.text_tertiary,
                                )),
                        )
                        .when(grp_open, |d| {
                            d.child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .children(
                                        msg.tool_calls
                                            .iter()
                                            .map(|tc| self.render_tool_card(tc, theme, cx)),
                                    ),
                            )
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
            // Main assistant markdown bubble
            .when(!msg.content.is_empty(), |d| {
                d.child(
                    div()
                        .w_full()
                        .px(px(16.0))
                        .py(px(12.0))
                        .rounded(px(18.0))
                        .bg(theme.bubble_agent)
                        .child(render_markdown(&msg.id, &msg.content, theme)),
                )
            })
            // Footer meta (model + copy)
            .when(!msg.content.is_empty() && !msg.streaming, |d| {
                d.child(
                    div()
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(11.0))
                        .text_color(theme.text_tertiary)
                        .when_some(msg.model.as_ref(), |d, m| d.child(short_model_name(m)))
                        .child(
                            gb_mini_icon_btn(
                                SharedString::from(format!("copy-msg-{}", msg.id)),
                                theme,
                                false,
                            )
                            .child(cursor_icon("copy", 11.0, theme.text_tertiary))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_content.clone()));
                            }),
                        ),
                )
            })
    }

    fn render_tool_card(
        &self,
        tool: &HermesToolCall,
        theme: &SandTheme,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.expanded_tools.contains(&tool.id);
        let tid = tool.id.clone();
        let running = tool.status == ToolCallStatus::Running;
        let (icon, label, detail, _) =
            describe_tool(&tool.name, tool.arguments.as_ref(), tool.preview.as_deref(), running);

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(SharedString::from(format!("tool-head-{}", tool.id)))
                    .w_full()
                    .min_h(px(28.0))
                    .py(px(3.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .text_color(theme.text_tertiary)
                    .cursor_pointer()
                    .hover(|st| st.text_color(theme.text_secondary))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded_tools.remove(&tid) {
                            this.expanded_tools.insert(tid.clone());
                        }
                        cx.notify();
                    }))
                    .child(cursor_icon(icon, 16.0, theme.text_tertiary))
                    .child(div().flex_shrink_0().child(label))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(detail),
                    )
                    .when(tool.status == ToolCallStatus::Error, |d| {
                        d.child(div().text_size(px(11.0)).text_color(theme.danger_text).child("Failed"))
                    })
                    .when(running, |d| {
                        d.child(div().text_size(px(11.0)).text_color(theme.text_tertiary).child("Running"))
                    })
                    .when_some(tool.duration_ms, |d, ms| {
                        d.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_tertiary)
                                .child(format!("{:.1}s", (ms as f64) / 1000.0)),
                        )
                    })
                    .child(cursor_icon(
                        if open { "chevron-down" } else { "chevron-right" },
                        12.0,
                        theme.text_tertiary,
                    )),
            )
            .when(open, |d| {
                d.child(
                    div()
                        .ml(px(8.0))
                        .my(px(4.0))
                        .pl(px(14.0))
                        .py(px(4.0))
                        .border_l_1()
                        .border_color(theme.border_default)
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .when_some(tool.arguments.as_ref(), |d, args| {
                            let pretty = serde_json::to_string_pretty(args)
                                .unwrap_or_else(|_| args.to_string());
                            d.child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.0))
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
                        .when_some(tool.output.as_ref(), |d, out| {
                            d.child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child("OUTPUT"),
                            )
                            .child(render_code_block(
                                SharedString::from(format!("tool-out-{}", tool.id)),
                                "text",
                                out,
                                theme,
                            ))
                        })
                        .when(tool.arguments.is_none() && tool.output.is_none(), |d| {
                            d.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_tertiary)
                                    .child("No payload available yet."),
                            )
                        }),
                )
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
        div()
            .w_full()
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
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_tertiary)
                    .child("Needs your OK"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(ap.title.clone()),
            )
            .when_some(ap.detail.as_ref().filter(|s| !s.is_empty()), |d, detail| {
                d.child(
                    div()
                        .p(px(10.0))
                        .rounded(px(10.0))
                        .bg(theme.bg_card)
                        .font_family(FONT_MONO)
                        .text_size(px(12.0))
                        .text_color(theme.text_secondary)
                        .child(detail.clone()),
                )
            })
            .child(if let Some(dec) = &ap.settled {
                let (tone, txt) = match dec {
                    ApprovalDecision::Denied => (BadgeTone::Danger, "Denied"),
                    ApprovalDecision::Always => (BadgeTone::Success, "Always allowed"),
                    ApprovalDecision::Allowed => (BadgeTone::Success, "Allowed once"),
                };
                div().mt(px(4.0)).child(gb_badge(theme, tone, txt))
            } else {
                div()
                    .mt(px(6.0))
                    .flex()
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
        div()
            .w_full()
            .max_w(px(560.0))
            .rounded(px(22.0))
            .bg(theme.bg_elevated)
            .border_1()
            .border_color(theme.border_default)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(8.0))
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(cl.question.clone()),
            )
            .child(if let Some(ans) = &cl.answered {
                div()
                    .px(px(16.0))
                    .pb(px(14.0))
                    .child(gb_badge(theme, BadgeTone::Success, format!("Answered: {ans}")))
            } else {
                let choices = cl.choices.clone().unwrap_or_default();
                let cid = cl.id.clone();
                div()
                    .px(px(12.0))
                    .pb(px(14.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(choices.into_iter().enumerate().map(|(i, choice)| {
                        let cid2 = cid.clone();
                        let ans = choice.clone();
                        div()
                            .id(SharedString::from(format!("clarify-{cid}-{i}")))
                            .w_full()
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
                            .child(div().text_size(px(13.5)).child(choice))
                    }))
            })
    }
}

fn render_usage_bar(
    theme: &SandTheme,
    usage: Option<&HermesSessionUsage>,
    context_limit: Option<u64>,
) -> Div {
    let used = usage.and_then(|u| u.context_tokens);
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
    let fill_w = (pct.unwrap_or(0.0) / 100.0) * 64.0;

    div()
        .w_full()
        .px(px(12.0))
        .pb(px(7.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .text_size(px(10.0))
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
        .child(label)
        .when(known, |d| {
            d.child(
                div()
                    .ml_auto()
                    .child(format!("{}%", pct.unwrap_or(0.0).round() as u32)),
            )
        })
}
