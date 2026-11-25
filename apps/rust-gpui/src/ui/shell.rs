use crate::face::FaceLayer;
use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{cursor_icon, FONT_SANS};
use crate::theme::tokens::ThemeMode;
use crate::ui::chat::ChatView;
use crate::ui::inspector::right_pane::RightSidePaneView;
use crate::ui::modals::avatar_editor::AvatarEditorModal;
use crate::ui::modals::command_palette::CommandPaletteModal;
use crate::ui::modals::model_picker::ModelPickerModal;
use crate::ui::motion::{toast_enter, view_enter, EntranceKey, SpringValue};
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

    // Always-hot primary views
    welcome: Entity<WelcomeView>,
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    sidebar_faces: FaceLayer,
    chat_faces: FaceLayer,

    // Lazily constructed on first visit so hidden views & PTYs cost 0 CPU & 0 RAM at startup
    boards: Option<Entity<BoardsView>>,
    skills: Option<Entity<SkillsView>>,
    cron: Option<Entity<CronView>>,
    memory: Option<Entity<MemoryView>>,
    gateway: Option<Entity<GatewayView>>,
    analytics: Option<Entity<AnalyticsView>>,
    settings: Option<Entity<SettingsView>>,
    right_pane: Option<Entity<RightSidePaneView>>,
    right_pane_progress: SpringValue,
    view_entrance: EntranceKey<Option<AppViewId>>,

    model_picker: Option<Entity<ModelPickerModal>>,
    avatar_editor: Option<Entity<AvatarEditorModal>>,
    command_palette: Option<Entity<CommandPaletteModal>>,
    open_modals: (bool, bool, bool),

    toast_task: Option<(Instant, Task<()>)>,
    toast_entrance: EntranceKey<Instant>,
}

impl HermesShell {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let welcome = cx.new(|cx| WelcomeView::new(app.clone(), window, cx));
        let sidebar = cx.new(|cx| SidebarView::new(app.clone(), window, cx));
        let chat = cx.new(|cx| ChatView::new(app.clone(), window, cx));
        window.focus(&focus_handle, cx);

        // Observe AppState changes, sync gpui-component ThemeMode, and manage toast auto-dismiss
        cx.observe(&app, |this, app_entity, cx| {
            let (mode, toast_at) = {
                let s = app_entity.read(cx);
                (s.theme_mode(), s.toast.as_ref().map(|(_, at)| *at))
            };
            if this.last_theme_mode != Some(mode) {
                this.last_theme_mode = Some(mode);
                sync_gpui_component_theme(mode, cx);
            }
            if let Some(at) = toast_at.filter(|at| this.toast_task.as_ref().map(|(t, _)| t) != Some(at)) {
                let app_toast = app_entity.clone();
                let delay = Duration::from_secs(3).saturating_sub(at.elapsed());
                this.toast_task = Some((at, cx.spawn(async move |shell, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = shell.update(cx, |v, cx| {
                        app_toast.update(cx, |s, cx| {
                            if s.toast.as_ref().is_some_and(|(_, current)| *current == at) {
                                s.toast = None;
                                cx.notify();
                            }
                        });
                        v.toast_task = None;
                    });
                })));
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
            welcome,
            sidebar,
            chat,
            sidebar_faces: FaceLayer::default(),
            chat_faces: FaceLayer::default(),
            boards: None,
            skills: None,
            cron: None,
            memory: None,
            gateway: None,
            analytics: None,
            settings: None,
            right_pane: None,
            right_pane_progress: SpringValue::new(0.0),
            view_entrance: EntranceKey::new(),
            model_picker: None,
            avatar_editor: None,
            command_palette: None,
            open_modals: (false, false, false),
            toast_task: None,
            toast_entrance: EntranceKey::new(),
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
                if s.command_palette_open {
                    s.model_picker_open = false;
                    s.avatar_editor_target = None;
                }
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
        if primary && matches!(ks.key.as_str(), "\\" | "backslash") {
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
                if s.model_picker_open {
                    s.command_palette_open = false;
                    s.avatar_editor_target = None;
                }
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
                 s.toast.clone(),
            )
        };

        let modals = (model_picker_open, avatar_editor_open, command_palette_open);
        let previous_modals = std::mem::replace(&mut self.open_modals, modals);
        if (previous_modals.0 && !modals.0)
            || (previous_modals.1 && !modals.1)
            || (previous_modals.2 && !modals.2)
        {
            let focus_handle = self.focus_handle.clone();
            // After Escape or backdrop dismissal the modal's input may still own
            // a handle, but won't be part of the newly rendered dispatch tree.
            window.on_next_frame(move |window, cx| {
                if window.focused(cx).is_none_or(|handle| !handle.is_rendered(window)) {
                    window.focus(&focus_handle, cx);
                }
            });
        }

        let app = self.app.clone();
        let full = Some(relative(1.).into());
        let center_content: AnyElement = match active_view {
            AppViewId::Chat => self.chat_faces.wrap(self.chat.clone().cached(StyleRefinement {
                size: SizeRefinement { width: full, height: full },
                min_size: SizeRefinement { width: Some(px(0.).into()), height: Some(px(0.).into()) },
                overflow: PointRefinement { x: Some(Overflow::Hidden), y: Some(Overflow::Hidden) },
                ..Default::default()
            })).into_any_element(),
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

        let show_workspace = !setup_open && !initializing;
        let sidebar = show_workspace.then(|| {
            let width = self.sidebar.update(cx, |sidebar, cx| sidebar.measure_width(window, cx));
            self.sidebar_faces.wrap(self.sidebar.clone().cached(StyleRefinement {
                size: SizeRefinement { width: Some(px(width).into()), height: full },
                flex_shrink: Some(0.),
                ..Default::default()
            }))
        });
        let view_id = self.view_entrance.id("view", show_workspace.then_some(active_view));
        let pane_progress = if show_workspace {
            self.right_pane_progress.set_target(if right_pane_open { 1.0 } else { 0.0 });
            self.right_pane_progress.advance(window)
        } else {
            self.right_pane_progress.snap(0.0);
            0.0
        };
        // Keep the pane mounted through the last visible frame of the closing slide.
        let right_pane_el = if show_workspace && (right_pane_open || pane_progress > 0.0) {
            let pane = self.right_pane.get_or_insert_with(|| {
                cx.new(|cx| RightSidePaneView::new(app.clone(), window, cx))
            });
            if pane.read(cx).open_progress() != pane_progress {
                pane.update(cx, |pane, cx| pane.set_open_progress(pane_progress, cx));
            }
            Some(pane.clone())
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
            .child(if !show_workspace {
                self.welcome.clone().into_any_element()
            } else {
                div()
                    .size_full()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .children(sidebar)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(view_enter(
                                view_id,
                                div().size_full().flex().flex_col().child(center_content),
                            )),
                    )
                    .when_some(right_pane_el, |d, rp| d.child(rp))
                    .into_any_element()
            })
            // Global Modals (lazily initialized on first open)
            .when_some(model_picker_el, |d, m| d.child(m))
            .when_some(avatar_editor_el, |d, m| d.child(m))
            .when_some(command_palette_el, |d, m| d.child(m))
            // Floating bottom-right Toast notification
            .when_some(toast_msg, |d, (msg, at)| {
                d.child(toast_enter(
                    self.toast_entrance.id("toast-enter", at),
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
                ))
            })
    }
}
