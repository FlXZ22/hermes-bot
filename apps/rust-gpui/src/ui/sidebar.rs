use crate::hermes::types::HermesSession;
use crate::state::app_state::{AppState, AppViewId, AvatarEditorTarget, ConnectionStatus};
use crate::theme::bot_face::{bot_face, BotFaceState};
use crate::theme::components::{
    cursor_icon, format_relative_time, gb_board_pill, gb_btn, gb_circle_btn, gb_dot,
    gb_mini_icon_btn, gb_traffic_lights, BtnVariant, FONT_SANS,
};
use crate::theme::tokens::{resolve_bot_color, SandTheme, ThemeMode};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};

// Exact Grok Bot navigation dock WITHOUT Work (as requested by user)
const NAV_ITEMS: &[(AppViewId, &str, &str)] = &[
    (AppViewId::Chat, "Chat", "chat-bubble"),
    (AppViewId::Boards, "Boards", "board-kanban"),
    (AppViewId::Skills, "Skills", "sparkles"),
    (AppViewId::Cron, "Cron", "clock"),
    (AppViewId::Memory, "Memory", "brain"),
    (AppViewId::Gateway, "Gateway", "globe"),
    (AppViewId::Analytics, "Usage", "chart-bars"),
];

const BOARD_COLORS: &[&str] = &[
    "blue", "violet", "cyan", "green", "orange", "red", "magenta", "yellow",
];

pub struct SidebarView {
    app: Entity<AppState>,
    search_input: Entity<InputState>,
    show_archived: bool,
    hovered_session_id: Option<String>,
    renaming_id: Option<String>,
    rename_input: Entity<InputState>,
    // Add Bot modal state
    new_bot_open: bool,
    bot_name_input: Entity<InputState>,
    bot_desc_input: Entity<InputState>,
    bot_busy: bool,
    bot_error: String,
    // New Board modal state
    new_board_open: bool,
    board_name_input: Entity<InputState>,
    board_desc_input: Entity<InputState>,
    board_path_input: Entity<InputState>,
    board_color: String,
}

impl SidebarView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions..."));
        let rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("Session title"));
        let bot_name_input = cx.new(|cx| InputState::new(window, cx).placeholder("research"));
        let bot_desc_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Optional purpose for this bot"));
        let board_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. Backend Rust Refactor, Research, Ops")
        });
        let board_desc_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Optional workspace purpose or notes")
        });
        let board_path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("/home/metis/Projects/..."));

        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&search_input, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&rename_input, |this, inp, ev: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                if let Some(sid) = this.renaming_id.take() {
                    let val = inp.read(cx).value().to_string();
                    this.app
                        .update(cx, |s, cx| s.rename_session(sid, val, cx));
                    cx.notify();
                }
            }
        })
        .detach();

        Self {
            app,
            search_input,
            show_archived: false,
            hovered_session_id: None,
            renaming_id: None,
            rename_input,
            new_bot_open: false,
            bot_name_input,
            bot_desc_input,
            bot_busy: false,
            bot_error: String::new(),
            new_board_open: false,
            board_name_input,
            board_desc_input,
            board_path_input,
            board_color: "violet".into(),
        }
    }

    fn create_bot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.bot_busy {
            return;
        }
        let name = self.bot_name_input.read(cx).value().trim().to_string();
        let desc = self.bot_desc_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let (client, rt, exists) = {
            let s = self.app.read(cx);
            (
                s.client.clone(),
                s.tokio_rt.clone(),
                s.profiles
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(&name)),
            )
        };
        let Some(client) = client else {
            self.bot_error = "Connect to Hermes to add a bot.".into();
            cx.notify();
            return;
        };
        if exists {
            self.bot_error = "A bot with that name already exists.".into();
            cx.notify();
            return;
        }
        self.bot_busy = true;
        self.bot_error.clear();
        cx.notify();

        self.bot_name_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.bot_desc_input
            .update(cx, |st, cx| st.set_value("", window, cx));

        cx.spawn(async move |this, cx| {
            let n2 = name.clone();
            let res = rt
                .spawn(async move {
                    client
                        .create_profile(&n2, if desc.is_empty() { None } else { Some(&desc) })
                        .await
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.bot_busy = false;
                match res {
                    Ok(_) => {
                        v.new_bot_open = false;
                        v.app.update(cx, |s, cx| {
                            s.refresh_capabilities(false, cx);
                            s.switch_profile(name, cx);
                        });
                    }
                    Err(e) => v.bot_error = e,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.board_name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let desc = self.board_desc_input.read(cx).value().trim().to_string();
        let path = self.board_path_input.read(cx).value().trim().to_string();
        let color = self.board_color.clone();
        self.app
            .update(cx, |s, cx| s.create_board(name, desc, color, path, cx));
        self.board_name_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.board_desc_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.board_path_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.new_board_open = false;
        cx.notify();
    }

    fn render_session_row(
        &self,
        sess: &HermesSession,
        theme: &SandTheme,
        collapsed: bool,
        active_view: AppViewId,
        active_session_id: Option<&str>,
        active_streaming: bool,
        active_bot_state: BotFaceState,
        active_board_slug: &str,
        tick: u64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let s = self.app.read(cx);
        let is_active = active_view == AppViewId::Chat && active_session_id == Some(sess.id.as_str());
        let seed = sess
            .profile
            .as_deref()
            .filter(|p| !p.is_empty())
            .unwrap_or(&sess.id);
        let av = s.bot_avatars.get(seed);
        let is_working = is_active && active_streaming;
        let assigned_slug = s
            .session_boards
            .get(&sess.id)
            .map(String::as_str)
            .or(sess.board_slug.as_deref());
        let assigned_board_name = assigned_slug
            .filter(|sl| *sl != "default" && active_board_slug == "all")
            .and_then(|sl| s.boards.iter().find(|b| b.slug == sl).map(|b| b.name.clone()));

        let face_size = if collapsed {
            22.0
        } else if sess.archived {
            24.0
        } else {
            36.0
        };
        let face_state = if is_active {
            active_bot_state
        } else if sess.archived {
            BotFaceState::Sleeping
        } else {
            BotFaceState::Idle
        };

        let sid = sess.id.clone();
        let sid_pin = sess.id.clone();
        let sid_ren = sess.id.clone();
        let sid_arch = sess.id.clone();
        let title_for_ren = sess.title.clone().unwrap_or_default();
        let archived = sess.archived;
        let pinned = sess.pinned;
        let is_renaming = self.renaming_id.as_deref() == Some(sess.id.as_str());
        let time_str = format_relative_time(sess.timestamp_secs());
        let preview_text = sess
            .preview
            .clone()
            .or_else(|| sess.model.clone())
            .unwrap_or_else(|| format!("{} messages", sess.message_count.unwrap_or(0)));
        let hover_bg = theme.sidebar_hover;
        let is_hovered = self.hovered_session_id.as_deref() == Some(sess.id.as_str());
        let sid_hover = sess.id.clone();

        div()
            .id(SharedString::from(format!("sess-row-{}", sess.id)))
            .w_full()
            .flex()
            .items_center()
            .when(collapsed, |d| d.justify_center().px(px(4.0)).py(px(7.0)))
            .when(!collapsed, |d| d.gap(px(10.0)).p(px(8.0)))
            .rounded(px(11.0))
            .bg(if is_active {
                theme.sidebar_selected
            } else {
                SandTheme::transparent()
            })
            .cursor_pointer()
            .hover(move |st| st.bg(hover_bg))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    if this.hovered_session_id.as_deref() != Some(sid_hover.as_str()) {
                        this.hovered_session_id = Some(sid_hover.clone());
                        cx.notify();
                    }
                } else if this.hovered_session_id.as_deref() == Some(sid_hover.as_str()) {
                    this.hovered_session_id = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.renaming_id.as_deref() != Some(sid.as_str()) {
                    this.app
                        .update(cx, |s, cx| s.select_session(Some(sid.clone()), cx));
                }
            }))
            .child(bot_face(
                theme,
                seed,
                av.and_then(|a| a.shape_id.as_deref()),
                if sess.archived {
                    Some("gray")
                } else {
                    av.and_then(|a| a.color_id.as_deref())
                },
                face_size,
                face_state,
                tick,
            ))
            .when(!collapsed, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.0))
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(6.0))
                                .child(if is_renaming {
                                    div()
                                        .flex_1()
                                        .child(Input::new(&self.rename_input).h(px(24.0)))
                                        .into_any_element()
                                } else {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_size(px(13.5))
                                        .line_height(px(19.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.sidebar_text_primary)
                                        .child(sess.display_title().to_string())
                                        .into_any_element()
                                })
                                .child(if is_hovered && !is_renaming {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.0))
                                        .flex_shrink_0()
                                        .child(
                                            gb_mini_icon_btn(
                                                SharedString::from(format!("pin-{}", sess.id)),
                                                theme,
                                                false,
                                            )
                                            .child(cursor_icon(
                                                if pinned { "pin-slash" } else { "pin" },
                                                12.0,
                                                theme.sidebar_text_secondary,
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.app.update(cx, |s, cx| {
                                                    s.toggle_pin_session(sid_pin.clone(), cx);
                                                });
                                            })),
                                        )
                                        .child(
                                            gb_mini_icon_btn(
                                                SharedString::from(format!("ren-{}", sess.id)),
                                                theme,
                                                false,
                                            )
                                            .child(cursor_icon("pencil", 12.0, theme.sidebar_text_secondary))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.renaming_id = Some(sid_ren.clone());
                                                let init_t = title_for_ren.clone();
                                                this.rename_input.update(cx, |st, cx| {
                                                    st.set_value(init_t, window, cx);
                                                });
                                                cx.notify();
                                            })),
                                        )
                                        .child(
                                            gb_mini_icon_btn(
                                                SharedString::from(format!("arch-{}", sess.id)),
                                                theme,
                                                true,
                                            )
                                            .child(cursor_icon("archive", 12.0, theme.sidebar_text_secondary))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.app.update(cx, |s, cx| {
                                                    s.archive_session(sid_arch.clone(), !archived, cx);
                                                });
                                            })),
                                        )
                                        .into_any_element()
                                } else {
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(11.0))
                                        .text_color(theme.sidebar_text_tertiary)
                                        .child(time_str)
                                        .into_any_element()
                                }),
                        )
                        .child(if is_working {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .text_size(px(12.0))
                                .text_color(theme.accent_text)
                                .child(cursor_icon("spinner", 11.0, theme.accent_text))
                                .child("Working...")
                                .into_any_element()
                        } else {
                            let label = if let Some(bname) = assigned_board_name {
                                format!("[{bname}] {preview_text}")
                            } else {
                                preview_text
                            };
                            div()
                                .w_full()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(12.5))
                                .line_height(px(17.0))
                                .text_color(theme.sidebar_text_secondary)
                                .child(label)
                                .into_any_element()
                        }),
                )
            })
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.app.read(cx);
        let theme = s.sand_theme();
        let mode = s.theme_mode();
        let collapsed = s.sidebar_collapsed;
        let active_view = s.active_view;
        let active_session_id = s.active_session_id.clone();
        let active_profile = s.active_profile.clone();
        let active_board_slug = s.active_board_slug.clone();
        let profile_switching = s.profile_switching;
        let conn_status = s.connection_status;
        let tick = s.anim_tick;
        let active_chat = s.active_chat();
        let active_bot_state = active_chat.bot_state();
        let active_streaming = active_chat.is_streaming;

        let search_q = self.search_input.read(cx).value().trim().to_lowercase();
        let filtered: Vec<HermesSession> = s
            .sessions
            .iter()
            .filter(|sess| {
                if sess.archived != self.show_archived {
                    return false;
                }
                if active_board_slug != "all" {
                    let assigned = s
                        .session_boards
                        .get(&sess.id)
                        .map(String::as_str)
                        .or(sess.board_slug.as_deref())
                        .unwrap_or("default");
                    if assigned != active_board_slug {
                        return false;
                    }
                }
                if search_q.is_empty() {
                    return true;
                }
                let hay = format!(
                    "{} {} {} {}",
                    sess.title.as_deref().unwrap_or(""),
                    sess.preview.as_deref().unwrap_or(""),
                    sess.model.as_deref().unwrap_or(""),
                    sess.profile.as_deref().unwrap_or("")
                )
                .to_lowercase();
                hay.contains(&search_q)
            })
            .cloned()
            .collect();

        let pinned: Vec<HermesSession> = filtered.iter().filter(|x| x.pinned).cloned().collect();
        let recent: Vec<HermesSession> = filtered.iter().filter(|x| !x.pinned).cloned().collect();
        let profiles = s.profiles.clone();
        let boards = s.boards.clone();
        let bot_avatars = s.bot_avatars.clone();

        let status_dot_color = match conn_status {
            ConnectionStatus::Connected => theme.success_primary,
            ConnectionStatus::Local | ConnectionStatus::Connecting => theme.warn_primary,
            ConnectionStatus::Error => theme.danger_primary,
            ConnectionStatus::Disconnected => theme.sidebar_text_tertiary,
        };
        let status_label = match conn_status {
            ConnectionStatus::Connected => "Hermes Live",
            ConnectionStatus::Local => "Local Rust",
            ConnectionStatus::Connecting => "Connecting...",
            ConnectionStatus::Error | ConnectionStatus::Disconnected => "Offline",
        };

        let section_label = if self.show_archived {
            "Archived".to_string()
        } else if active_board_slug == "all" {
            "Sessions".to_string()
        } else {
            let bname = boards
                .iter()
                .find(|b| b.slug == active_board_slug)
                .map(|b| b.name.as_str())
                .unwrap_or(&active_board_slug);
            format!("Board · {bname}")
        };

        let sidebar_w = if collapsed { 68.0 } else { 280.0 };

        div()
            .w(px(sidebar_w))
            .h_full()
            .bg(theme.bg_sidebar)
            .text_color(theme.sidebar_text_primary)
            .border_r_1()
            .border_color(theme.sidebar_separator)
            .font_family(FONT_SANS)
            .flex()
            .flex_col()
            .flex_shrink_0()
            // Top bar: Traffic lights + collapse + new chat
            .child(
                div()
                    .h(px(56.0))
                    .w_full()
                    .px(px(if collapsed { 8.0 } else { 14.0 }))
                    .flex()
                    .items_center()
                    .when(collapsed, |d| d.justify_center())
                    .when(!collapsed, |d| d.justify_between())
                    .flex_shrink_0()
                    .when(!collapsed, |d| d.child(gb_traffic_lights()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                gb_circle_btn("sidebar-collapse-btn", &theme, false)
                                    .child(cursor_icon(
                                        if collapsed {
                                            "layout-sidebar-left-on"
                                        } else {
                                            "layout-sidebar-left"
                                        },
                                        15.0,
                                        theme.sidebar_text_primary,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| {
                                            s.sidebar_collapsed = !s.sidebar_collapsed;
                                            cx.notify();
                                        });
                                    })),
                            )
                            .when(!collapsed, |d| {
                                d.child(
                                    gb_circle_btn("sidebar-new-chat-btn", &theme, false)
                                        .child(cursor_icon(
                                            "chat-bubble-plus",
                                            15.0,
                                            theme.sidebar_text_primary,
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| {
                                                s.create_new_session(None, None, None, None, None, cx);
                                            });
                                        })),
                                )
                            }),
                    ),
            )
            // Search input
            .when(!collapsed, |d| {
                d.child(
                    div()
                        .h(px(32.0))
                        .mx(px(14.0))
                        .mb(px(8.0))
                        .px(px(10.0))
                        .rounded(px(10.0))
                        .bg(theme.bg_tertiary)
                        .border_1()
                        .border_color(theme.sidebar_separator)
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .flex_shrink_0()
                        .child(cursor_icon("search", 13.0, theme.sidebar_text_tertiary))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    Input::new(&self.search_input)
                                        .appearance(false)
                                        .bordered(false)
                                        .h(px(28.0)),
                                ),
                        )
                        .when(!search_q.is_empty(), |d| {
                            d.child(
                                gb_mini_icon_btn("sidebar-clear-search", &theme, false)
                                    .child(cursor_icon("close", 11.0, theme.sidebar_text_tertiary))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.search_input
                                            .update(cx, |st, cx| st.set_value("", window, cx));
                                        cx.notify();
                                    })),
                            )
                        }),
                )
            })
            // Session Board pills strip
            .when(!collapsed, |d| {
                d.child(
                    div()
                        .id("sidebar-boards-strip")
                        .mx(px(14.0))
                        .mb(px(8.0))
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .overflow_x_scroll()
                        .flex_shrink_0()
                        .child(
                            gb_board_pill("sb-board-all", &theme, active_board_slug == "all")
                                .child("All")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.app.update(cx, |s, cx| {
                                        s.active_board_slug = "all".into();
                                        cx.notify();
                                    });
                                })),
                        )
                        .children(boards.iter().map(|b| {
                            let slug = b.slug.clone();
                            let active = active_board_slug == b.slug;
                            let preset = resolve_bot_color(&b.slug, b.color.as_deref());
                            gb_board_pill(
                                SharedString::from(format!("sb-board-{}", b.slug)),
                                &theme,
                                active,
                            )
                            .child(gb_dot(preset.hex, 7.0))
                            .child(b.name.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.active_board_slug = slug.clone();
                                    cx.notify();
                                });
                            }))
                        }))
                        .child(
                            gb_board_pill("sb-board-add", &theme, false)
                                .child(cursor_icon("plus", 11.0, theme.sidebar_text_secondary))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.new_board_open = true;
                                    cx.notify();
                                })),
                        ),
                )
            })
            // Main scrollable area: Hermes Bots grid + Pinned + Sessions
            .child(
                div()
                    .id("sidebar-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(if collapsed { 8.0 } else { 12.0 }))
                    .pb(px(12.0))
                    .flex()
                    .flex_col()
                    // 3-column Hermes Bots grid
                    .when(!collapsed, |d| {
                        let active_prof_for_btn = active_profile.clone();
                        let active_title_for_btn = bot_avatars
                            .get(&active_profile)
                            .and_then(|a| a.display_name.clone())
                            .unwrap_or_else(|| active_profile.clone());
                        d.child(
                            div()
                                .px(px(6.0))
                                .py(px(5.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.sidebar_text_tertiary)
                                .child("Hermes Bots")
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .child(
                                            gb_mini_icon_btn("sb-add-bot", &theme, false)
                                                .child(cursor_icon("plus", 13.0, theme.sidebar_text_tertiary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.bot_error.clear();
                                                    this.new_bot_open = true;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_mini_icon_btn("sb-customize-bot", &theme, false)
                                                .child(cursor_icon("sliders", 12.0, theme.sidebar_text_tertiary))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.app.update(cx, |s, cx| {
                                                        s.avatar_editor_target = Some(AvatarEditorTarget {
                                                            seed: active_prof_for_btn.clone(),
                                                            title: active_title_for_btn.clone(),
                                                        });
                                                        cx.notify();
                                                    });
                                                })),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .py(px(4.0))
                                .pb(px(10.0))
                                .flex()
                                .flex_wrap()
                                .gap(px(4.0))
                                .children(profiles.iter().map(|prof| {
                                    let pname = prof.name.clone();
                                    let pname_ctx = prof.name.clone();
                                    let av = bot_avatars.get(&prof.name);
                                    let active = active_profile == prof.name;
                                    let display_name = av
                                        .and_then(|a| a.display_name.clone())
                                        .unwrap_or_else(|| {
                                            if prof.name == "default" {
                                                "Hermes".into()
                                            } else {
                                                prof.name.clone()
                                            }
                                        });
                                    let display_ctx = display_name.clone();
                                    let tile_state = if active {
                                        active_bot_state
                                    } else {
                                        BotFaceState::Idle
                                    };
                                    let hover_bg = theme.sidebar_hover;

                                    div()
                                        .id(SharedString::from(format!("bot-tile-{}", prof.name)))
                                        .w(px(80.0))
                                        .py(px(8.0))
                                        .px(px(6.0))
                                        .rounded(px(12.0))
                                        .bg(if active {
                                            theme.sidebar_selected
                                        } else {
                                            SandTheme::transparent()
                                        })
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(px(6.0))
                                        .cursor_pointer()
                                        .hover(move |st| st.bg(hover_bg))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if !profile_switching {
                                                this.app.update(cx, |s, cx| {
                                                    s.switch_profile(pname.clone(), cx);
                                                });
                                            }
                                        }))
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            cx.listener(move |this, _, _, cx| {
                                                this.app.update(cx, |s, cx| {
                                                    s.avatar_editor_target = Some(AvatarEditorTarget {
                                                        seed: pname_ctx.clone(),
                                                        title: display_ctx.clone(),
                                                    });
                                                    cx.notify();
                                                });
                                            }),
                                        )
                                        .child(bot_face(
                                            &theme,
                                            &prof.name,
                                            av.and_then(|a| a.shape_id.as_deref()),
                                            av.and_then(|a| a.color_id.as_deref()),
                                            50.0,
                                            tile_state,
                                            tick,
                                        ))
                                        .child(
                                            div()
                                                .max_w_full()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.sidebar_text_primary)
                                                .child(display_name),
                                        )
                                })),
                        )
                        .child(
                            div()
                                .h(px(1.0))
                                .w_full()
                                .my(px(6.0))
                                .bg(theme.sidebar_separator),
                        )
                    })
                    // Pinned sessions
                    .when(!pinned.is_empty(), |d| {
                        d.when(!collapsed, |d| {
                            d.child(
                                div()
                                    .px(px(6.0))
                                    .py(px(5.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.sidebar_text_tertiary)
                                    .child("Pinned")
                                    .child(pinned.len().to_string()),
                            )
                        })
                        .children(pinned.iter().map(|sess| {
                            self.render_session_row(
                                sess,
                                &theme,
                                collapsed,
                                active_view,
                                active_session_id.as_deref(),
                                active_streaming,
                                active_bot_state,
                                &active_board_slug,
                                tick,
                                cx,
                            )
                        }))
                        .when(!collapsed, |d| {
                            d.child(
                                div()
                                    .h(px(1.0))
                                    .w_full()
                                    .my(px(6.0))
                                    .bg(theme.sidebar_separator),
                            )
                        })
                    })
                    // Recent / Archived sessions header
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .px(px(6.0))
                                .py(px(5.0))
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.sidebar_text_tertiary)
                                .child(section_label)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(recent.len().to_string())
                                        .child(
                                            gb_mini_icon_btn("sb-toggle-archived", &theme, false)
                                                .child(cursor_icon(
                                                    "archive",
                                                    12.0,
                                                    if self.show_archived {
                                                        theme.accent_text
                                                    } else {
                                                        theme.sidebar_text_tertiary
                                                    },
                                                ))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.show_archived = !this.show_archived;
                                                    cx.notify();
                                                })),
                                        ),
                                ),
                        )
                    })
                    .children(recent.iter().map(|sess| {
                        self.render_session_row(
                            sess,
                            &theme,
                            collapsed,
                            active_view,
                            active_session_id.as_deref(),
                            active_streaming,
                            active_bot_state,
                            &active_board_slug,
                            tick,
                            cx,
                        )
                    }))
                    .when(recent.is_empty() && !collapsed, |d| {
                        d.child(
                            div()
                                .px(px(10.0))
                                .py(px(14.0))
                                .text_size(px(12.5))
                                .text_color(theme.sidebar_text_tertiary)
                                .child("No sessions here yet. Click + to start a new chat."),
                        )
                    }),
            )
            // Navigation Dock (exact Grok Bot pills, without Work)
            .child(
                div()
                    .w_full()
                    .px(px(if collapsed { 4.0 } else { 12.0 }))
                    .py(px(6.0))
                    .border_t_1()
                    .border_color(theme.sidebar_separator)
                    .flex()
                    .when(collapsed, |d| d.flex_col().items_center())
                    .when(!collapsed, |d| d.flex_wrap())
                    .gap(px(4.0))
                    .flex_shrink_0()
                    .children(NAV_ITEMS.iter().map(|&(view_id, label, icon)| {
                        let active = active_view == view_id;
                        let hover_bg = theme.sidebar_hover;
                        let hover_fg = theme.sidebar_text_primary;
                        let fg = if active {
                            theme.sidebar_text_primary
                        } else {
                            theme.sidebar_text_secondary
                        };
                        div()
                            .id(SharedString::from(format!("nav-{label}")))
                            .h(px(28.0))
                            .px(px(9.0))
                            .rounded(px(8.0))
                            .bg(if active {
                                theme.sidebar_selected
                            } else {
                                SandTheme::transparent()
                            })
                            .text_color(fg)
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .cursor_pointer()
                            .hover(move |st| st.bg(hover_bg).text_color(hover_fg))
                            .child(cursor_icon(icon, 14.0, fg))
                            .when(!collapsed, |d| d.child(label))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.active_view = view_id;
                                    cx.notify();
                                });
                            }))
                    })),
            )
            // Footer: connection status chip + command palette + theme + settings
            .child(
                div()
                    .w_full()
                    .px(px(if collapsed { 6.0 } else { 12.0 }))
                    .py(px(10.0))
                    .border_t_1()
                    .border_color(theme.sidebar_separator)
                    .flex()
                    .when(collapsed, |d| d.flex_col().items_center())
                    .when(!collapsed, |d| d.items_center().justify_between())
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("sb-conn-chip")
                            .flex_1()
                            .min_w_0()
                            .px(px(8.0))
                            .py(px(5.0))
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .hover(|st| st.bg(theme.sidebar_hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.app.update(cx, |s, cx| {
                                    s.active_view = AppViewId::Settings;
                                    cx.notify();
                                });
                            }))
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded_full()
                                    .bg(status_dot_color)
                                    .flex_shrink_0(),
                            )
                            .when(!collapsed, |d| {
                                d.child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(theme.sidebar_text_secondary)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(status_label),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .id("sb-cmd-btn")
                                    .w(px(30.0))
                                    .h(px(30.0))
                                    .rounded(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|st| st.bg(theme.sidebar_hover))
                                    .child(cursor_icon("command", 14.0, theme.sidebar_text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| {
                                            s.command_palette_open = true;
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .id("sb-theme-btn")
                                    .w(px(30.0))
                                    .h(px(30.0))
                                    .rounded(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|st| st.bg(theme.sidebar_hover))
                                    .child(cursor_icon(
                                        if mode == ThemeMode::Dark { "sun" } else { "moon" },
                                        14.0,
                                        theme.sidebar_text_secondary,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| s.toggle_theme(cx));
                                    })),
                            )
                            .child(
                                div()
                                    .id("sb-settings-btn")
                                    .w(px(30.0))
                                    .h(px(30.0))
                                    .rounded(px(8.0))
                                    .bg(if active_view == AppViewId::Settings {
                                        theme.sidebar_selected
                                    } else {
                                        SandTheme::transparent()
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|st| st.bg(theme.sidebar_hover))
                                    .child(cursor_icon("settings-gear", 14.0, theme.sidebar_text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| {
                                            s.active_view = AppViewId::Settings;
                                            cx.notify();
                                        });
                                    })),
                            ),
                    ),
            )
            // Inline Add Bot Modal
            .when(self.new_bot_open, |d| {
                d.child(self.render_new_bot_modal(&theme, window.viewport_size(), cx))
            })
            // Inline New Session Board Modal
            .when(self.new_board_open, |d| {
                d.child(self.render_new_board_modal(&theme, window.viewport_size(), cx))
            })
    }
}

impl SidebarView {
    fn render_new_bot_modal(
        &self,
        theme: &SandTheme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        deferred(
            div()
                .id("new-bot-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .w(viewport.width)
                .h(viewport.height)
                .bg(theme.bg_scrim)
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    if !this.bot_busy {
                        this.new_bot_open = false;
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .id("new-bot-modal")
                        .w(px(440.0))
                        .rounded(px(22.0))
                        .bg(theme.bg_elevated)
                        .border_1()
                        .border_color(theme.border_default)
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .px(px(18.0))
                                .py(px(14.0))
                                .border_b_1()
                                .border_color(theme.border_hairline)
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(15.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Add a bot"),
                                )
                                .child(
                                    gb_mini_icon_btn("close-bot-modal", theme, false)
                                        .child(cursor_icon("close", 14.0, theme.text_secondary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if !this.bot_busy {
                                                this.new_bot_open = false;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(14.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(theme.text_tertiary)
                                        .child("A separate Hermes profile with its own conversations and memory."),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_secondary)
                                                .child("Bot name"),
                                        )
                                        .child(Input::new(&self.bot_name_input).h(px(36.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_secondary)
                                                .child("Description"),
                                        )
                                        .child(Input::new(&self.bot_desc_input).h(px(36.0))),
                                )
                                .when(!self.bot_error.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.danger_text)
                                            .child(self.bot_error.clone()),
                                    )
                                })
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .child(
                                            gb_btn("cancel-bot-btn", theme, BtnVariant::Ghost, false, self.bot_busy)
                                                .child("Cancel")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.new_bot_open = false;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_btn("submit-bot-btn", theme, BtnVariant::Primary, false, self.bot_busy)
                                                .child(if self.bot_busy { "Creating…" } else { "Create bot" })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.create_bot(window, cx);
                                                })),
                                        ),
                                ),
                        ),
                ),
        )
    }

    fn render_new_board_modal(
        &self,
        theme: &SandTheme,
        viewport: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        deferred(
            div()
                .id("new-board-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .w(viewport.width)
                .h(viewport.height)
                .bg(theme.bg_scrim)
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.new_board_open = false;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("new-board-modal")
                        .w(px(460.0))
                        .rounded(px(22.0))
                        .bg(theme.bg_elevated)
                        .border_1()
                        .border_color(theme.border_default)
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .px(px(18.0))
                                .py(px(14.0))
                                .border_b_1()
                                .border_color(theme.border_hairline)
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(15.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("New Session Board"),
                                )
                                .child(
                                    gb_mini_icon_btn("close-board-modal", theme, false)
                                        .child(cursor_icon("close", 14.0, theme.text_secondary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.new_board_open = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(13.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Board Name"))
                                        .child(Input::new(&self.board_name_input).h(px(36.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Description"))
                                        .child(Input::new(&self.board_desc_input).h(px(36.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Workspace Directory (Optional)"))
                                        .child(Input::new(&self.board_path_input).h(px(36.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Accent Color"))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(8.0))
                                                .children(BOARD_COLORS.iter().map(|&c| {
                                                    let preset = resolve_bot_color(c, Some(c));
                                                    let active = self.board_color == c;
                                                    div()
                                                        .id(SharedString::from(format!("bcol-{c}")))
                                                        .w(px(24.0))
                                                        .h(px(24.0))
                                                        .rounded_full()
                                                        .bg(rgb(preset.hex))
                                                        .border_2()
                                                        .border_color(if active {
                                                            theme.text_primary
                                                        } else {
                                                            SandTheme::transparent()
                                                        })
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.board_color = c.into();
                                                            cx.notify();
                                                        }))
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .mt(px(4.0))
                                        .child(
                                            gb_btn("cancel-board-btn", theme, BtnVariant::Ghost, false, false)
                                                .child("Cancel")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.new_board_open = false;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_btn("submit-board-btn", theme, BtnVariant::Primary, false, false)
                                                .child("Create Board")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.create_board(window, cx);
                                                })),
                                        ),
                                ),
                        ),
                ),
        )
    }
}
