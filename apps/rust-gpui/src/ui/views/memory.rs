use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_row, gb_section_card, gb_section_card_head,
    gb_segmented_item, gb_segmented_shell, BadgeTone, BtnVariant, FONT_SANS,
};
use crate::ui::motion::{gpu_spinner, view_enter, EntranceKey};
use crate::ui::smooth_scroll::{smooth_scroll_layer, NestedScrollRegions, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;

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
    tab_entrance: EntranceKey<MemoryTab>,
    graph_loading: bool,
    graph_source: Option<u64>,
    graph_error: Option<String>,
    curator_busy: bool,
    mem_input: Entity<TextareaState>,
    user_input: Entity<TextareaState>,
    soul_input: Entity<TextareaState>,
    synced_mem: String,
    synced_user: String,
    synced_soul: String,
    scroll_handle: ScrollHandle,
    editor_regions: NestedScrollRegions,
    smooth_scroller: SmoothScroller,
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
        cx.observe(&app, |_, app_e, cx| {
            let app = app_e.read(cx);
            if !app.setup_open && !app.initializing && app.active_view == AppViewId::Memory {
                cx.notify();
            }
        })
        .detach();
        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());
        Self {
            app,
            tab: MemoryTab::Memory,
            tab_entrance: EntranceKey::new(),
            graph_loading: false,
            graph_source: None,
            graph_error: None,
            curator_busy: false,
            mem_input,
            user_input,
            soul_input,
            synced_mem: m,
            synced_user: u,
            synced_soul: so,
            scroll_handle,
            editor_regions: NestedScrollRegions::default(),
            smooth_scroller,
        }
    }

    fn load_graph(&mut self, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let source = app.profile_generation;
        if self.graph_loading && self.graph_source == Some(source) {
            return;
        }
        self.graph_source = Some(source);
        self.graph_error = None;
        self.graph_loading = true;
        let rt = app.tokio_rt.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = rt
                .spawn(async move { client.get_learning_graph().await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                if view.graph_source != Some(source) {
                    return;
                }
                view.graph_loading = false;
                if view.app.read(cx).profile_generation == source {
                    match result {
                        Ok(graph) => view.app.update(cx, |app, cx| {
                            app.learning_graph = Some(graph);
                            cx.notify();
                        }),
                        Err(error) => view.graph_error = Some(error),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn run_curator(&mut self, cx: &mut Context<Self>) {
        if self.curator_busy {
            return;
        }
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let source = app.profile_generation;
        let rt = app.tokio_rt.clone();
        self.curator_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = rt
                .spawn(async move { client.run_curator().await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                view.curator_busy = false;
                cx.notify();
                if view.app.read(cx).profile_generation != source {
                    return;
                }
                match result {
                    Ok(_) => {
                        view.app.update(cx, |app, cx| {
                            app.show_toast("Triggered Hermes memory curator", cx)
                        });
                        if view.tab == MemoryTab::Graph {
                            view.load_graph(cx);
                        }
                    }
                    Err(error) => view.app.update(cx, |app, cx| app.show_toast(error, cx)),
                }
            });
        })
        .detach();
    }
}

impl Render for MemoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (theme, is_local, mem, usr, soul, source, has_client) = {
            let s = self.app.read(cx);
            (
                s.sand_theme(),
                s.connection.is_local(),
                s.memory_md.clone(),
                s.user_md.clone(),
                s.soul_md.clone(),
                s.profile_generation,
                s.client.is_some(),
            )
        };
        if self.synced_mem != mem && self.mem_input.read(cx).value().as_ref() == self.synced_mem {
            self.synced_mem = mem.clone();
            self.mem_input
                .update(cx, |st, cx| st.set_value(mem, window, cx));
        }
        if self.synced_user != usr && self.user_input.read(cx).value().as_ref() == self.synced_user
        {
            self.synced_user = usr.clone();
            self.user_input
                .update(cx, |st, cx| st.set_value(usr, window, cx));
        }
        if self.synced_soul != soul && self.soul_input.read(cx).value().as_ref() == self.synced_soul
        {
            self.synced_soul = soul.clone();
            self.soul_input
                .update(cx, |st, cx| st.set_value(soul, window, cx));
        }
        if self.tab == MemoryTab::Graph && has_client && self.graph_source != Some(source) {
            self.load_graph(cx);
        }
        let tab_id = self.tab_entrance.id("memory-tab", self.tab);

        div()
            .size_full()
            .min_w_0()
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
                            .flex_wrap()
                            .gap(px(8.0))
                            .child(
                                gb_segmented_shell(&theme).flex_wrap()
                                    .child(
                                        gb_segmented_item("mem-t-mem", &theme, self.tab == MemoryTab::Memory, "MEMORY.md")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::Memory;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("mem-t-user", &theme, self.tab == MemoryTab::User, "USER.md")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::User;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("mem-t-soul", &theme, self.tab == MemoryTab::Soul, "SOUL.md")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::Soul;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("mem-t-graph", &theme, self.tab == MemoryTab::Graph, "Learning Graph")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = MemoryTab::Graph;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                this.load_graph(cx);
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .when(has_client, |d| {
                                d.child(
                                    gb_btn("mem-curator-btn", &theme, BtnVariant::Secondary, false, self.curator_busy)
                                        .child(cursor_icon("sparkles", 13.0, theme.text_primary))
                                        .child("Run Curator")
                                        .on_click(cx.listener(|this, _, _, cx| this.run_curator(cx))),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(self.editor_regions.reset_marker())
                    .child(
                        div()
                            .id("memory-scroll")
                            .track_scroll(&self.scroll_handle)
                            .size_full()
                            .overflow_y_scroll()
                            .px(px(24.0))
                    .py(px(20.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(view_enter(tab_id,
                        div()
                            .w(px(920.0))
                            .max_w_full()
                            .flex_shrink_0()
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
                                                .relative()
                                                .child(self.editor_regions.mark_nested())
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
                                                .relative()
                                                .child(self.editor_regions.mark_nested())
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
                                                .relative()
                                                .child(self.editor_regions.mark_nested())
                                                .child(Textarea::new(&self.soul_input).h(px(380.0))),
                                        ),
                                )
                            })
                            .when(self.tab == MemoryTab::Graph, |d| {
                                let loaded = has_client && self.graph_source == Some(source)
                                    && !self.graph_loading && self.graph_error.is_none();
                                let nodes = if loaded {
                                    self.app.read(cx).learning_graph.as_ref()
                                        .map_or(&[][..], |graph| graph.nodes.as_slice())
                                } else {
                                    &[]
                                };
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
                                                .when(self.graph_loading, |d| d.child(gpu_spinner(28.0, theme.text_tertiary)))
                                                .when(!self.graph_loading, |d| d.child(cursor_icon("brain", 28.0, theme.text_tertiary)))
                                                .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child(
                                                    if !has_client { "Connect to Hermes to view the learning graph." }
                                                    else if self.graph_loading { "Loading learning graph…" }
                                                    else if self.graph_error.is_some() { "Could not load the learning graph." }
                                                    else { "No learned skills or memories yet." }
                                                ))
                                                .when_some(self.graph_error.as_deref(), |view, error| {
                                                    view.child(error.to_owned())
                                                        .child(gb_btn("mem-retry-graph", &theme, BtnVariant::Secondary, true, false)
                                                            .child("Retry")
                                                            .on_click(cx.listener(|this, _, _, cx| this.load_graph(cx))))
                                                })
                                                .into_any_element()]
                                        } else {
                                            nodes
                                                .iter()
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
                                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(node.label.clone()))
                                                                        .child(gb_badge(&theme, BadgeTone::Accent, node.kind.clone())),
                                                                )
                                                                .when_some(node.summary.clone(), |d, sum| {
                                                                    d.child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child(sum))
                                                                }),
                                                        )
                                                        .into_any_element()
                                                })
                                                .collect()
                                        }),
                                )
                            }),
                    )),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(smooth_scroll_layer(&self.smooth_scroller).yield_to(&self.editor_regions)),
            )
    }
}
