use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{cursor_icon, FONT_SANS};
use crate::theme::tokens::ThemeMode;
use crate::ui::chat::ChatView;
use crate::ui::inspector::right_pane::RightSidePaneView;
use crate::ui::modals::avatar_editor::AvatarEditorModal;
use crate::ui::modals::command_palette::CommandPaletteModal;
use crate::ui::modals::model_picker::ModelPickerModal;
use crate::ui::sidebar::SidebarView;
use crate::ui::views::analytics::AnalyticsView;
use crate::ui::views::boards::BoardsView;
use crate::ui::views::cron::CronView;
use crate::ui::views::gateway::GatewayView;
use crate::ui::views::memory::MemoryView;
use crate::ui::views::settings::SettingsView;
use crate::ui::views::skills::SkillsView;
use crate::ui::welcome::WelcomeView;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::time::{Duration, Instant};

pub struct HermesShell {
    app: Entity<AppState>,
    focus_handle: FocusHandle,
    last_theme_mode: Option<ThemeMode>,
    anim_frame_scheduled: bool,
    last_anim_tick: Instant,

    // Always-hot primary views
    welcome: Entity<WelcomeView>,
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,

    // Lazily constructed on first visit so hidden views & PTYs cost 0 CPU & 0 RAM at startup
    boards: Option<Entity<BoardsView>>,
    skills: Option<Entity<SkillsView>>,
    cron: Option<Entity<CronView>>,
    memory: Option<Entity<MemoryView>>,
    gateway: Option<Entity<GatewayView>>,
    analytics: Option<Entity<AnalyticsView>>,
    settings: Option<Entity<SettingsView>>,
    right_pane: Option<Entity<RightSidePaneView>>,

    model_picker: Option<Entity<ModelPickerModal>>,
    avatar_editor: Option<Entity<AvatarEditorModal>>,
    command_palette: Option<Entity<CommandPaletteModal>>,

    _toast_task: Option<Task<()>>,
}

impl HermesShell {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let welcome = cx.new(|cx| WelcomeView::new(app.clone(), window, cx));
        let sidebar = cx.new(|cx| SidebarView::new(app.clone(), window, cx));
        let chat = cx.new(|cx| ChatView::new(app.clone(), window, cx));

        // Observe AppState changes, sync gpui-component ThemeMode, and manage toast auto-dismiss
        cx.observe(&app, |this, app_entity, cx| {
            let (mode, has_toast) = {
                let s = app_entity.read(cx);
                (s.theme_mode(), s.toast.is_some())
            };
            if this.last_theme_mode != Some(mode) {
                this.last_theme_mode = Some(mode);
                sync_gpui_component_theme(mode, cx);
            }
            if has_toast && this._toast_task.is_none() {
                let app_toast = app_entity.clone();
                this._toast_task = Some(cx.spawn(async move |shell, cx| {
                    cx.background_executor().timer(Duration::from_secs(4)).await;
                    let _ = shell.update(cx, |v, cx| {
                        v._toast_task = None;
                        app_toast.update(cx, |s, cx| {
                            if s.toast
                                .as_ref()
                                .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(3))
                            {
                                s.toast = None;
                                cx.notify();
                            }
                        });
                    });
                }));
            }
            cx.notify();
        })
        .detach();

        let initial_mode = app.read(cx).theme_mode();
        sync_gpui_component_theme(initial_mode, cx);

        // Kick off saved connection bootstrap
        app.update(cx, |s, cx| {
            s.bootstrap_saved_connection(cx);
        });

        Self {
            app,
            focus_handle,
            last_theme_mode: Some(initial_mode),
            anim_frame_scheduled: false,
            last_anim_tick: Instant::now(),
            welcome,
            sidebar,
            chat,
            boards: None,
            skills: None,
            cron: None,
            memory: None,
            gateway: None,
            analytics: None,
            settings: None,
            right_pane: None,
            model_picker: None,
            avatar_editor: None,
            command_palette: None,
            _toast_task: None,
        }
    }

    fn schedule_anim_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.anim_frame_scheduled {
            return;
        }
        self.anim_frame_scheduled = true;
        let weak_self = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = weak_self.update(cx, |this, cx| {
                this.anim_frame_scheduled = false;
                let need_anim = {
                    let s = this.app.read(cx);
                    s.chats.values().any(|c| c.is_streaming)
                        || s.avatar_editor_target.is_some()
                        || s.initializing
                };
                if !need_anim {
                    return;
                }
                // Advance morph frame smoothly at compositor VSync cadence (~12Hz SVG frame swap,
                // keeping full VSync smoothness without excessive state churn)
                if this.last_anim_tick.elapsed() >= Duration::from_millis(220) {
                    this.last_anim_tick = Instant::now();
                    this.app.update(cx, |s, cx| {
                        s.anim_tick = s.anim_tick.wrapping_add(1);
                        cx.notify();
                    });
                }
                this.schedule_anim_frame(window, cx);
            });
        });
    }

    fn handle_global_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        let primary = ks.modifiers.control || ks.modifiers.platform;
        if primary && ks.key.eq_ignore_ascii_case("k") {
            self.app.update(cx, |s, cx| {
                s.command_palette_open = !s.command_palette_open;
                cx.notify();
            });
            cx.stop_propagation();
            return;
        }
        if primary && ks.key.eq_ignore_ascii_case("n") {
            self.app.update(cx, |s, cx| {
                s.create_new_session(None, None, None, None, None, cx);
            });
            cx.stop_propagation();
            return;
        }
        if primary && ks.key.eq_ignore_ascii_case("b") {
            self.app.update(cx, |s, cx| {
                s.sidebar_collapsed = !s.sidebar_collapsed;
                cx.notify();
            });
            cx.stop_propagation();
            return;
        }
        if primary && ks.key == "\\" {
            self.app.update(cx, |s, cx| {
                s.right_pane_open = !s.right_pane_open;
                cx.notify();
            });
            cx.stop_propagation();
            return;
        }
        if primary && ks.modifiers.shift && ks.key.eq_ignore_ascii_case("m") {
            self.app.update(cx, |s, cx| {
                s.model_picker_open = !s.model_picker_open;
                cx.notify();
            });
            cx.stop_propagation();
            return;
        }
        if ks.key == "escape" {
            let mut closed = false;
            self.app.update(cx, |s, cx| {
                if s.command_palette_open {
                    s.command_palette_open = false;
                    closed = true;
                } else if s.model_picker_open {
                    s.model_picker_open = false;
                    closed = true;
                } else if s.avatar_editor_target.is_some() {
                    s.avatar_editor_target = None;
                    closed = true;
                }
                if closed {
                    cx.notify();
                }
            });
            if closed {
                cx.stop_propagation();
            }
        }
    }
}

fn sync_gpui_component_theme(mode: ThemeMode, cx: &mut App) {
    let target = match mode {
        ThemeMode::Light => gpui_component::ThemeMode::Light,
        ThemeMode::Dark => gpui_component::ThemeMode::Dark,
    };
    gpui_component::Theme::change(target, None, cx);
}

impl Render for HermesShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            theme,
            setup_open,
            initializing,
            active_view,
            right_pane_open,
            model_picker_open,
            avatar_editor_open,
            command_palette_open,
            toast_msg,
            need_anim,
        ) = {
            let s = self.app.read(cx);
            let need_anim = s.chats.values().any(|c| c.is_streaming)
                || s.avatar_editor_target.is_some()
                || s.initializing;
            (
                s.sand_theme(),
                s.setup_open,
                s.initializing,
                s.active_view,
                s.right_pane_open,
                s.model_picker_open,
                s.avatar_editor_target.is_some(),
                s.command_palette_open,
                s.toast.as_ref().map(|(m, _)| m.clone()),
                need_anim,
            )
        };

        if need_anim {
            self.schedule_anim_frame(window, cx);
        }

        let app = self.app.clone();
        let center_content: AnyElement = match active_view {
            AppViewId::Chat => self.chat.clone().into_any_element(),
            AppViewId::Boards => self
                .boards
                .get_or_insert_with(|| cx.new(|cx| BoardsView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
            AppViewId::Skills => self
                .skills
                .get_or_insert_with(|| cx.new(|cx| SkillsView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
            AppViewId::Cron => self
                .cron
                .get_or_insert_with(|| cx.new(|cx| CronView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
            AppViewId::Memory => self
                .memory
                .get_or_insert_with(|| cx.new(|cx| MemoryView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
            AppViewId::Gateway => self
                .gateway
                .get_or_insert_with(|| cx.new(|cx| GatewayView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
            AppViewId::Analytics => self
                .analytics
                .get_or_insert_with(|| cx.new(|cx| AnalyticsView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
            AppViewId::Settings => self
                .settings
                .get_or_insert_with(|| cx.new(|cx| SettingsView::new(app.clone(), window, cx)))
                .clone()
                .into_any_element(),
        };

        let right_pane_el = if right_pane_open {
            Some(
                self.right_pane
                    .get_or_insert_with(|| {
                        cx.new(|cx| RightSidePaneView::new(app.clone(), window, cx))
                    })
                    .clone(),
            )
        } else {
            None
        };

        let model_picker_el = if model_picker_open {
            Some(
                self.model_picker
                    .get_or_insert_with(|| {
                        cx.new(|cx| ModelPickerModal::new(app.clone(), window, cx))
                    })
                    .clone(),
            )
        } else {
            None
        };

        let avatar_editor_el = if avatar_editor_open {
            Some(
                self.avatar_editor
                    .get_or_insert_with(|| {
                        cx.new(|cx| AvatarEditorModal::new(app.clone(), window, cx))
                    })
                    .clone(),
            )
        } else {
            None
        };

        let command_palette_el = if command_palette_open {
            Some(
                self.command_palette
                    .get_or_insert_with(|| {
                        cx.new(|cx| CommandPaletteModal::new(app.clone(), window, cx))
                    })
                    .clone(),
            )
        } else {
            None
        };

        div()
            .id("hermes-shell-root")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.bg_pane)
            .text_color(theme.text_primary)
            .font_family(FONT_SANS)
            .relative()
            .overflow_hidden()
            .on_key_down(cx.listener(|v, ev, window, cx| {
                v.handle_global_key(ev, window, cx);
            }))
            // Main 3-column workspace or Welcome Onboarding
            .child(if setup_open || initializing {
                self.welcome.clone().into_any_element()
            } else {
                div()
                    .size_full()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(self.sidebar.clone())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(center_content),
                    )
                    .when_some(right_pane_el, |d, rp| d.child(rp))
                    .into_any_element()
            })
            // Global Modals (lazily initialized on first open)
            .when_some(model_picker_el, |d, m| d.child(m))
            .when_some(avatar_editor_el, |d, m| d.child(m))
            .when_some(command_palette_el, |d, m| d.child(m))
            // Floating bottom-right Toast notification
            .when_some(toast_msg, |d, msg| {
                d.child(
                    div()
                        .id("gb-toast")
                        .absolute()
                        .bottom(px(20.0))
                        .right(px(20.0))
                        .max_w(px(380.0))
                        .px(px(14.0))
                        .py(px(9.0))
                        .rounded_full()
                        .bg(theme.bg_primary)
                        .text_color(theme.text_on_primary)
                        .border_1()
                        .border_color(theme.border_default)
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.app.update(cx, |s, cx| {
                                s.toast = None;
                                cx.notify();
                            });
                        }))
                        .child(cursor_icon("info", 13.5, theme.text_on_primary))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(msg),
                        ),
                )
            })
    }
}
