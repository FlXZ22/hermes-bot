use crate::hermes::types::HermesMcpServer;
use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_mini_icon_btn, gb_row, gb_section_card, gb_segmented_item,
    gb_segmented_shell, gb_toggle_switch, BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::SandTheme;
use crate::ui::motion::{gpu_spinner, ModalMotion};
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillsTab {
    Skills,
    Hub,
    Toolsets,
    Mcp,
}

pub struct SkillsView {
    app: Entity<AppState>,
    tab: SkillsTab,
    search_input: Entity<InputState>,
    hub_input: Entity<InputState>,
    hub_results: Vec<Value>,
    hub_loading: bool,
    hub_request: u64,
    hub_source: Option<u64>,
    // Skill editor modal
    editing_skill: Option<String>,
    skill_modal: ModalMotion,
    skill_editor: Entity<TextareaState>,
    skill_request: u64,
    skill_source: Option<u64>,
    skill_loaded: bool,
    skill_saving: bool,
    mcp_saving: bool,
    // Add MCP modal
    mcp_modal: ModalMotion,
    mcp_name: Entity<InputState>,
    mcp_transport: String,
    mcp_command: Entity<InputState>,
    mcp_args: Entity<InputState>,
    mcp_url: Entity<InputState>,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
}

impl SkillsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Filter installed Hermes skills..."));
        let hub_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search Hermes Skills Hub (e.g. git, docker, arch, research)...")
        });
        let skill_editor = cx.new(|cx| TextareaState::new(window, cx).rows(14));
        let mcp_name =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. filesystem, github, linear"));
        let mcp_command = cx.new(|cx| InputState::new(window, cx).placeholder("npx"));
        let mcp_args = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("-y @modelcontextprotocol/server-filesystem /home")
        });
        let mcp_url = cx.new(|cx| InputState::new(window, cx).placeholder("https://..."));

        cx.observe(&app, |this, app_e, cx| {
            let app = app_e.read(cx);
            if !app.setup_open && !app.initializing && app.active_view == AppViewId::Skills {
                if this
                    .hub_source
                    .is_some_and(|source| source != app.profile_generation)
                {
                    this.hub_source = None;
                    this.hub_results.clear();
                    this.hub_loading = false;
                }
                if this
                    .skill_source
                    .is_some_and(|source| source != app.profile_generation)
                {
                    this.close_skill_editor();
                }
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&search_input, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&hub_input, |this, _, ev: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.search_hub(cx);
            }
        })
        .detach();

        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());

        Self {
            app,
            tab: SkillsTab::Skills,
            search_input,
            hub_input,
            hub_results: Vec::new(),
            hub_loading: false,
            hub_request: 0,
            hub_source: None,
            editing_skill: None,
            skill_modal: ModalMotion::default(),
            skill_editor,
            skill_request: 0,
            skill_source: None,
            skill_loaded: false,
            skill_saving: false,
            mcp_saving: false,
            mcp_modal: ModalMotion::default(),
            mcp_name,
            mcp_transport: "stdio".into(),
            mcp_command,
            mcp_args,
            mcp_url,
            scroll_handle,
            smooth_scroller,
        }
    }

    fn search_hub(&mut self, cx: &mut Context<Self>) {
        let query = self.hub_input.read(cx).value().trim().to_string();
        if query.is_empty() || self.hub_loading {
            return;
        }
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let rt = app.tokio_rt.clone();
        let source = app.profile_generation;
        self.hub_source = Some(source);
        self.hub_request = self.hub_request.wrapping_add(1);
        let request = self.hub_request;
        self.hub_loading = true;
        self.hub_results.clear();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = rt
                .spawn(async move { client.search_skills_hub(&query).await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                if view.hub_request != request {
                    return;
                }
                view.hub_loading = false;
                if view.app.read(cx).profile_generation == source {
                    match result {
                        Ok(items) => view.hub_results = items,
                        Err(error) => view.app.update(cx, |app, cx| app.show_toast(error, cx)),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_skill_editor(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let rt = app.tokio_rt.clone();
        let source = app.profile_generation;
        self.skill_source = Some(source);
        self.editing_skill = Some(name.clone());
        self.skill_modal.set_open(true);
        self.skill_loaded = false;
        self.skill_saving = false;
        self.skill_request = self.skill_request.wrapping_add(1);
        let request = self.skill_request;
        self.skill_editor
            .update(cx, |state, cx| state.set_value("Loading…", window, cx));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = rt
                .spawn(async move { client.get_skill_content(&name).await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update_in(cx, |view, window, cx| {
                if view.skill_request != request || view.app.read(cx).profile_generation != source {
                    return;
                }
                match result {
                    Ok(content) => {
                        view.skill_loaded = true;
                        view.skill_editor
                            .update(cx, |state, cx| state.set_value(content, window, cx));
                        let focus = view.skill_editor.read(cx).focus_handle(cx);
                        window.focus(&focus, cx);
                    }
                    Err(error) => {
                        view.skill_editor.update(cx, |state, cx| {
                            state.set_value("Could not load this skill.", window, cx)
                        });
                        view.app.update(cx, |app, cx| app.show_toast(error, cx));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_skill_editor(&mut self) {
        self.skill_request = self.skill_request.wrapping_add(1);
        self.skill_source = None;
        self.editing_skill = None;
        self.skill_loaded = false;
        self.skill_saving = false;
        self.skill_modal.set_open(false);
    }

    fn save_skill(&mut self, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        if !self.skill_loaded
            || self.skill_saving
            || self.skill_source != Some(app.profile_generation)
        {
            return;
        }
        let (Some(client), Some(name)) = (app.client.clone(), self.editing_skill.clone()) else {
            return;
        };
        let source = app.profile_generation;
        let rt = app.tokio_rt.clone();
        let content = self.skill_editor.read(cx).value().to_string();
        let request = self.skill_request;
        self.skill_saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = rt
                .spawn(async move { client.save_skill_content(&name, &content).await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                if view.skill_request != request || view.app.read(cx).profile_generation != source {
                    return;
                }
                view.skill_saving = false;
                match result {
                    Ok(_) => {
                        view.close_skill_editor();
                        view.app.update(cx, |app, cx| {
                            app.refresh_skills(cx);
                            app.show_toast("Saved skill", cx);
                        });
                    }
                    Err(error) => view.app.update(cx, |app, cx| app.show_toast(error, cx)),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for SkillsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_height = (window.viewport_size().height - px(48.0)).max(px(0.0));
        let s = self.app.read(cx);
        let theme = s.sand_theme();
        let skills = s.skills.clone();
        let toolsets = s.toolsets.clone();
        let mcps = s.mcp_servers.clone();
        let q = self.search_input.read(cx).value().trim().to_lowercase();
        let filtered_skills: Vec<_> = skills
            .iter()
            .filter(|sk| {
                q.is_empty()
                    || format!(
                        "{} {} {}",
                        sk.name,
                        sk.category.as_deref().unwrap_or(""),
                        sk.description.as_deref().unwrap_or("")
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .cloned()
            .collect();

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
            // Header
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
                            .child(div().text_size(px(16.0)).font_weight(FontWeight::SEMIBOLD).child("Capabilities & MCP"))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(theme.text_tertiary)
                                    .child(format!(
                                        "{} skills · {} toolsets · {} MCP servers",
                                        skills.len(),
                                        toolsets.len(),
                                        mcps.len()
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                gb_segmented_shell(&theme).flex_wrap()
                                    .child(
                                        gb_segmented_item("sk-tab-skills", &theme, self.tab == SkillsTab::Skills, "Skills")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = SkillsTab::Skills;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("sk-tab-hub", &theme, self.tab == SkillsTab::Hub, "Hub")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = SkillsTab::Hub;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("sk-tab-tools", &theme, self.tab == SkillsTab::Toolsets, "Toolsets")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = SkillsTab::Toolsets;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gb_segmented_item("sk-tab-mcp", &theme, self.tab == SkillsTab::Mcp, "MCP")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.tab = SkillsTab::Mcp;
                                                this.smooth_scroller.cancel();
                                                this.scroll_handle.set_offset(Point::default());
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .when(self.tab == SkillsTab::Mcp, |d| {
                                d.child(
                                    gb_btn("sk-add-mcp", &theme, BtnVariant::Primary, false, false)
                                        .child(cursor_icon("plus", 13.0, theme.text_on_primary))
                                        .child("Add MCP")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mcp_modal.set_open(true);
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
            // Body
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(
                        div()
                            .id("skills-body-scroll")
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
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .when(self.tab == SkillsTab::Skills, |d| {
                                d.child(Input::new(&self.search_input).h(px(36.0)))
                                    .child(gb_section_card(&theme).children(
                                        if filtered_skills.is_empty() {
                                            vec![div()
                                                .p(px(36.0))
                                                .flex()
                                                .flex_col()
                                                .items_center()
                                                .gap(px(8.0))
                                                .text_color(theme.text_tertiary)
                                                .child(cursor_icon("sparkles", 26.0, theme.text_tertiary))
                                                .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child("No skills found"))
                                                .child("Install skills from the Hub or ask Hermes to write one for you.")
                                                .into_any_element()]
                                        } else {
                                            filtered_skills
                                                .iter()
                                                .map(|sk| {
                                                    let name_ed = sk.name.clone();
                                                    let name_tog = sk.name.clone();
                                                    let next_en = !sk.enabled;
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap(px(8.0))
                                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(sk.name.clone()))
                                                                        .when_some(sk.category.as_ref().filter(|c| !c.is_empty()), |d, cat| {
                                                                            d.child(gb_badge(&theme, BadgeTone::Muted, cat.clone()))
                                                                        }),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.0))
                                                                        .text_color(theme.text_tertiary)
                                                                        .child(
                                                                            sk.description
                                                                                .clone()
                                                                                .or_else(|| sk.path.clone())
                                                                                .unwrap_or_default(),
                                                                        ),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap(px(10.0))
                                                                .child(
                                                                    gb_btn(
                                                                        SharedString::from(format!("sk-ed-{}", sk.name)),
                                                                        &theme,
                                                                        BtnVariant::Secondary,
                                                                        true,
                                                                        false,
                                                                    )
                                                                    .child("Edit")
                                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                                        this.open_skill_editor(name_ed.clone(), window, cx);
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_toggle_switch(
                                                                        SharedString::from(format!("sk-tog-{}", sk.name)),
                                                                        &theme,
                                                                        sk.enabled,
                                                                    )
                                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                                        this.app.update(cx, |s, cx| {
                                                                            if s.client.is_some() { s.toggle_skill_optimistic(name_tog.clone(), next_en, cx); }
                                                                        });
                                                                    })),
                                                                ),
                                                        )
                                                        .into_any_element()
                                                })
                                                .collect()
                                        },
                                    ))
                            })
                            .when(self.tab == SkillsTab::Hub, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .gap(px(8.0))
                                        .child(div().flex_1().child(Input::new(&self.hub_input).h(px(36.0))))
                                        .child(
                                            gb_btn("hub-search-btn", &theme, BtnVariant::Primary, false, self.hub_loading)
                                                .when(self.hub_loading, |d| d.child(gpu_spinner(13.0, theme.text_on_primary)))
                                                .child(if self.hub_loading { "Searching..." } else { "Search Hub" })
                                                .on_click(cx.listener(|this, _, _, cx| this.search_hub(cx))),
                                        ),
                                )
                                .child(gb_section_card(&theme).children(
                                    if self.hub_results.is_empty() {
                                        vec![div()
                                            .p(px(36.0))
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(8.0))
                                            .text_color(theme.text_tertiary)
                                            .child(cursor_icon("search", 26.0, theme.text_tertiary))
                                            .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child("Discover Community Skills"))
                                            .child("Search Hermes Skills Hub to preview and install skills in one click.")
                                            .into_any_element()]
                                    } else {
                                        self.hub_results
                                            .iter()
                                            .enumerate()
                                            .map(|(i, item)| {
                                                let id = item
                                                    .get("identifier")
                                                    .or_else(|| item.get("name"))
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("skill")
                                                    .to_string();
                                                let name = item
                                                    .get("name")
                                                    .and_then(Value::as_str)
                                                    .unwrap_or(&id)
                                                    .to_string();
                                                let desc = item
                                                    .get("description")
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("")
                                                    .to_string();
                                                let id_inst = id.clone();
                                                gb_row(&theme)
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(name))
                                                            .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child(desc)),
                                                    )
                                                    .child(
                                                        gb_btn(
                                                            SharedString::from(format!("hub-inst-{i}")),
                                                            &theme,
                                                            BtnVariant::Primary,
                                                            true,
                                                            false,
                                                        )
                                                        .child("Install")
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            let (client, rt) = {
                                                                let s = this.app.read(cx);
                                                                (s.client.clone(), s.tokio_rt.clone())
                                                            };
                                                            let id2 = id_inst.clone();
                                                            if let Some(c) = client {
                                                                cx.spawn(async move |this, cx| {
                                                                    let id3 = id2.clone();
                                                                    let result = rt.spawn(async move { c.install_hub_skill(&id3).await })
                                                                        .await.unwrap_or_else(|e| Err(e.to_string()));
                                                                    let _ = this.update(cx, |v, cx| {
                                                                        v.app.update(cx, |s, cx| match result {
                                                                            Ok(_) => {
                                                                                s.refresh_skills(cx);
                                                                                s.show_toast(format!("Installed {id2}"), cx);
                                                                            }
                                                                            Err(e) => s.show_toast(e, cx),
                                                                        });
                                                                    });
                                                                })
                                                                .detach();
                                                            }
                                                        })),
                                                    )
                                                    .into_any_element()
                                            })
                                            .collect()
                                    },
                                ))
                            })
                            .when(self.tab == SkillsTab::Toolsets, |d| {
                                d.child(gb_section_card(&theme).children(
                                    if toolsets.is_empty() {
                                        vec![div()
                                            .p(px(36.0))
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(8.0))
                                            .text_color(theme.text_tertiary)
                                            .child(cursor_icon("wrench", 26.0, theme.text_tertiary))
                                            .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child("No toolsets loaded yet"))
                                            .into_any_element()]
                                    } else {
                                        toolsets
                                            .iter()
                                            .map(|ts| {
                                                let tname = ts.name.clone();
                                                let next = !ts.enabled;
                                                let desc = ts.description.clone().unwrap_or_else(|| {
                                                    ts.tools
                                                        .as_ref()
                                                        .map(|list| {
                                                            format!(
                                                                "{} tools: {}",
                                                                list.len(),
                                                                list.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
                                                            )
                                                        })
                                                        .unwrap_or_default()
                                                });
                                                gb_row(&theme)
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(ts.name.clone()))
                                                            .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child(desc)),
                                                    )
                                                    .child(
                                                        gb_toggle_switch(
                                                            SharedString::from(format!("ts-tog-{}", ts.name)),
                                                            &theme,
                                                            ts.enabled,
                                                        )
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.app.update(cx, |s, cx| {
                                                                if s.client.is_some() { s.toggle_toolset_optimistic(tname.clone(), next, cx); }
                                                            });
                                                        })),
                                                    )
                                                    .into_any_element()
                                            })
                                            .collect()
                                    },
                                ))
                            })
                            .when(self.tab == SkillsTab::Mcp, |d| {
                                d.child(gb_section_card(&theme).children(
                                    if mcps.is_empty() {
                                        vec![div()
                                            .p(px(36.0))
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(10.0))
                                            .text_color(theme.text_tertiary)
                                            .child(cursor_icon("plug", 26.0, theme.text_tertiary))
                                            .child(div().text_size(px(15.0)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text_primary).child("No MCP servers configured"))
                                            .child(
                                                gb_btn("mcp-empty-add", &theme, BtnVariant::Primary, false, false)
                                                    .child("Add MCP Server")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.mcp_modal.set_open(true);
                                                        cx.notify();
                                                    })),
                                            )
                                            .into_any_element()]
                                    } else {
                                        mcps.iter()
                                            .map(|srv| {
                                                let sname_test = srv.name.clone();
                                                let sname_tog = srv.name.clone();
                                                let sname_del = srv.name.clone();
                                                let enabled = srv.enabled.unwrap_or(true);
                                                let transport = srv
                                                    .transport
                                                    .clone()
                                                    .unwrap_or_else(|| if srv.url.is_some() { "http".into() } else { "stdio".into() });
                                                let detail = srv.url.clone().unwrap_or_else(|| {
                                                    format!(
                                                        "{} {}",
                                                        srv.command.as_deref().unwrap_or(""),
                                                        srv.args.clone().unwrap_or_default().join(" ")
                                                    )
                                                });
                                                gb_row(&theme)
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .child(
                                                                div()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap(px(8.0))
                                                                    .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child(srv.name.clone()))
                                                                    .child(gb_badge(&theme, BadgeTone::Muted, transport))
                                                                    .when_some(srv.status.as_ref(), |d, st| {
                                                                        d.child(gb_badge(&theme, BadgeTone::Info, st.clone()))
                                                                    }),
                                                            )
                                                            .child(
                                                                div()
                                                                    .font_family(FONT_MONO)
                                                                    .text_size(px(12.0))
                                                                    .text_color(theme.text_tertiary)
                                                                    .child(detail),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(8.0))
                                                            .child(
                                                                gb_btn(
                                                                    SharedString::from(format!("mcp-test-{}", srv.name)),
                                                                    &theme,
                                                                    BtnVariant::Secondary,
                                                                    true,
                                                                    false,
                                                                )
                                                                .child("Test")
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    let (client, rt) = {
                                                                        let s = this.app.read(cx);
                                                                        (s.client.clone(), s.tokio_rt.clone())
                                                                    };
                                                                    let n = sname_test.clone();
                                                                    if let Some(c) = client {
                                                                        cx.spawn(async move |this, cx| {
                                                                            let n2 = n.clone();
                                                                            let res = rt.spawn(async move { c.test_mcp_server(&n2).await }).await.unwrap_or_else(|e| Err(e.to_string()));
                                                                            let _ = this.update(cx, |v, cx| {
                                                                                v.app.update(cx, |s, cx| match res {
                                                                                    Ok(_) => s.show_toast(format!("MCP \"{n}\" healthy"), cx),
                                                                                    Err(e) => s.show_toast(e, cx),
                                                                                });
                                                                            });
                                                                        })
                                                                        .detach();
                                                                    }
                                                                })),
                                                            )
                                                            .child(
                                                                gb_toggle_switch(
                                                                    SharedString::from(format!("mcp-tog-{}", srv.name)),
                                                                    &theme,
                                                                    enabled,
                                                                )
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    this.app.update(cx, |s, cx| {
                                                                        if s.client.is_some() { s.toggle_mcp_optimistic(sname_tog.clone(), !enabled, cx); }
                                                                    });
                                                                })),
                                                            )
                                                            .child(
                                                                gb_btn(
                                                                    SharedString::from(format!("mcp-del-{}", srv.name)),
                                                                    &theme,
                                                                    BtnVariant::Danger,
                                                                    true,
                                                                    false,
                                                                )
                                                                .child(cursor_icon("trash", 12.0, theme.danger_text))
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    this.app.update(cx, |s, cx| {
                                                                        if s.client.is_some() { s.delete_mcp_optimistic(sname_del.clone(), cx); }
                                                                    });
                                                                })),
                                                            ),
                                                    )
                                                    .into_any_element()
                                            })
                                            .collect()
                                    },
                                ))
                            }),
                    ),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(smooth_scroll_layer(&self.smooth_scroller)),
            )
            // Skill editor modal
            .when_some(self.editing_skill.clone(), |d, sk_name| {
                d.child(deferred(
                    div()
                        .id("sk-edit-backdrop")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .child(self.skill_modal.backdrop("skill-scrim-enter", theme.bg_scrim))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(20.0))
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                            this.close_skill_editor();
                            cx.notify();
                        }))
                        .child(self.skill_modal.card("skill-card-enter",
                            div()
                                .id("sk-edit-modal")
                                .w(px(600.0))
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
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(div().text_size(px(15.5)).font_weight(FontWeight::SEMIBOLD).child(format!("SKILL.md · {sk_name}")))
                                        .child(
                                            gb_mini_icon_btn("close-sk-ed", &theme, false)
                                                .child(cursor_icon("close", 14.0, theme.text_secondary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_skill_editor();
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .child(Textarea::new(&self.skill_editor).h(px(320.0)).readonly(!self.skill_loaded || self.skill_saving))
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .child(
                                            gb_btn("sk-ed-cancel", &theme, BtnVariant::Ghost, false, false)
                                                .child("Cancel")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_skill_editor();
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_btn("sk-ed-save", &theme, BtnVariant::Primary, false, !self.skill_loaded || self.skill_saving)
                                                .child("Save SKILL.md")
                                                .on_click(cx.listener(|this, _, _, cx| this.save_skill(cx))),
                                        ),
                                )
                                .overflow_y_scrollbar()
                        )),
                ))
            })
            // Add MCP Modal
            .when(self.mcp_modal.is_open(), |d| {
                d.child(self.render_mcp_modal(&theme, modal_height, cx))
            })
    }
}

impl SkillsView {
    fn render_mcp_modal(&self, theme: &SandTheme, modal_height: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let is_stdio = self.mcp_transport == "stdio";
        deferred(
            div()
                .id("mcp-modal-backdrop")
                .occlude()
                .absolute()
                .inset_0()
                .child(self.mcp_modal.backdrop("mcp-scrim-enter", theme.bg_scrim))
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.mcp_modal.set_open(false);
                    cx.notify();
                }))
                .child(self.mcp_modal.card("mcp-card-enter",
                    div()
                        .id("mcp-modal")
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
                        .child(div().text_size(px(15.5)).font_weight(FontWeight::SEMIBOLD).child("Add MCP Server"))
                        .child(Input::new(&self.mcp_name).h(px(36.0)).disabled(self.mcp_saving))
                        .child(
                            gb_segmented_shell(theme)
                                .child(
                                    gb_segmented_item("mcp-tr-stdio", theme, self.mcp_transport == "stdio", "stdio")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mcp_transport = "stdio".into();
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    gb_segmented_item("mcp-tr-sse", theme, self.mcp_transport == "sse", "SSE")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mcp_transport = "sse".into();
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    gb_segmented_item("mcp-tr-http", theme, self.mcp_transport == "streamable_http", "HTTP")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mcp_transport = "streamable_http".into();
                                            cx.notify();
                                        })),
                                ),
                        )
                        .when(is_stdio, |d| {
                            d.child(Input::new(&self.mcp_command).h(px(36.0)).disabled(self.mcp_saving))
                                .child(Input::new(&self.mcp_args).h(px(36.0)).disabled(self.mcp_saving))
                        })
                        .when(!is_stdio, |d| d.child(Input::new(&self.mcp_url).h(px(36.0)).disabled(self.mcp_saving)))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    gb_btn("mcp-cancel", theme, BtnVariant::Ghost, false, false)
                                        .child("Cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.mcp_modal.set_open(false);
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    gb_btn("mcp-add-submit", theme, BtnVariant::Primary, false, self.mcp_saving)
                                        .child("Add Server")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if this.mcp_saving {
                                                return;
                                            }
                                            let name = this.mcp_name.read(cx).value().trim().to_string();
                                            let transport = this.mcp_transport.clone();
                                            let command = this.mcp_command.read(cx).value().trim().to_string();
                                            let args = this.mcp_args.read(cx).value().trim().to_string();
                                            let url = this.mcp_url.read(cx).value().trim().to_string();
                                            let stdio = transport == "stdio";
                                            if name.is_empty() || (stdio && command.is_empty()) || (!stdio && url.is_empty()) {
                                                this.app.update(cx, |app, cx| app.show_toast("Enter a server name and a command or URL.", cx));
                                                return;
                                            }
                                            let app = this.app.read(cx);
                                            let Some(client) = app.client.clone() else {
                                                return;
                                            };
                                            let source = app.profile_generation;
                                            let rt = app.tokio_rt.clone();
                                            let server = HermesMcpServer {
                                                name,
                                                transport: Some(transport),
                                                command: stdio.then_some(command),
                                                args: (stdio && !args.is_empty()).then(|| args.split_whitespace().map(String::from).collect()),
                                                url: (!stdio).then_some(url),
                                                enabled: Some(true),
                                                status: None,
                                            };
                                            this.mcp_saving = true;
                                            cx.notify();
                                            cx.spawn_in(window, async move |this, cx| {
                                                let result = rt.spawn(async move { client.add_mcp_server(&server).await })
                                                    .await.unwrap_or_else(|error| Err(error.to_string()));
                                                let _ = this.update_in(cx, |view, window, cx| {
                                                    view.mcp_saving = false;
                                                    cx.notify();
                                                    if view.app.read(cx).profile_generation != source {
                                                        return;
                                                    }
                                                    match result {
                                                        Ok(_) => {
                                                            view.mcp_modal.set_open(false);
                                                            for input in [&view.mcp_name, &view.mcp_command, &view.mcp_args, &view.mcp_url] {
                                                                input.update(cx, |state, cx| state.set_value("", window, cx));
                                                            }
                                                            view.app.update(cx, |app, cx| {
                                                                app.refresh_mcp_servers(cx);
                                                                app.show_toast("Added MCP server", cx);
                                                            });
                                                        }
                                                        Err(error) => view.app.update(cx, |app, cx| app.show_toast(error, cx)),
                                                    }
                                                });
                                            }).detach();
                                        })),
                                ),
                        )
                                .overflow_y_scrollbar()
                )),
        )
    }
}
