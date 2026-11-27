use crate::harness::events::WorkProject;
use crate::harness::projects::{
    browse_server_directory, home_scope_notice, validate_server_directory, ServerBrowseResult,
};
use crate::state::app_state::AppState;
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_mini_icon_btn, gb_row, gb_section_card, gb_section_card_head,
    BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::SandTheme;
use crate::ui::motion::{gpu_spinner, ModalMotion};
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;

#[derive(Clone, PartialEq, Eq)]
struct ServerKey {
    id: String,
    url: String,
}

impl ServerKey {
    fn from_app(app: &AppState) -> Self {
        Self {
            id: app.connection.id.clone(),
            url: app.connection.url.clone(),
        }
    }

    fn matches(&self, app: &AppState) -> bool {
        self.id == app.connection.id && self.url == app.connection.url
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DirectoryOperation {
    Browsing,
    Validating,
}

pub struct ProjectPickerModal {
    app: Entity<AppState>,
    motion: ModalMotion,
    focus: FocusHandle,
    server: Option<ServerKey>,
    name_input: Entity<InputState>,
    path_input: Entity<InputState>,
    directory: Option<ServerBrowseResult>,
    error: Option<String>,
    request: Option<(DirectoryOperation, tokio::task::AbortHandle)>,
    generation: u64,
    scroll: ScrollHandle,
    smooth: SmoothScroller,
}

impl ProjectPickerModal {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Project name (optional)"));
        let path_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Enter a directory on the server"));

        cx.observe(&app, |this, app, cx| {
            let app = app.read(cx);
            if app.project_picker_open {
                cx.notify();
            } else {
                this.cancel_request();
                this.motion.set_open(false);
                this.server = None;
            }
        })
        .detach();

        cx.subscribe_in(
            &path_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.go(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();

        let scroll = ScrollHandle::new();
        let smooth = SmoothScroller::for_handle(scroll.clone());
        Self {
            app,
            motion: ModalMotion::default(),
            focus: cx.focus_handle(),
            server: None,
            name_input,
            path_input,
            directory: None,
            error: None,
            request: None,
            generation: 0,
            scroll,
            smooth,
        }
    }

    fn cancel_request(&mut self) {
        if let Some((_, request)) = self.request.take() {
            request.abort();
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.cancel_request();
        self.app.update(cx, |app, cx| app.close_project_picker(cx));
    }

    fn request_is_current(&self, generation: u64, server: &ServerKey, cx: &App) -> bool {
        let app = self.app.read(cx);
        self.generation == generation && app.project_picker_open && server.matches(app)
    }

    fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.request.is_some() {
            return;
        }
        let path = self.path_input.read(cx).value().trim().to_owned();
        if path.is_empty() {
            self.error = Some("Enter a directory path.".into());
            cx.notify();
        } else {
            self.browse(Some(path), false, window, cx);
        }
    }

    fn use_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = self
            .directory
            .as_ref()
            .and_then(|directory| directory.home_path.clone())
            .unwrap_or_else(|| "~".into());
        self.browse(Some(home), true, window, cx);
    }

    fn browse(
        &mut self,
        path: Option<String>,
        require_home: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app = self.app.read(cx);
        if self.request.is_some() || !app.project_picker_open {
            return;
        }
        let server = ServerKey::from_app(app);
        let connection = app.connection.clone();
        let client = app.client.clone();
        // A disconnected remote must not fall back to browsing this computer.
        if !connection.is_local() && client.is_none() && connection.ssh.is_none() {
            self.error = Some("Connect to this server to browse its directories.".into());
            cx.notify();
            return;
        }
        let worker = app
            .tokio_rt
            .spawn(async move { browse_server_directory(client, connection, path).await });
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.request = Some((DirectoryOperation::Browsing, worker.abort_handle()));
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = worker
                .await
                .unwrap_or_else(|error| Err(format!("Directory browser failed: {error}")));
            let _ = this.update_in(cx, |view, window, cx| {
                if !view.request_is_current(generation, &server, cx) {
                    return;
                }
                view.request = None;
                match result {
                    Ok(directory) if require_home && !directory.is_home_dir => {
                        view.error = Some("The server did not identify a home directory.".into());
                    }
                    Ok(directory) if directory.server_id == server.id => {
                        view.path_input.update(cx, |input, cx| {
                            input.set_value(directory.canonical_path.clone(), window, cx);
                        });
                        view.directory = Some(directory);
                    }
                    Ok(_) => {
                        view.error =
                            Some("The directory response belonged to another server.".into());
                    }
                    Err(error) => {
                        view.error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        let (Some(server), Some(directory)) = (self.server.clone(), self.directory.as_ref()) else {
            return;
        };
        if self.request.is_some()
            || !app.project_picker_open
            || !server.matches(app)
            || self.path_input.read(cx).value().trim() != directory.canonical_path
        {
            return;
        }
        let connection = app.connection.clone();
        let client = app.client.clone();
        if !connection.is_local() && client.is_none() && connection.ssh.is_none() {
            self.error = Some("Connect to this server to validate the directory.".into());
            cx.notify();
            return;
        }
        let path = directory.canonical_path.clone();
        let name = self.name_input.read(cx).value().trim().to_owned();
        let worker = app
            .tokio_rt
            .spawn(async move { validate_server_directory(client, connection, &path).await });
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.request = Some((DirectoryOperation::Validating, worker.abort_handle()));
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = worker
                .await
                .unwrap_or_else(|error| Err(format!("Directory validation failed: {error}")));
            let _ = this.update(cx, |view, cx| {
                if !view.request_is_current(generation, &server, cx) {
                    return;
                }
                view.request = None;
                match result {
                    Ok(validated) => view.app.update(cx, |app, cx| {
                        app.create_and_select_work_project(
                            name,
                            validated.canonical_path,
                            validated.is_home_dir,
                            cx,
                        );
                    }),
                    Err(error) => view.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn saved_projects(
        &self,
        projects: &[WorkProject],
        server: &ServerKey,
        selected: Option<&str>,
        theme: &SandTheme,
        cx: &Context<Self>,
    ) -> Div {
        let saved = projects
            .iter()
            .filter(|project| project.server_id == server.id);
        gb_section_card(theme)
            .child(
                gb_section_card_head(theme).child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Saved projects"),
                ),
            )
            .when(saved.clone().next().is_none(), |card| {
                card.child(
                    div()
                        .p(px(14.))
                        .text_color(theme.text_secondary)
                        .child("No projects saved for this server."),
                )
            })
            .children(saved.map(|project| {
                let id = project.id.clone();
                let active = selected == Some(id.as_str());
                let hover = theme.bg_hover;
                gb_row(theme)
                    .id(SharedString::from(format!("project-picker-saved-{id}")))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let app = view.app.read(cx);
                        if app.project_picker_open
                            && app.work_projects.iter().any(|project| {
                                project.id == id && project.server_id == app.connection.id
                            })
                        {
                            view.cancel_request();
                            view.app
                                .update(cx, |app, cx| app.select_work_project(id.clone(), cx));
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(project.name.clone()),
                            )
                            .child(path_label(&project.cwd, theme)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap(px(6.))
                            .when(project.is_home, |row| {
                                row.child(gb_badge(theme, BadgeTone::Warning, "Home"))
                            })
                            .when(active, |row| {
                                row.child(gb_badge(theme, BadgeTone::Accent, "Active"))
                            }),
                    )
            }))
    }

    fn browser_panel(&self, theme: &SandTheme, cx: &Context<Self>) -> Div {
        let busy = self.request.is_some();
        let path_value = self.path_input.read(cx).value();
        let directory = self.directory.as_ref();
        let at_selected_path =
            directory.is_some_and(|directory| path_value.trim() == directory.canonical_path);
        let parent = directory.and_then(|directory| directory.parent_path.clone());
        gb_section_card(theme)
            .child(
                gb_section_card_head(theme).child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Create new project"),
                ),
            )
            .child(
                div()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(Input::new(&self.name_input).disabled(busy))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Input::new(&self.path_input).disabled(busy)),
                            )
                            .child(
                                gb_btn(
                                    "project-picker-go",
                                    theme,
                                    BtnVariant::Secondary,
                                    true,
                                    busy,
                                )
                                .child("Go")
                                .on_click(cx.listener(|view, _, window, cx| view.go(window, cx))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(
                                gb_btn(
                                    "project-picker-up",
                                    theme,
                                    BtnVariant::Ghost,
                                    true,
                                    busy || parent.is_none(),
                                )
                                .child(cursor_icon("arrow-up", 12., theme.text_secondary))
                                .child("Up")
                                .on_click(cx.listener(
                                    move |view, _, window, cx| {
                                        if let Some(path) = parent.clone() {
                                            view.browse(Some(path), false, window, cx);
                                        }
                                    },
                                )),
                            )
                            .child(
                                gb_btn(
                                    "project-picker-home",
                                    theme,
                                    BtnVariant::Secondary,
                                    true,
                                    busy,
                                )
                                .child("Use Home (~)")
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.use_home(window, cx)),
                                ),
                            ),
                    )
                    .when_some(self.error.as_ref(), |panel, error| {
                        panel.child(
                            div()
                                .rounded(px(8.))
                                .p(px(10.))
                                .bg(theme.danger_soft)
                                .text_color(theme.danger_text)
                                .child(error.clone()),
                        )
                    })
                    .when_some(directory, |panel, directory| {
                        panel
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme.text_secondary)
                                            .child("Selected directory"),
                                    )
                                    .child(path_label(&directory.canonical_path, theme)),
                            )
                            .when(directory.is_home_dir, |panel| {
                                panel.child(
                                    div()
                                        .rounded(px(8.))
                                        .p(px(10.))
                                        .bg(theme.warn_soft)
                                        .text_color(theme.warn_text)
                                        .child(home_scope_notice()),
                                )
                            })
                            .when(!directory.is_git_repo, |panel| {
                                panel.child(
                                    div()
                                        .text_color(theme.text_secondary)
                                        .child("Non-Git folder — works without git init"),
                                )
                            })
                            .when(!busy && !at_selected_path, |panel| {
                                panel.child(
                                    div()
                                        .text_color(theme.text_secondary)
                                        .child("Open the typed path with Go before saving."),
                                )
                            })
                            .child(self.directory_entries(directory, theme, cx))
                    }),
            )
    }

    fn directory_entries(
        &self,
        directory: &ServerBrowseResult,
        theme: &SandTheme,
        cx: &Context<Self>,
    ) -> Div {
        let folders = directory.entries.iter().filter(|entry| entry.is_dir);
        let empty_text = if directory.entries.is_empty() {
            "This directory is empty."
        } else {
            "No subdirectories here."
        };
        let hover = theme.bg_hover;
        div()
            .flex()
            .flex_col()
            .when(folders.clone().next().is_none(), |list| {
                list.child(
                    div()
                        .py(px(8.))
                        .text_color(theme.text_secondary)
                        .child(empty_text),
                )
            })
            .children(folders.map(|entry| {
                let path = entry.path.clone();
                let busy = self.request.is_some();
                div()
                    .id(SharedString::from(format!(
                        "project-picker-directory-{path}"
                    )))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(7.))
                    .rounded(px(7.))
                    .when(!busy, |row| {
                        row.cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.browse(Some(path.clone()), false, window, cx)
                            }))
                    })
                    .child(cursor_icon("folder", 14., theme.text_secondary))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(entry.name.clone())
                            .when(entry.is_symlink, |row| {
                                let target = entry.symlink_target.as_deref().map_or_else(
                                    || "Symbolic link".to_owned(),
                                    |target| format!("Symbolic link → {target}"),
                                );
                                row.child(path_label(&target, theme))
                            }),
                    )
            }))
    }
}

impl Render for ProjectPickerModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let is_open = app.project_picker_open;
        self.motion.set_open(is_open);
        if !is_open {
            return div().into_any_element();
        }

        if self
            .server
            .as_ref()
            .is_none_or(|server| !server.matches(app))
        {
            let server = ServerKey::from_app(app);
            self.cancel_request();
            self.server = Some(server);
            self.directory = None;
            self.error = None;
            self.smooth.cancel();
            self.scroll.set_offset(Point::default());
            self.name_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.path_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            let focus = self.focus.clone();
            window.on_next_frame(move |window, cx| {
                if focus.is_rendered(window) {
                    window.focus(&focus, cx);
                }
            });
            self.browse(None, false, window, cx);
        }

        let app = self.app.read(cx);
        let theme = app.sand_theme();
        let server = self
            .server
            .as_ref()
            .expect("the picker records its server before rendering");
        let busy = self.request.as_ref().map(|(operation, _)| *operation);
        let path = self.path_input.read(cx).value();
        let can_save = busy.is_none()
            && self
                .directory
                .as_ref()
                .is_some_and(|directory| path.trim() == directory.canonical_path);
        let modal_height = (window.viewport_size().height - px(48.)).clamp(px(0.), px(740.));
        let server_name = if app.connection.name.trim().is_empty() {
            "Hermes server".into()
        } else {
            app.connection.name.clone()
        };
        deferred(
            div()
                .id("project-picker-scrim")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.))
                .font_family(FONT_SANS)
                .text_color(theme.text_primary)
                .child(
                    self.motion
                        .backdrop("project-picker-backdrop-enter", theme.bg_scrim),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| view.close(cx)),
                )
                .child(
                    self.motion.card(
                        "project-picker-card-enter",
                        div()
                            .id("project-picker-card")
                            .w(px(650.))
                            .max_w_full()
                            .h(modal_height)
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .rounded(px(20.))
                            .border_1()
                            .border_color(theme.border_default)
                            .bg(theme.bg_elevated)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                                if event.keystroke.key == "escape" {
                                    view.close(cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_start()
                                    .justify_between()
                                    .gap(px(12.))
                                    .px(px(16.))
                                    .py(px(12.))
                                    .border_b_1()
                                    .border_color(theme.border_hairline)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.))
                                            .child(
                                                div()
                                                    .text_size(px(17.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Choose a Work project"),
                                            )
                                            .child(
                                                div()
                                                    .text_color(theme.text_secondary)
                                                    .child(server_name),
                                            )
                                            .child(path_label(&server.url, &theme)),
                                    )
                                    .child(
                                        gb_mini_icon_btn("close-project-picker", &theme, false)
                                            .child(cursor_icon("close", 14., theme.text_primary))
                                            .on_click(cx.listener(|view, _, _, cx| view.close(cx))),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .relative()
                                    .child(
                                        div()
                                            .id("project-picker-body")
                                            .track_scroll(&self.scroll)
                                            .size_full()
                                            .overflow_y_scroll()
                                            .restrict_scroll_to_axis()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .w_full()
                                                    .flex_shrink_0()
                                                    .p(px(16.))
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(14.))
                                                    .child(self.saved_projects(
                                                        &app.work_projects,
                                                        server,
                                                        app.selected_work_project_id.as_deref(),
                                                        &theme,
                                                        cx,
                                                    ))
                                                    .child(self.browser_panel(&theme, cx)),
                                            ),
                                    )
                                    .vertical_scrollbar(&self.scroll)
                                    .child(smooth_scroll_layer(&self.smooth)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .flex_wrap()
                                    .items_center()
                                    .justify_end()
                                    .gap(px(8.))
                                    .px(px(16.))
                                    .py(px(10.))
                                    .border_t_1()
                                    .border_color(theme.border_hairline)
                                    .when_some(busy, |row, operation| {
                                        row.child(
                                            div()
                                                .flex()
                                                .flex_1()
                                                .items_center()
                                                .gap(px(8.))
                                                .text_color(theme.text_secondary)
                                                .child(gpu_spinner(14., theme.accent_text))
                                                .child(match operation {
                                                    DirectoryOperation::Browsing => {
                                                        "Loading directories…"
                                                    }
                                                    DirectoryOperation::Validating => {
                                                        "Validating directory…"
                                                    }
                                                }),
                                        )
                                    })
                                    .child(
                                        gb_btn(
                                            "project-picker-cancel",
                                            &theme,
                                            BtnVariant::Ghost,
                                            false,
                                            false,
                                        )
                                        .child("Cancel")
                                        .on_click(cx.listener(|view, _, _, cx| view.close(cx))),
                                    )
                                    .child(
                                        gb_btn(
                                            "project-picker-save",
                                            &theme,
                                            BtnVariant::Primary,
                                            false,
                                            !can_save,
                                        )
                                        .child("Save & Use Project")
                                        .on_click(cx.listener(|view, _, _, cx| view.save(cx))),
                                    ),
                            ),
                    ),
                ),
        )
        .into_any_element()
    }
}

fn path_label(path: &str, theme: &SandTheme) -> Div {
    div()
        .min_w_0()
        .font_family(FONT_MONO)
        .text_size(px(12.))
        .text_color(theme.text_secondary)
        .child(SharedString::from(path))
}
