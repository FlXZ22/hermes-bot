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
use std::time::Duration;

pub struct HermesShell {
    app: Entity<AppState>,
    focus_handle: FocusHandle,
    last_theme_mode: Option<ThemeMode>,

    welcome: Entity<WelcomeView>,
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    boards: Entity<BoardsView>,
    skills: Entity<SkillsView>,
    cron: Entity<CronView>,
    memory: Entity<MemoryView>,
    gateway: Entity<GatewayView>,
    analytics: Entity<AnalyticsView>,
    settings: Entity<SettingsView>,
    right_pane: Entity<RightSidePaneView>,

    model_picker: Entity<ModelPickerModal>,
    avatar_editor: Entity<AvatarEditorModal>,
    command_palette: Entity<CommandPaletteModal>,

    _anim_task: Task<()>,
}

impl HermesShell {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let welcome = cx.new(|cx| WelcomeView::new(app.clone(), window, cx));
        let sidebar = cx.new(|cx| SidebarView::new(app.clone(), window, cx));
        let chat = cx.new(|cx| ChatView::new(app.clone(), window, cx));
        let boards = cx.new(|cx| BoardsView::new(app.clone(), window, cx));
        let skills = cx.new(|cx| SkillsView::new(app.clone(), window, cx));
        let cron = cx.new(|cx| CronView::new(app.clone(), window, cx));
        let memory = cx.new(|cx| MemoryView::new(app.clone(), window, cx));
        let gateway = cx.new(|cx| GatewayView::new(app.clone(), window, cx));
        let analytics = cx.new(|cx| AnalyticsView::new(app.clone(), window, cx));
        let settings = cx.new(|cx| SettingsView::new(app.clone(), window, cx));
        let right_pane = cx.new(|cx| RightSidePaneView::new(app.clone(), window, cx));

        let model_picker = cx.new(|cx| ModelPickerModal::new(app.clone(), window, cx));
        let avatar_editor = cx.new(|cx| AvatarEditorModal::new(app.clone(), window, cx));
        let command_palette = cx.new(|cx| CommandPaletteModal::new(app.clone(), window, cx));

        // Observe AppState changes and sync gpui-component ThemeMode cleanly
        cx.observe(&app, |this, app_entity, cx| {
            let mode = app_entity.read(cx).theme_mode();
            if this.last_theme_mode != Some(mode) {
                this.last_theme_mode = Some(mode);
                sync_gpui_component_theme(mode, cx);
            }
            cx.notify();
        })
        .detach();

        let initial_mode = app.read(cx).theme_mode();
        sync_gpui_component_theme(initial_mode, cx);

        // Background animation ticker for morphing bot faces & toast auto-dismissal
        let app_anim = app.clone();
        let anim_task = cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(450))
                    .await;
                app_anim.update(cx, |s, cx| {
                    let any_streaming = s.chats.values().any(|c| c.is_streaming);
                    let toast_expired = s
                        .toast
                        .as_ref()
                        .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4));
                    let need_anim =
                        any_streaming || s.avatar_editor_target.is_some() || s.initializing;
                    let mut changed = false;
                    if toast_expired {
                        s.toast = None;
                        changed = true;
                    }
                    if need_anim {
                        s.anim_tick = s.anim_tick.wrapping_add(1);
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                });
            }
        });

        // Kick off saved connection bootstrap
        app.update(cx, |s, cx| {
            s.bootstrap_saved_connection(cx);
        });

        Self {
            app,
            focus_handle,
            last_theme_mode: Some(initial_mode),
            welcome,
            sidebar,
            chat,
            boards,
            skills,
            cron,
            memory,
            gateway,
            analytics,
            settings,
            right_pane,
            model_picker,
            avatar_editor,
            command_palette,
            _anim_task: anim_task,
        }
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        ) = {
            let s = self.app.read(cx);
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
            )
        };

        let center_content: AnyElement = match active_view {
            AppViewId::Chat => self.chat.clone().into_any_element(),
            AppViewId::Boards => self.boards.clone().into_any_element(),
            AppViewId::Skills => self.skills.clone().into_any_element(),
            AppViewId::Cron => self.cron.clone().into_any_element(),
            AppViewId::Memory => self.memory.clone().into_any_element(),
            AppViewId::Gateway => self.gateway.clone().into_any_element(),
            AppViewId::Analytics => self.analytics.clone().into_any_element(),
            AppViewId::Settings => self.settings.clone().into_any_element(),
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
                    .when(right_pane_open, |d| d.child(self.right_pane.clone()))
                    .into_any_element()
            })
            // Global Modals
            .when(model_picker_open, |d| d.child(self.model_picker.clone()))
            .when(avatar_editor_open, |d| d.child(self.avatar_editor.clone()))
            .when(command_palette_open, |d| {
                d.child(self.command_palette.clone())
            })
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
