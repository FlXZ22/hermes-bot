use crate::hermes::types::{HermesKanbanStatus, HermesKanbanTask};
use crate::state::app_state::AppState;
use crate::theme::bot_face::{bot_face, BotFaceState};
use crate::theme::components::{
    cursor_icon, format_relative_time, gb_badge, gb_board_pill, gb_btn, gb_dot, gb_mini_icon_btn,
    gb_segmented_item, gb_segmented_shell, short_model_name, BadgeTone, BtnVariant, FONT_SANS,
};
use crate::theme::tokens::{resolve_bot_color, SandTheme};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardsMode {
    Kanban,
    Sessions,
}

const BOARD_COLORS: &[&str] = &[
    "blue", "violet", "cyan", "green", "orange", "red", "magenta", "yellow",
];

pub struct BoardsView {
    app: Entity<AppState>,
    mode: BoardsMode,
    // Task editor modal
    task_modal_open: bool,
    editing_task_id: Option<String>,
    task_title: Entity<InputState>,
    task_body: Entity<TextareaState>,
    task_status: HermesKanbanStatus,
    task_priority: Entity<InputState>,
    task_cwd: Entity<InputState>,
    task_assignee: String,
    task_model: String,
    task_provider: String,
    // New Board modal
    board_modal_open: bool,
    new_board_name: Entity<InputState>,
    new_board_desc: Entity<InputState>,
    new_board_path: Entity<InputState>,
    new_board_color: String,
}

impl BoardsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let task_title =
            cx.new(|cx| InputState::new(window, cx).placeholder("What should Hermes build or investigate?"));
        let task_body = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Detailed context, acceptance criteria, or instructions...")
                .rows(4)
        });
        let task_priority = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("0");
            st.set_value("0", window, cx);
            st
        });
        let task_cwd =
            cx.new(|cx| InputState::new(window, cx).placeholder("/home/metis/Projects/..."));
        let new_board_name =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Frontend Sprint, Research Lab"));
        let new_board_desc =
            cx.new(|cx| InputState::new(window, cx).placeholder("Optional description"));
        let new_board_path =
            cx.new(|cx| InputState::new(window, cx).placeholder("/home/metis/Projects/..."));

        cx.observe(&app, |_, _, cx| cx.notify()).detach();

        Self {
            app,
            mode: BoardsMode::Kanban,
            task_modal_open: false,
            editing_task_id: None,
            task_title,
            task_body,
            task_status: HermesKanbanStatus::Todo,
            task_priority,
            task_cwd,
            task_assignee: "default".into(),
            task_model: String::new(),
            task_provider: "auto".into(),
            board_modal_open: false,
            new_board_name,
            new_board_desc,
            new_board_path,
            new_board_color: "violet".into(),
        }
    }

    fn open_new_task(&mut self, default_status: HermesKanbanStatus, window: &mut Window, cx: &mut Context<Self>) {
        let (prof, model, prov, cwd) = {
            let s = self.app.read(cx);
            let wp = s
                .boards
                .iter()
                .find(|b| b.slug == s.active_board_slug)
                .and_then(|b| b.workspace_path.clone())
                .unwrap_or_default();
            (s.active_profile.clone(), s.active_model.clone(), s.active_provider.clone(), wp)
        };
        self.editing_task_id = None;
        self.task_title.update(cx, |st, cx| st.set_value("", window, cx));
        self.task_body.update(cx, |st, cx| st.set_value("", window, cx));
        self.task_priority.update(cx, |st, cx| st.set_value("0", window, cx));
        self.task_cwd.update(cx, |st, cx| st.set_value(cwd, window, cx));
        self.task_status = default_status;
        self.task_assignee = prof;
        self.task_model = model;
        self.task_provider = prov;
        self.task_modal_open = true;
        cx.notify();
    }

    fn open_edit_task(&mut self, task: &HermesKanbanTask, window: &mut Window, cx: &mut Context<Self>) {
        let (prof, model, prov) = {
            let s = self.app.read(cx);
            (s.active_profile.clone(), s.active_model.clone(), s.active_provider.clone())
        };
        self.editing_task_id = Some(task.id.clone());
        let title = task.title.clone();
        let body = task.body.clone().or_else(|| task.description.clone()).unwrap_or_default();
        let prio = task.priority_num().to_string();
        let cwd = task.cwd.clone().unwrap_or_default();
        self.task_title.update(cx, |st, cx| st.set_value(title, window, cx));
        self.task_body.update(cx, |st, cx| st.set_value(body, window, cx));
        self.task_priority.update(cx, |st, cx| st.set_value(prio, window, cx));
        self.task_cwd.update(cx, |st, cx| st.set_value(cwd, window, cx));
        self.task_status = task.status;
        self.task_assignee = task.assignee.clone().unwrap_or(prof);
        self.task_model = task.model.clone().unwrap_or(model);
        self.task_provider = task.provider.clone().unwrap_or(prov);
        self.task_modal_open = true;
        cx.notify();
    }

    fn save_task(&mut self, cx: &mut Context<Self>) {
        let title = self.task_title.read(cx).value().trim().to_string();
        if title.is_empty() {
            return;
        }
        let body = self.task_body.read(cx).value().trim().to_string();
        let prio: i32 = self.task_priority.read(cx).value().trim().parse().unwrap_or(0);
        let cwd = self.task_cwd.read(cx).value().trim().to_string();
        let status = self.task_status;
        let assignee = self.task_assignee.clone();
        let model = self.task_model.clone();
        let provider = self.task_provider.clone();

        if let Some(eid) = self.editing_task_id.clone() {
            self.app.update(cx, |s, cx| {
                let mut existing = s
                    .tasks
                    .iter()
                    .find(|t| t.id == eid)
                    .cloned()
                    .unwrap_or_default();
                existing.id = eid.clone();
                existing.title = title;
                existing.body = Some(body);
                existing.status = status;
                existing.priority = Some(json!(prio));
                existing.assignee = Some(assignee);
                existing.model = Some(model);
                existing.provider = Some(provider);
                existing.cwd = if cwd.is_empty() { None } else { Some(cwd) };
                s.update_task(eid, existing, cx);
            });
        } else {
            self.app.update(cx, |s, cx| {
                let board = if s.active_board_slug == "all" {
                    "default".into()
                } else {
                    s.active_board_slug.clone()
                };
                s.create_task(
                    HermesKanbanTask {
                        id: String::new(),
                        board: Some(board),
                        title,
                        body: Some(body),
                        description: None,
                        status,
                        priority: Some(json!(prio)),
                        assignee: Some(assignee),
                        model: Some(model),
                        provider: Some(provider),
                        cwd: if cwd.is_empty() { None } else { Some(cwd) },
                        session_id: None,
                    },
                    cx,
                );
            });
        }
        self.task_modal_open = false;
        cx.notify();
    }
}

impl Render for BoardsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.app.read(cx);
        let theme = s.sand_theme();
        let boards = s.boards.clone();
        let active_slug = s.active_board_slug.clone();
        let active_board = if active_slug == "all" {
            None
        } else {
            boards.iter().find(|b| b.slug == active_slug).cloned()
        };

        let visible_tasks: Vec<HermesKanbanTask> = if active_slug == "all" {
            s.tasks.clone()
        } else {
            s.tasks
                .iter()
                .filter(|t| t.board.as_deref().unwrap_or("default") == active_slug)
                .cloned()
                .collect()
        };
        let visible_sessions: Vec<_> = if active_slug == "all" {
            s.sessions.clone()
        } else {
            s.sessions
                .iter()
                .filter(|sess| {
                    s.session_boards
                        .get(&sess.id)
                        .map(String::as_str)
                        .or(sess.board_slug.as_deref())
                        .unwrap_or("default")
                        == active_slug
                })
                .cloned()
                .collect()
        };
        let active_profile = s.active_profile.clone();
        let bot_avatars = s.bot_avatars.clone();
        let profiles = s.profiles.clone();

        let title = active_board
            .as_ref()
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "All Session Boards".into());
        let sub = active_board
            .as_ref()
            .and_then(|b| b.description.clone())
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "{} tasks · {} sessions across {} boards",
                    visible_tasks.len(),
                    visible_sessions.len(),
                    boards.len()
                )
            });

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(theme.bg_pane)
            .text_color(theme.text_primary)
            .font_family(FONT_SANS)
            .flex()
            .flex_col()
            .overflow_hidden()
            .relative()
            // Panel Header
            .child(
                div()
                    .min_h(px(54.0))
                    .px(px(20.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme.border_default)
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap(px(12.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(16.0)).font_weight(FontWeight::SEMIBOLD).child(title))
                            .child(div().text_size(px(12.5)).text_color(theme.text_tertiary).child(sub)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                gb_segmented_shell(&theme)
                                    .child(
                                        gb_segmented_item("bd-mode-kanban", &theme, self.mode == BoardsMode::Kanban, "Kanban")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.mode = BoardsMode::Kanban;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item(
                                            "bd-mode-sess",
                                            &theme,
                                            self.mode == BoardsMode::Sessions,
                                            format!("Sessions ({})", visible_sessions.len()),
                                        )
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mode = BoardsMode::Sessions;
                                            cx.notify();
                                        })),
                                    ),
                            )
                            .child(
                                gb_btn("bd-dispatch", &theme, BtnVariant::Secondary, false, false)
                                    .child(cursor_icon("play", 13.0, theme.text_primary))
                                    .child("Dispatch")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| s.dispatch_board_workers(cx));
                                    })),
                            )
                            .child(
                                gb_btn("bd-new-primary", &theme, BtnVariant::Primary, false, false)
                                    .child(cursor_icon("plus", 13.0, theme.text_on_primary))
                                    .child(if self.mode == BoardsMode::Kanban {
                                        "New Task"
                                    } else {
                                        "New Session"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.mode == BoardsMode::Kanban {
                                            this.open_new_task(HermesKanbanStatus::Todo, window, cx);
                                        } else {
                                            this.app.update(cx, |s, cx| {
                                                s.create_new_session(None, None, None, None, None, cx);
                                            });
                                        }
                                    })),
                            ),
                    ),
            )
            // Board Switcher Sub-bar
            .child(
                div()
                    .w_full()
                    .px(px(20.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme.border_default)
                    .bg(theme.bg_chrome)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("bd-sub-pills")
                            .flex_1()
                            .overflow_x_scroll()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                gb_board_pill("bd-pill-all", &theme, active_slug == "all")
                                    .child("All Boards")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| {
                                            s.active_board_slug = "all".into();
                                            cx.notify();
                                        });
                                    })),
                            )
                            .children(boards.iter().map(|b| {
                                let sl = b.slug.clone();
                                let preset = resolve_bot_color(&b.slug, b.color.as_deref());
                                gb_board_pill(
                                    SharedString::from(format!("bd-pill-{}", b.slug)),
                                    &theme,
                                    active_slug == b.slug,
                                )
                                .child(gb_dot(preset.hex, 7.0))
                                .child(b.name.clone())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.app.update(cx, |s, cx| {
                                        s.active_board_slug = sl.clone();
                                        cx.notify();
                                    });
                                }))
                            }))
                            .child(
                                gb_board_pill("bd-pill-new", &theme, false)
                                    .child(cursor_icon("plus", 11.0, theme.text_secondary))
                                    .child("New Board")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.board_modal_open = true;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when_some(active_board.filter(|b| b.slug != "default"), |d, b| {
                        let sl = b.slug.clone();
                        d.child(
                            gb_btn("bd-delete-board", &theme, BtnVariant::Danger, true, false)
                                .child(cursor_icon("trash", 12.0, theme.danger_text))
                                .child("Delete Board")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.app.update(cx, |s, cx| s.delete_board(sl.clone(), cx));
                                })),
                        )
                    }),
            )
            // Body: Kanban columns OR Sessions grid
            .child(if self.mode == BoardsMode::Kanban {
                div()
                    .id("kanban-cols-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_x_scroll()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .gap(px(12.0))
                    .children(HermesKanbanStatus::COLUMNS.iter().enumerate().map(|(col_idx, &(status, label, dot_hex))| {
                        let col_tasks: Vec<HermesKanbanTask> = visible_tasks
                            .iter()
                            .filter(|t| t.status == status)
                            .cloned()
                            .collect();
                        let prev_status = if col_idx > 0 {
                            Some(HermesKanbanStatus::COLUMNS[col_idx - 1].0)
                        } else {
                            None
                        };
                        let next_status = if col_idx + 1 < HermesKanbanStatus::COLUMNS.len() {
                            Some(HermesKanbanStatus::COLUMNS[col_idx + 1].0)
                        } else {
                            None
                        };

                        div()
                            .w(px(286.0))
                            .min_w(px(286.0))
                            .h_full()
                            .rounded(px(16.0))
                            .bg(theme.bg_sidebar)
                            .border_1()
                            .border_color(theme.border_default)
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            // Column header
                            .child(
                                div()
                                    .px(px(12.0))
                                    .py(px(11.0))
                                    .border_b_1()
                                    .border_color(theme.sidebar_separator)
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(7.0))
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(gb_dot(dot_hex, 7.0))
                                            .child(label)
                                            .child(gb_badge(&theme, BadgeTone::Muted, col_tasks.len().to_string())),
                                    )
                                    .child(
                                        gb_mini_icon_btn(
                                            SharedString::from(format!("add-col-{}", status.as_str())),
                                            &theme,
                                            false,
                                        )
                                        .child(cursor_icon("plus", 13.0, theme.text_secondary))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_new_task(status, window, cx);
                                        })),
                                    ),
                            )
                            // Column body
                            .child(
                                div()
                                    .id(SharedString::from(format!("col-body-{}", status.as_str())))
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .p(px(10.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .children(col_tasks.iter().map(|task| {
                                        let t_edit = task.clone();
                                        let t_run = task.clone();
                                        let t_left = task.clone();
                                        let t_right = task.clone();
                                        let seed = task.assignee.as_deref().unwrap_or(&active_profile);
                                        let av = bot_avatars.get(seed);
                                        let prio = task.priority_num();
                                        let prio_tone = if prio >= 2 {
                                            BadgeTone::Danger
                                        } else if prio >= 1 {
                                            BadgeTone::Warning
                                        } else {
                                            BadgeTone::Muted
                                        };
                                        let desc = task.body.clone().or_else(|| task.description.clone()).unwrap_or_default();
                                        let has_sess = task.session_id.is_some();

                                        div()
                                            .id(SharedString::from(format!("task-card-{}", task.id)))
                                            .w_full()
                                            .p(px(11.0))
                                            .rounded(px(13.0))
                                            .bg(theme.bg_elevated)
                                            .border_1()
                                            .border_color(theme.border_default)
                                            .flex()
                                            .flex_col()
                                            .gap(px(7.0))
                                            .cursor_pointer()
                                            .hover(|st| st.border_color(theme.border_primary))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_edit_task(&t_edit, window, cx);
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(13.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(task.title.clone()),
                                            )
                                            .when(!desc.is_empty(), |d| {
                                                d.child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(theme.text_secondary)
                                                        .child(desc.chars().take(120).collect::<String>()),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap(px(6.0))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(5.0))
                                                            .child(bot_face(
                                                                &theme,
                                                                seed,
                                                                av.and_then(|a| a.shape_id.as_deref()),
                                                                av.and_then(|a| a.color_id.as_deref()),
                                                                20.0,
                                                                BotFaceState::Idle,
                                                                0,
                                                            ))
                                                            .child(gb_badge(&theme, prio_tone, prio.to_string()))
                                                            .when_some(task.model.as_ref(), |d, m| {
                                                                d.child(gb_badge(&theme, BadgeTone::Muted, short_model_name(m)))
                                                            }),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(3.0))
                                                            .when_some(prev_status, |d, ps| {
                                                                d.child(
                                                                    gb_mini_icon_btn(
                                                                        SharedString::from(format!("mv-l-{}", task.id)),
                                                                        &theme,
                                                                        false,
                                                                    )
                                                                    .child(cursor_icon("chevron-left", 11.0, theme.text_tertiary))
                                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                                        cx.stop_propagation();
                                                                        let mut next = t_left.clone();
                                                                        next.status = ps;
                                                                        this.app.update(cx, |s, cx| {
                                                                            s.update_task(next.id.clone(), next, cx);
                                                                        });
                                                                    })),
                                                                )
                                                            })
                                                            .when_some(next_status, |d, ns| {
                                                                d.child(
                                                                    gb_mini_icon_btn(
                                                                        SharedString::from(format!("mv-r-{}", task.id)),
                                                                        &theme,
                                                                        false,
                                                                    )
                                                                    .child(cursor_icon("chevron-right", 11.0, theme.text_tertiary))
                                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                                        cx.stop_propagation();
                                                                        let mut next = t_right.clone();
                                                                        next.status = ns;
                                                                        this.app.update(cx, |s, cx| {
                                                                            s.update_task(next.id.clone(), next, cx);
                                                                        });
                                                                    })),
                                                                )
                                                            })
                                                            .child(
                                                                gb_btn(
                                                                    SharedString::from(format!("run-t-{}", task.id)),
                                                                    &theme,
                                                                    BtnVariant::Secondary,
                                                                    true,
                                                                    false,
                                                                )
                                                                .child(cursor_icon("chat-bubble", 11.0, theme.text_primary))
                                                                .child(if has_sess { "Open" } else { "Run" })
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    this.app.update(cx, |s, cx| {
                                                                        s.launch_session_for_task(t_run.clone(), cx);
                                                                    });
                                                                })),
                                                            ),
                                                    ),
                                            )
                                    }))
                                    .when(col_tasks.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .p(px(18.0))
                                                .rounded(px(12.0))
                                                .border_1()
                                                .border_color(theme.border_subtle)
                                                .text_center()
                                                .text_size(px(12.0))
                                                .text_color(theme.text_tertiary)
                                                .child("No tasks yet"),
                                        )
                                    }),
                            )
                    }))
                    .into_any_element()
            } else {
                div()
                    .id("board-sessions-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(24.0))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(920.0))
                            .max_w_full()
                            .flex()
                            .flex_wrap()
                            .gap(px(12.0))
                            .children(visible_sessions.iter().map(|sess| {
                                let sid = sess.id.clone();
                                let seed = sess.profile.as_deref().unwrap_or(&sess.id);
                                let av = bot_avatars.get(seed);
                                div()
                                    .id(SharedString::from(format!("bs-card-{}", sess.id)))
                                    .w(px(280.0))
                                    .p(px(14.0))
                                    .rounded(px(14.0))
                                    .bg(theme.bg_elevated)
                                    .border_1()
                                    .border_color(theme.border_default)
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .cursor_pointer()
                                    .hover(|st| st.bg(theme.bg_elevated_hover))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.app.update(cx, |s, cx| s.select_session(Some(sid.clone()), cx));
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(10.0))
                                            .child(bot_face(
                                                &theme,
                                                seed,
                                                av.and_then(|a| a.shape_id.as_deref()),
                                                av.and_then(|a| a.color_id.as_deref()),
                                                36.0,
                                                BotFaceState::Idle,
                                                0,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .overflow_hidden()
                                                            .text_ellipsis()
                                                            .whitespace_nowrap()
                                                            .child(sess.display_title().to_string()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.5))
                                                            .text_color(theme.text_tertiary)
                                                            .child(format!(
                                                                "{} · {} msgs",
                                                                format_relative_time(sess.timestamp_secs()),
                                                                sess.message_count.unwrap_or(0)
                                                            )),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme.text_secondary)
                                            .child(sess.preview.clone().unwrap_or_else(|| "Open conversation".into())),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(6.0))
                                            .when_some(sess.model.as_ref(), |d, m| {
                                                d.child(gb_badge(&theme, BadgeTone::Muted, short_model_name(m)))
                                            })
                                            .when_some(sess.source.as_ref(), |d, src| {
                                                d.child(gb_badge(&theme, BadgeTone::Info, src.clone()))
                                            }),
                                    )
                            })),
                    )
                    .into_any_element()
            })
            // Task Editor Modal
            .when(self.task_modal_open, |d| {
                d.child(self.render_task_modal(&theme, &profiles, cx))
            })
            // New Board Modal
            .when(self.board_modal_open, |d| {
                d.child(self.render_new_board_modal(&theme, cx))
            })
    }
}

impl BoardsView {
    fn render_task_modal(
        &self,
        theme: &SandTheme,
        profiles: &[crate::hermes::types::HermesProfile],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_edit = self.editing_task_id.is_some();
        let edit_id = self.editing_task_id.clone();

        deferred(
            div()
                .id("task-modal-backdrop")
                .absolute()
                .inset_0()
                .bg(theme.bg_scrim)
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.task_modal_open = false;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("task-modal")
                        .w(px(520.0))
                        .max_h(px(640.0))
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
                                        .child(if is_edit { "Edit Board Task" } else { "New Board Task" }),
                                )
                                .child(
                                    gb_mini_icon_btn("close-task-modal", theme, false)
                                        .child(cursor_icon("close", 14.0, theme.text_secondary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.task_modal_open = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("task-modal-body")
                                .flex_1()
                                .overflow_y_scroll()
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Task Title"))
                                        .child(Input::new(&self.task_title).h(px(36.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Specification / Prompt Details"))
                                        .child(Textarea::new(&self.task_body).h(px(96.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Column"))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(5.0))
                                                .children(HermesKanbanStatus::COLUMNS.iter().map(|&(st, lbl, dot)| {
                                                    gb_board_pill(
                                                        SharedString::from(format!("tm-col-{}", st.as_str())),
                                                        theme,
                                                        self.task_status == st,
                                                    )
                                                    .child(gb_dot(dot, 6.0))
                                                    .child(lbl)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.task_status = st;
                                                        cx.notify();
                                                    }))
                                                })),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(10.0))
                                        .child(
                                            div()
                                                .w(px(110.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.0))
                                                .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Priority"))
                                                .child(Input::new(&self.task_priority).h(px(34.0))),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.0))
                                                .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Assigned Profile"))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_wrap()
                                                        .gap(px(4.0))
                                                        .children(profiles.iter().map(|p| {
                                                            let pn = p.name.clone();
                                                            gb_board_pill(
                                                                SharedString::from(format!("tm-prof-{}", p.name)),
                                                                theme,
                                                                self.task_assignee == p.name,
                                                            )
                                                            .child(p.name.clone())
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.task_assignee = pn.clone();
                                                                cx.notify();
                                                            }))
                                                        })),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .child(div().text_size(px(12.0)).text_color(theme.text_secondary).child("Workspace Directory (Optional)"))
                                        .child(Input::new(&self.task_cwd).h(px(34.0))),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .mt(px(6.0))
                                        .child(if let Some(eid) = edit_id {
                                            gb_btn("tm-delete", theme, BtnVariant::Danger, false, false)
                                                .child("Delete")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.app.update(cx, |s, cx| s.delete_task(eid.clone(), cx));
                                                    this.task_modal_open = false;
                                                    cx.notify();
                                                }))
                                                .into_any_element()
                                        } else {
                                            div().into_any_element()
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .gap(px(8.0))
                                                .child(
                                                    gb_btn("tm-cancel", theme, BtnVariant::Ghost, false, false)
                                                        .child("Cancel")
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.task_modal_open = false;
                                                            cx.notify();
                                                        })),
                                                )
                                                .child(
                                                    gb_btn("tm-save", theme, BtnVariant::Primary, false, false)
                                                        .child(if is_edit { "Save Task" } else { "Create Task" })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.save_task(cx);
                                                        })),
                                                ),
                                        ),
                                ),
                        ),
                ),
        )
    }

    fn render_new_board_modal(&self, theme: &SandTheme, cx: &mut Context<Self>) -> impl IntoElement {
        deferred(
            div()
                .id("bd-new-board-backdrop")
                .absolute()
                .inset_0()
                .bg(theme.bg_scrim)
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.board_modal_open = false;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("bd-new-board-modal")
                        .w(px(460.0))
                        .rounded(px(22.0))
                        .bg(theme.bg_elevated)
                        .border_1()
                        .border_color(theme.border_default)
                        .p(px(18.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(div().text_size(px(15.5)).font_weight(FontWeight::SEMIBOLD).child("Create Session Board"))
                        .child(Input::new(&self.new_board_name).h(px(36.0)))
                        .child(Input::new(&self.new_board_desc).h(px(36.0)))
                        .child(Input::new(&self.new_board_path).h(px(36.0)))
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .children(BOARD_COLORS.iter().map(|&c| {
                                    let preset = resolve_bot_color(c, Some(c));
                                    div()
                                        .id(SharedString::from(format!("nb-col-{c}")))
                                        .w(px(24.0))
                                        .h(px(24.0))
                                        .rounded_full()
                                        .bg(rgb(preset.hex))
                                        .border_2()
                                        .border_color(if self.new_board_color == c {
                                            theme.text_primary
                                        } else {
                                            SandTheme::transparent()
                                        })
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.new_board_color = c.into();
                                            cx.notify();
                                        }))
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    gb_btn("nb-cancel", theme, BtnVariant::Ghost, false, false)
                                        .child("Cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.board_modal_open = false;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    gb_btn("nb-create", theme, BtnVariant::Primary, false, false)
                                        .child("Create Board")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let name = this.new_board_name.read(cx).value().trim().to_string();
                                            if name.is_empty() {
                                                return;
                                            }
                                            let desc = this.new_board_desc.read(cx).value().trim().to_string();
                                            let path = this.new_board_path.read(cx).value().trim().to_string();
                                            let color = this.new_board_color.clone();
                                            this.app.update(cx, |s, cx| s.create_board(name, desc, color, path, cx));
                                            this.new_board_name.update(cx, |st, cx| st.set_value("", window, cx));
                                            this.new_board_desc.update(cx, |st, cx| st.set_value("", window, cx));
                                            this.new_board_path.update(cx, |st, cx| st.set_value("", window, cx));
                                            this.board_modal_open = false;
                                            cx.notify();
                                        })),
                                ),
                        ),
                ),
        )
    }
}
