use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{cursor_icon, FONT_SANS};
use crate::theme::tokens::{SandTheme, ThemeMode};
use crate::ui::motion::ModalMotion;
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;

#[derive(Clone)]
enum PaletteAction {
    NewChat,
    PickModel,
    RestartBackend,
    RefreshModels,
    ToggleTheme,
    OpenView(AppViewId),
    OpenBoard(String),
    SelectSession(String),
}

struct PaletteItem {
    id: String,
    label: String,
    detail: String,
    icon: &'static str,
    action: PaletteAction,
}

pub struct CommandPaletteModal {
    app: Entity<AppState>,
    motion: ModalMotion,
    need_focus: bool,
    query_input: Entity<InputState>,
    selected_idx: usize,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
}

impl CommandPaletteModal {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Jump to a session, board, model, or command...")
        });
        cx.observe(&app, |this, app_e, cx| {
            let open = app_e.read(cx).command_palette_open;
            let was_open = this.motion.is_open();
            this.motion.set_open(open);
            if open {
                if !was_open {
                    this.need_focus = true;
                }
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&query_input, |this, _, ev: &InputEvent, cx| match ev {
            InputEvent::Change => {
                this.selected_idx = 0;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {
                let items = this.build_items(cx);
                if let Some(it) = items.get(this.selected_idx) {
                    this.run_action(it.action.clone(), cx);
                }
            }
            _ => {}
        })
        .detach();

        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());

        Self {
            app,
            motion: ModalMotion::default(),
            need_focus: true,
            query_input,
            selected_idx: 0,
            scroll_handle,
            smooth_scroller,
        }
    }

    fn build_items(&self, cx: &Context<Self>) -> Vec<PaletteItem> {
        let s = self.app.read(cx);
        let is_dark = s.theme_mode() == ThemeMode::Dark;
        let mut items = vec![
            PaletteItem {
                id: "new-chat".into(),
                label: "New Chat Session".into(),
                detail: "Start a fresh Hermes conversation".into(),
                icon: "chat-bubble-plus",
                action: PaletteAction::NewChat,
            },
            PaletteItem {
                id: "pick-model".into(),
                label: "Choose Model & Provider".into(),
                detail: "Ctrl+Shift+M".into(),
                icon: "sparkles",
                action: PaletteAction::PickModel,
            },
            PaletteItem {
                id: "restart-backend".into(),
                label: "Restart Hermes Backend".into(),
                detail: "Reload providers, plugins, and gateway".into(),
                icon: "refresh",
                action: PaletteAction::RestartBackend,
            },
            PaletteItem {
                id: "refresh-models".into(),
                label: "Refresh Models & Providers".into(),
                detail: "Re-fetch live provider catalogs".into(),
                icon: "refresh",
                action: PaletteAction::RefreshModels,
            },
            PaletteItem {
                id: "open-boards".into(),
                label: "Open Session Boards (Kanban)".into(),
                detail: "View all boards & tasks".into(),
                icon: "board-kanban",
                action: PaletteAction::OpenView(AppViewId::Boards),
            },
            PaletteItem {
                id: "toggle-theme".into(),
                label: if is_dark {
                    "Switch to Sand Light Theme".into()
                } else {
                    "Switch to Sand Dark Theme".into()
                },
                detail: "Grok Bot appearance".into(),
                icon: if is_dark { "sun" } else { "moon" },
                action: PaletteAction::ToggleTheme,
            },
            PaletteItem {
                id: "open-skills".into(),
                label: "Skills, Toolsets & MCP Servers".into(),
                detail: "Manage capabilities".into(),
                icon: "plug",
                action: PaletteAction::OpenView(AppViewId::Skills),
            },
            PaletteItem {
                id: "open-cron".into(),
                label: "Cron Jobs & Automations".into(),
                detail: "Scheduled prompts".into(),
                icon: "clock",
                action: PaletteAction::OpenView(AppViewId::Cron),
            },
            PaletteItem {
                id: "open-memory".into(),
                label: "Memory & SOUL.md".into(),
                detail: "Knowledge graph & persona".into(),
                icon: "brain",
                action: PaletteAction::OpenView(AppViewId::Memory),
            },
            PaletteItem {
                id: "open-gateway".into(),
                label: "Gateway, Messaging & Webhooks".into(),
                detail: "Platforms & pairings".into(),
                icon: "globe",
                action: PaletteAction::OpenView(AppViewId::Gateway),
            },
            PaletteItem {
                id: "open-analytics".into(),
                label: "Usage & Analytics".into(),
                detail: "Tokens & model activity".into(),
                icon: "chart-bars",
                action: PaletteAction::OpenView(AppViewId::Analytics),
            },
            PaletteItem {
                id: "open-settings".into(),
                label: "Settings & Connections".into(),
                detail: "API keys, config, logs".into(),
                icon: "settings-gear",
                action: PaletteAction::OpenView(AppViewId::Settings),
            },
        ];

        for b in &s.boards {
            items.push(PaletteItem {
                id: format!("board:{}", b.slug),
                label: format!("Board: {}", b.name),
                detail: "Filter sidebar & open board".into(),
                icon: "board-kanban",
                action: PaletteAction::OpenBoard(b.slug.clone()),
            });
        }
        for sess in s.sessions.iter().take(20) {
            items.push(PaletteItem {
                id: format!("sess:{}", sess.id),
                label: sess.display_title().to_string(),
                detail: sess.model.clone().unwrap_or_else(|| "Session".into()),
                icon: "chat-bubble",
                action: PaletteAction::SelectSession(sess.id.clone()),
            });
        }

        let q = self.query_input.read(cx).value().trim().to_lowercase();
        if q.is_empty() {
            items.truncate(14);
            items
        } else {
            items
                .into_iter()
                .filter(|it| format!("{} {}", it.label, it.detail).to_lowercase().contains(&q))
                .take(14)
                .collect()
        }
    }

    fn run_action(&mut self, action: PaletteAction, cx: &mut Context<Self>) {
        self.app.update(cx, |s, cx| {
            s.command_palette_open = false;
            match action {
                PaletteAction::NewChat => s.create_new_session(None, None, None, None, None, cx),
                PaletteAction::PickModel => s.model_picker_open = true,
                PaletteAction::RestartBackend => s.restart_backend(cx),
                PaletteAction::RefreshModels => s.refresh_models(cx),
                PaletteAction::ToggleTheme => s.toggle_theme(cx),
                PaletteAction::OpenView(v) => s.active_view = v,
                PaletteAction::OpenBoard(slug) => {
                    s.active_board_slug = slug;
                    s.active_view = AppViewId::Boards;
                }
                PaletteAction::SelectSession(sid) => s.select_session(Some(sid), cx),
            }
            cx.notify();
        });
    }
}

impl Render for CommandPaletteModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_open, theme) = {
            let s = self.app.read(cx);
            (s.command_palette_open, s.sand_theme())
        };
        let was_open = self.motion.is_open();
        self.motion.set_open(is_open);
        if !is_open {
            return div().into_any_element();
        }
        if !was_open || self.need_focus {
            self.need_focus = false;
            self.selected_idx = 0;
            self.query_input
                .update(cx, |st, cx| st.set_value("", window, cx));
            let fh = self.query_input.read(cx).focus_handle(cx);
            window.on_next_frame(move |window, cx| {
                window.focus(&fh, cx);
            });
        }
        let items = self.build_items(cx);

        deferred(
            div()
                .id("cmd-palette-backdrop")
                .absolute()
                .inset_0()
                .child(self.motion.backdrop("cmd-palette-scrim-enter", theme.bg_scrim))
                .flex()
                .items_start()
                .justify_center()
                .pt(px(80.0))
                .px(px(20.0))
                .font_family(FONT_SANS)
                .text_color(theme.text_primary)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.app.update(cx, |s, cx| {
                        s.command_palette_open = false;
                        cx.notify();
                    });
                }))
                .child(self.motion.card("cmd-palette-card-enter",
                    div()
                        .id("cmd-palette-modal")
                        .w(px(560.0))
                        .max_w_full()
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
                                .px(px(16.0))
                                .py(px(12.0))
                                .border_b_1()
                                .border_color(theme.border_hairline)
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(cursor_icon("search", 16.0, theme.text_tertiary))
                                .child(
                                    div().flex_1().child(
                                        Input::new(&self.query_input)
                                            .appearance(false)
                                            .bordered(false)
                                            .h(px(30.0)),
                                    ),
                                )
                                .child(
                                    div()
                                        .px(px(7.0))
                                        .py(px(3.0))
                                        .rounded(px(7.0))
                                        .bg(theme.bg_secondary)
                                        .border_1()
                                        .border_color(theme.border_subtle)
                                        .text_size(px(11.0))
                                        .text_color(theme.text_secondary)
                                        .child("Esc"),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .max_h(px(360.0))
                                .overflow_hidden()
                                .relative()
                                .child(
                                    div()
                                        .id("cmd-palette-list")
                                        .track_scroll(&self.scroll_handle)
                                        .p(px(8.0))
                                        .pb(px(12.0))
                                        .max_h(px(360.0))
                                        .overflow_y_scroll()
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .children(items.into_iter().enumerate().map(|(idx, item)| {
                                            let active = idx == self.selected_idx;
                                            let act = item.action.clone();
                                            div()
                                                .id(SharedString::from(format!("cmd-item-{}", item.id)))
                                                .w_full()
                                                .px(px(9.0))
                                                .py(px(7.0))
                                                .rounded(px(9.0))
                                                .bg(if active {
                                                    theme.bg_active
                                                } else {
                                                    SandTheme::transparent()
                                                })
                                                .flex()
                                                .items_center()
                                                .gap(px(9.0))
                                                .cursor_pointer()
                                                .hover(|st| st.bg(theme.bg_active))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.run_action(act.clone(), cx);
                                                }))
                                                .child(cursor_icon(item.icon, 15.0, theme.text_secondary))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .text_ellipsis()
                                                        .whitespace_nowrap()
                                                        .text_size(px(13.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(item.label),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(theme.text_tertiary)
                                                        .child(item.detail),
                                                )
                                        })),
                                )
                                .vertical_scrollbar(&self.scroll_handle)
                                .child(smooth_scroll_layer(&self.smooth_scroller)),
                        ),
                )),
        )
        .into_any_element()
    }
}
