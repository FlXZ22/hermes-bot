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
    visible_items: Vec<PaletteItem>,
    scroll_handle: UniformListScrollHandle,
    _keyboard: Subscription,
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
                this.visible_items.clear();
                this.smooth_scroller.cancel();
                this.scroll_handle
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(Point::default());
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {
                if let Some(action) = this
                    .visible_items
                    .get(this.selected_idx)
                    .map(|item| item.action.clone())
                {
                    this.run_action(action, cx);
                }
            }
            _ => {}
        })
        .detach();

        let palette = cx.weak_entity();
        let query_focus = query_input.read(cx).focus_handle(cx);
        let keyboard = cx.intercept_keystrokes(move |event, window, cx| {
            let key = &event.keystroke;
            if key.modifiers.control
                || key.modifiers.platform
                || key.modifiers.alt
                || key.modifiers.shift
                || !query_focus.is_focused(window)
            {
                return;
            }
            let direction = match key.key.as_str() {
                "up" => -1,
                "down" => 1,
                _ => return,
            };
            let _ = palette.update(cx, |this, cx| {
                if this.app.read(cx).command_palette_open {
                    this.move_selection(direction, cx);
                    cx.stop_propagation();
                }
            });
        });
        let scroll_handle = UniformListScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_uniform_list(scroll_handle.clone());

        Self {
            app,
            motion: ModalMotion::default(),
            need_focus: true,
            query_input,
            selected_idx: 0,
            visible_items: Vec::new(),
            scroll_handle,
            _keyboard: keyboard,
            smooth_scroller,
        }
    }

    fn move_selection(&mut self, direction: isize, cx: &mut Context<Self>) {
        let len = self.visible_items.len();
        if len == 0 {
            return;
        }
        self.selected_idx =
            (self.selected_idx as isize + direction).rem_euclid(len as isize) as usize;
        self.smooth_scroller.cancel();
        self.scroll_handle
            .scroll_to_item(self.selected_idx, ScrollStrategy::Nearest);
        cx.notify();
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
        for sess in &s.sessions {
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
            return items;
        }
        items.retain(|item| {
            let label = item.label.to_lowercase();
            let detail = item.detail.to_lowercase();
            q.split_whitespace()
                .all(|word| label.contains(word) || detail.contains(word))
        });
        items
    }

    fn run_action(&mut self, action: PaletteAction, cx: &mut Context<Self>) {
        if !self.app.read(cx).command_palette_open {
            return;
        }
        self.app.update(cx, |s, cx| {
            s.command_palette_open = false;
            s.bump_shell();
            match action {
                PaletteAction::NewChat => s.create_new_session(None, None, None, None, None, cx),
                PaletteAction::PickModel => s.model_picker_open = true,
                PaletteAction::RestartBackend => s.restart_backend(cx),
                PaletteAction::RefreshModels => s.refresh_models(cx),
                PaletteAction::ToggleTheme => s.toggle_theme(cx),
                PaletteAction::OpenView(v) => s.set_active_view(v, cx),
                PaletteAction::OpenBoard(slug) => {
                    s.active_board_slug = slug;
                    s.bump_sidebar();
                    s.set_active_view(AppViewId::Boards, cx);
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
            self.smooth_scroller.cancel();
            self.scroll_handle
                .0
                .borrow()
                .base_handle
                .set_offset(Point::default());
            self.query_input
                .update(cx, |st, cx| st.set_value("", window, cx));
            let fh = self.query_input.read(cx).focus_handle(cx);
            window.on_next_frame(move |window, cx| {
                if fh.is_rendered(window) {
                    window.focus(&fh, cx);
                }
            });
        }
        self.visible_items = self.build_items(cx);
        let count = self.visible_items.len();
        self.selected_idx = self.selected_idx.min(count.saturating_sub(1));
        let top = (window.viewport_size().height * 0.12).min(px(80.0));
        let max_height = (window.viewport_size().height - top - px(84.0)).clamp(px(0.0), px(360.0));
        let list_height = px(count as f32 * 44.0).min(max_height);

        deferred(
            div()
                .id("cmd-palette-backdrop")
                .occlude()
                .absolute()
                .inset_0()
                .child(self.motion.backdrop("cmd-palette-scrim-enter", theme.bg_scrim))
                .flex()
                .items_start()
                .justify_center()
                .pt(top)
                .px(px(20.0))
                .font_family(FONT_SANS)
                .text_color(theme.text_primary)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.app.update(cx, |s, cx| {
                        s.command_palette_open = false;
                        s.bump_shell();
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
                        .child(if count == 0 {
                            div()
                                .p(px(16.0))
                                .text_color(theme.text_secondary)
                                .child("No matching commands, boards, or sessions.")
                                .into_any_element()
                        } else {
                            div()
                                .w_full()
                                .h(list_height)
                                .min_h_0()
                                .relative()
                                .child(
                                    uniform_list("cmd-palette-list", count, cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                        range.filter_map(|index| {
                                            let item = this.visible_items.get(index)?;
                                            let active = index == this.selected_idx;
                                            let action = item.action.clone();
                                            Some(div()
                                                .id(SharedString::from(format!("cmd-item-{}", item.id)))
                                                .w_full()
                                                .h(px(44.0))
                                                .px(px(14.0))
                                                .rounded(px(9.0))
                                                .bg(if active { theme.bg_active } else { SandTheme::transparent() })
                                                .flex()
                                                .items_center()
                                                .gap(px(9.0))
                                                .cursor_pointer()
                                                .hover(|style| style.bg(theme.bg_active))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.run_action(action.clone(), cx);
                                                }))
                                                .child(cursor_icon(item.icon, 15.0, theme.text_secondary))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .flex()
                                                        .flex_col()
                                                        .child(div().truncate().text_size(px(13.0)).font_weight(FontWeight::MEDIUM).child(item.label.clone()))
                                                        .child(div().truncate().text_size(px(11.0)).text_color(theme.text_tertiary).child(item.detail.clone())),
                                                ))
                                        }).collect::<Vec<_>>()
                                    }))
                                    .size_full()
                                    .track_scroll(&self.scroll_handle),
                                )
                                .vertical_scrollbar(&self.scroll_handle)
                                .child(smooth_scroll_layer(&self.smooth_scroller))
                                .into_any_element()
                        }),
                )),
        )
        .into_any_element()
    }
}
