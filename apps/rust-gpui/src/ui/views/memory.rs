use crate::state::app_state::AppState;
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_row, gb_section_card, gb_section_card_head,
    gb_segmented_item, gb_segmented_shell, BadgeTone, BtnVariant, FONT_SANS,
};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Textarea, TextareaState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryTab {
    Memory,
    User,
    Soul,
    Graph,
}

pub struct MemoryView {
    app: Entity<AppState>,
    tab: MemoryTab,
    mem_input: Entity<TextareaState>,
    user_input: Entity<TextareaState>,
    soul_input: Entity<TextareaState>,
    synced_mem: String,
    synced_user: String,
    synced_soul: String,
}

impl MemoryView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (m, u, so) = {
            let s = app.read(cx);
            (s.memory_md.clone(), s.user_md.clone(), s.soul_md.clone())
        };
        let mem_input = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .placeholder("Long-term curated agent memories...")
                .rows(16);
            st.set_value(m.clone(), window, cx);
            st
        });
        let user_input = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .placeholder("Your preferences, tools, OS, coding style...")
                .rows(16);
            st.set_value(u.clone(), window, cx);
            st
        });
        let soul_input = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .placeholder("Define Hermes personality & tone in SOUL.md...")
                .rows(16);
            st.set_value(so.clone(), window, cx);
            st
        });
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            tab: MemoryTab::Memory,
            mem_input,
            user_input,
            soul_input,
            synced_mem: m,
            synced_user: u,
            synced_soul: so,
        }
    }

    fn load_graph(&mut self, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(g)) = rt
                .spawn(async move { client.get_learning_graph().await })
                .await
            {
                let _ = this.update(cx, |v, cx| {
                    v.app.update(cx, |s, cx| {
                        s.learning_graph = Some(g);
                        cx.notify();
                    });
                });
            }
        })
        .detach();
    }
}

impl Render for MemoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (theme, is_local, mem, usr, soul, graph, has_client) = {
            let s = self.app.read(cx);
            (
                s.sand_theme(),
                s.connection.is_local(),
                s.memory_md.clone(),
                s.user_md.clone(),
                s.soul_md.clone(),
                s.learning_graph.clone(),
                s.client.is_some(),
            )
        };
        if self.synced_mem != mem {
            self.synced_mem = mem.clone();
            self.mem_input
                .update(cx, |st, cx| st.set_value(mem, window, cx));
        }
        if self.synced_user != usr {
            self.synced_user = usr.clone();
            self.user_input
                .update(cx, |st, cx| st.set_value(usr, window, cx));
        }
        if self.synced_soul != soul {
            self.synced_soul = soul.clone();
            self.soul_input
                .update(cx, |st, cx| st.set_value(soul, window, cx));
        }

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
                            .child(div().text_size(px(16.0)).font_weight(FontWeight::SEMIBOLD).child("Memory & Knowledge"))
                            .child(div().text_size(px(12.5)).text_color(theme.text_tertiary).child("Persistent Hermes memory, user profile, SOUL.md persona, and learning graph")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                gb_segmented_shell(&theme)
                                    .child(
                                        gb_segmented_item("mem-t-mem", &theme, self.tab == MemoryTab::Memory, "MEMORY.md")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::Memory;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("mem-t-user", &theme, self.tab == MemoryTab::User, "USER.md")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::User;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("mem-t-soul", &theme, self.tab == MemoryTab::Soul, "SOUL.md")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::Soul;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("mem-t-graph", &theme, self.tab == MemoryTab::Graph, "Learning Graph")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::Graph;
                                                this.load_graph(cx);
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .when(has_client, |d| {
                                d.child(
                                    gb_btn("mem-curator-btn", &theme, BtnVariant::Secondary, false, false)
                                        .child(cursor_icon("sparkles", 13.0, theme.text_primary))
                                        .child("Run Curator")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let (c, rt) = {
                                                let s = this.app.read(cx);
                                                (s.client.clone(), s.tokio_rt.clone())
                                            };
                                            if let Some(c) = c {
                                                cx.spawn(async move |this, cx| {
                                                    let _ = rt.spawn(async move { c.run_curator().await }).await;
                                                    let _ = this.update(cx, |v, cx| {
                                                        v.app.update(cx, |s, cx| s.show_toast("Triggered Hermes memory curator", cx));
                                                    });
                                                })
                                                .detach();
                                            }
                                        })),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("memory-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(24.0))
                    .py(px(20.0))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(920.0))
                            .max_w_full()
                            .when(self.tab == MemoryTab::Memory, |d| {
                                d.child(
                                    gb_section_card(&theme)
                                        .child(
                                            gb_section_card_head(&theme)
                                                .child(div().text_size(px(14.0)).font_weight(FontWeight::SEMIBOLD).child("~/.hermes/memories/MEMORY.md"))
                                                .child(
                                                    gb_btn("save-mem-btn", &theme, BtnVariant::Primary, true, !is_local)
                                                        .child("Save MEMORY.md")
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            let txt = this.mem_input.read(cx).value().to_string();
                                                            this.app.update(cx, |s, cx| s.save_memory_file("memory", txt, cx));
                                                        })),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .p(px(16.0))
                                                .child(Textarea::new(&self.mem_input).h(px(380.0)).readonly(!is_local)),
                                        ),
                                )
                            })
                            .when(self.tab == MemoryTab::User, |d| {
                                d.child(
                                    gb_section_card(&theme)
                                        .child(
                                            gb_section_card_head(&theme)
                                                .child(div().text_size(px(14.0)).font_weight(FontWeight::SEMIBOLD).child("~/.hermes/memories/USER.md"))
                                                .child(
                                                    gb_btn("save-user-btn", &theme, BtnVariant::Primary, true, !is_local)
                                                        .child("Save USER.md")
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            let txt = this.user_input.read(cx).value().to_string();
                                                            this.app.update(cx, |s, cx| s.save_memory_file("user", txt, cx));
                                                        })),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .p(px(16.0))
                                                .child(Textarea::new(&self.user_input).h(px(380.0)).readonly(!is_local)),
                                        ),
                                )
                            })
                            .when(self.tab == MemoryTab::Soul, |d| {
                                d.child(
                                    gb_section_card(&theme)
                                        .child(
                                            gb_section_card_head(&theme)
                                                .child(div().text_size(px(14.0)).font_weight(FontWeight::SEMIBOLD).child("~/.hermes/SOUL.md"))
                                                .child(
                                                    gb_btn("save-soul-btn", &theme, BtnVariant::Primary, true, false)
                                                        .child("Save SOUL.md")
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            let txt = this.soul_input.read(cx).value().to_string();
                                                            this.app.update(cx, |s, cx| s.save_memory_file("soul", txt, cx));
                                                        })),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .p(px(16.0))
                                                .child(Textarea::new(&self.soul_input).h(px(380.0))),
                                        ),
                                )
                            })
                            .when(self.tab == MemoryTab::Graph, |d| {
                                let nodes = graph.map(|g| g.nodes).unwrap_or_default();
                                d.child(
                                    gb_section_card(&theme)
                                        .child(
                                            gb_section_card_head(&theme).child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("Learned Skills & Memory Nodes ({})", nodes.len())),
                                            ),
                                        )
                                        .children(if nodes.is_empty() {
                                            vec![div()
                                                .p(px(40.0))
                                                .flex()
                                                .flex_col()
                                                .items_center()
                                                .gap(px(8.0))
                                                .text_color(theme.text_tertiary)
                                                .child(cursor_icon("brain", 28.0, theme.text_tertiary))
                                                .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child("Learning graph is building"))
                                                .child("As you work with Hermes, learned skills and memories appear here.")
                                                .into_any_element()]
                                        } else {
                                            nodes
                                                .into_iter()
                                                .map(|node| {
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap(px(8.0))
                                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(node.label))
                                                                        .child(gb_badge(&theme, BadgeTone::Accent, node.kind)),
                                                                )
                                                                .when_some(node.summary, |d, sum| {
                                                                    d.child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child(sum))
                                                                }),
                                                        )
                                                        .into_any_element()
                                                })
                                                .collect()
                                        }),
                                )
                            }),
                    ),
            )
    }
}
