use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_row, gb_section_card, BadgeTone, BtnVariant, FONT_MONO,
    FONT_SANS,
};
use crate::ui::motion::ModalMotion;
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;

pub struct CronView {
    app: Entity<AppState>,
    new_job_modal: ModalMotion,
    saving_job: bool,
    name_input: Entity<InputState>,
    schedule_input: Entity<InputState>,
    prompt_input: Entity<TextareaState>,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
}

impl CronView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Morning Git Digest"));
        let schedule_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("0 9 * * *");
            st.set_value("0 9 * * *", window, cx);
            st
        });
        let prompt_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What should Hermes do when triggered?")
                .rows(4)
        });
        cx.observe(&app, |_, app_e, cx| {
            let app = app_e.read(cx);
            if !app.setup_open && !app.initializing && app.active_view == AppViewId::Cron {
                cx.notify();
            }
        })
        .detach();
        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());
        Self {
            app,
            new_job_modal: ModalMotion::default(),
            saving_job: false,
            name_input,
            schedule_input,
            prompt_input,
            scroll_handle,
            smooth_scroller,
        }
    }

    fn create_job(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving_job {
            return;
        }
        let name = self.name_input.read(cx).value().trim().to_string();
        let schedule = self.schedule_input.read(cx).value().trim().to_string();
        let prompt = self.prompt_input.read(cx).value().trim().to_string();
        if name.is_empty() || schedule.is_empty() || prompt.is_empty() {
            self.app.update(cx, |app, cx| {
                app.show_toast("Enter a name, schedule, and prompt.", cx)
            });
            return;
        }
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let profile = app.active_profile.clone();
        let source = app.profile_generation;
        let rt = app.tokio_rt.clone();
        self.saving_job = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = rt
                .spawn(async move {
                    client
                        .create_cron_job(&name, &schedule, &prompt, &profile)
                        .await
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update_in(cx, |view, window, cx| {
                view.saving_job = false;
                cx.notify();
                if view.app.read(cx).profile_generation != source {
                    return;
                }
                match result {
                    Ok(_) => {
                        view.new_job_modal.set_open(false);
                        view.name_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        view.prompt_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        view.app.update(cx, |app, cx| {
                            app.refresh_cron_jobs(cx);
                            app.show_toast("Created cron automation", cx);
                        });
                    }
                    Err(error) => view.app.update(cx, |app, cx| app.show_toast(error, cx)),
                }
            });
        })
        .detach();
    }
}

impl Render for CronView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.app.read(cx);
        let theme = s.sand_theme();
        let jobs = s.cron_jobs.clone();
        let has_client = s.client.is_some();
        let modal_height = (window.viewport_size().height - px(48.0)).max(px(0.0));

        div()
            .size_full()
            .min_w_0()
            .bg(theme.bg_pane)
            .text_color(theme.text_primary)
            .font_family(FONT_SANS)
            .flex()
            .flex_col()
            .overflow_hidden()
            .relative()
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
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .child(div().text_size(px(16.0)).font_weight(FontWeight::SEMIBOLD).child("Cron & Automations"))
                            .child(div().text_size(px(12.5)).text_color(theme.text_tertiary).child("Scheduled autonomous Hermes agent runs")),
                    )
                    .child(
                        gb_btn("cron-new-btn", &theme, BtnVariant::Primary, false, false)
                            .child(cursor_icon("plus", 13.0, theme.text_on_primary))
                            .child("New Schedule")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new_job_modal.set_open(true);
                                cx.notify();
                            })),
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
                            .id("cron-scroll")
                            .track_scroll(&self.scroll_handle)
                            .size_full()
                            .overflow_y_scroll()
                            .px(px(24.0))
                    .py(px(20.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w(px(920.0))
                            .max_w_full()
                            .flex_shrink_0()
                            .child(gb_section_card(&theme).children(if jobs.is_empty() {
                                vec![div()
                                    .p(px(40.0))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(10.0))
                                    .text_color(theme.text_tertiary)
                                    .child(cursor_icon("clock", 28.0, theme.text_tertiary))
                                    .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child("No scheduled jobs yet"))
                                    .child("Automate daily briefings, repository audits, or notifications with Hermes Cron.")
                                    .child(
                                        gb_btn("cron-empty-add", &theme, BtnVariant::Primary, false, false)
                                            .child("Create Cron Job")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.new_job_modal.set_open(true);
                                                cx.notify();
                                            })),
                                    )
                                    .into_any_element()]
                            } else {
                                jobs.iter()
                                    .map(|job| {
                                        let jid_run = job.id.clone();
                                        let jname_run = job.name.clone();
                                        let jid_tog = job.id.clone();
                                        let jid_del = job.id.clone();
                                        let paused = job.paused;
                                        gb_row(&theme).flex_wrap()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(8.0))
                                                            .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(job.name.clone()))
                                                            .child(
                                                                gb_badge(&theme, BadgeTone::Muted, job.schedule.clone())
                                                                    .font_family(FONT_MONO),
                                                            )
                                                            .child(gb_badge(
                                                                &theme,
                                                                if paused { BadgeTone::Warning } else { BadgeTone::Success },
                                                                if paused { "Paused" } else { "Active" },
                                                            )),
                                                    )
                                                    .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child(job.prompt.clone())),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .child(
                                                        gb_btn(
                                                            SharedString::from(format!("cron-run-{}", job.id)),
                                                            &theme,
                                                            BtnVariant::Secondary,
                                                            true,
                                                            !has_client,
                                                        )
                                                        .child(cursor_icon("play", 12.0, theme.text_primary))
                                                        .child("Run")
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            let (c, rt) = {
                                                                let s = this.app.read(cx);
                                                                (s.client.clone(), s.tokio_rt.clone())
                                                            };
                                                            let id = jid_run.clone();
                                                            let nm = jname_run.clone();
                                                            if let Some(c) = c {
                                                                cx.spawn(async move |this, cx| {
                                                                    let res = rt.spawn(async move { c.trigger_cron_job(&id).await })
                                                                        .await.unwrap_or_else(|e| Err(e.to_string()));
                                                                    let _ = this.update(cx, |v, cx| {
                                                                        v.app.update(cx, |s, cx| match res {
                                                                            Ok(_) => {
                                                                                s.refresh_cron_jobs(cx);
                                                                                s.show_toast(format!("Triggered \"{nm}\""), cx);
                                                                            }
                                                                            Err(e) => s.show_toast(e, cx),
                                                                        });
                                                                    });
                                                                })
                                                                .detach();
                                                            }
                                                        })),
                                                    )
                                                    .child(
                                                        gb_btn(
                                                            SharedString::from(format!("cron-tog-{}", job.id)),
                                                            &theme,
                                                            BtnVariant::Secondary,
                                                            true,
                                                            !has_client,
                                                        )
                                                        .child(cursor_icon(if paused { "play" } else { "pause" }, 12.0, theme.text_primary))
                                                        .child(if paused { "Resume" } else { "Pause" })
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.app.update(cx, |s, cx| {
                                                                if s.client.is_some() { s.toggle_cron_pause_optimistic(jid_tog.clone(), !paused, cx); }
                                                            });
                                                        })),
                                                    )
                                                    .child(
                                                        gb_btn(
                                                            SharedString::from(format!("cron-del-{}", job.id)),
                                                            &theme,
                                                            BtnVariant::Danger,
                                                            true,
                                                            !has_client,
                                                        )
                                                        .child(cursor_icon("trash", 12.0, theme.danger_text))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.app.update(cx, |s, cx| {
                                                                if s.client.is_some() { s.delete_cron_optimistic(jid_del.clone(), cx); }
                                                            });
                                                        })),
                                                    ),
                                            )
                                            .into_any_element()
                                    })
                                    .collect()
                            })),
                    ),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(smooth_scroll_layer(&self.smooth_scroller)),
            )
            .when(self.new_job_modal.is_open(), |d| {
                d.child(deferred(
                    div()
                        .id("cron-modal-backdrop")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .child(self.new_job_modal.backdrop("cron-scrim-enter", theme.bg_scrim))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(20.0))
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                            this.new_job_modal.set_open(false);
                            cx.notify();
                        }))
                        .child(self.new_job_modal.card("cron-card-enter",
                            div()
                                .id("cron-modal")
                                .w(px(480.0))
                                .max_w_full()
                                .max_h(modal_height)
                                .rounded(px(22.0))
                                .bg(theme.bg_elevated)
                                .border_1()
                                .border_color(theme.border_default)
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(div().text_size(px(15.5)).font_weight(FontWeight::SEMIBOLD).child("New Hermes Cron Job"))
                                .child(Input::new(&self.name_input).h(px(36.0)).disabled(self.saving_job))
                                .child(Input::new(&self.schedule_input).h(px(36.0)).disabled(self.saving_job))
                                .child(Textarea::new(&self.prompt_input).h(px(100.0)).readonly(self.saving_job))
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .child(
                                            gb_btn("cron-cancel", &theme, BtnVariant::Ghost, false, false)
                                                .child("Cancel")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.new_job_modal.set_open(false);
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_btn("cron-save", &theme, BtnVariant::Primary, false, self.saving_job || !has_client)
                                                .child("Save Schedule")
                                                .on_click(cx.listener(|this, _, window, cx| this.create_job(window, cx))),
                                        ),
                                )
                                .overflow_y_scrollbar()
                        )),
                ))
            })
    }
}
