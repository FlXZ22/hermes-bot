use crate::hermes::types::HermesModelOption;
use crate::state::app_state::{AppState, ReasoningEffort};
use crate::theme::components::{
    cursor_icon, format_tokens, gb_badge, gb_board_pill, gb_btn, gb_mini_icon_btn, gb_row,
    gb_section_card, gb_segmented_item, gb_segmented_shell, gb_toggle_switch, BadgeTone,
    BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::SandTheme;
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use std::collections::BTreeMap;

pub struct ModelPickerModal {
    app: Entity<AppState>,
    search_input: Entity<InputState>,
    custom_input: Entity<InputState>,
    provider_filter: String,
    save_default: bool,
    list_scroll: UniformListScrollHandle,
    list_smooth: SmoothScroller,
    visible_models: Vec<HermesModelOption>,
}

impl ModelPickerModal {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search models or providers (e.g. anti-gravity, claude, grok, gemini)...")
        });
        let custom_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Custom model ID (e.g. openrouter/anthropic/claude-opus-4.6)")
        });
        cx.observe(&app, |_, app_e, cx| {
            if app_e.read(cx).model_picker_open {
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
        cx.subscribe_in(&custom_input, window, |this, inp, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                let val = inp.read(cx).value().trim().to_string();
                if !val.is_empty() {
                    let save_def = this.save_default;
                    inp.update(cx, |st, cx| st.set_value("", window, cx));
                    this.app.update(cx, |s, cx| {
                        let prov = s.active_provider.clone();
                        s.select_model(val, Some(prov), save_def, cx);
                        s.model_picker_open = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();

        let list_scroll = UniformListScrollHandle::new();
        let list_smooth = SmoothScroller::for_uniform_list(list_scroll.clone());

        Self {
            app,
            search_input,
            custom_input,
            provider_filter: "all".into(),
            save_default: false,
            list_scroll,
            list_smooth,
            visible_models: Vec::new(),
        }
    }
}

impl Render for ModelPickerModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.app.read(cx);
        if !s.model_picker_open {
            return div().into_any_element();
        }
        let theme = s.sand_theme();
        let models = s.models.clone();
        let active_model = s.active_model.clone();
        let active_provider = s.active_provider.clone();
        let effort = s.reasoning_effort;
        let fast = s.fast_mode;
        let yolo = s.yolo_mode;
        let refreshing = s.refreshing_models;
        let restarting = s.restarting_backend;

        let mut prov_map: BTreeMap<String, (usize, bool)> = BTreeMap::new();
        for m in &models {
            if m.provider.is_empty() {
                continue;
            }
            let e = prov_map.entry(m.provider.clone()).or_insert((0, false));
            e.0 += 1;
            if m.configured {
                e.1 = true;
            }
        }
        let mut providers: Vec<(String, usize, bool)> = prov_map
            .into_iter()
            .map(|(k, (c, cfg))| (k, c, cfg))
            .collect();
        providers.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        providers.insert(0, ("all".into(), models.len(), true));

        let q = self.search_input.read(cx).value().trim().to_lowercase();
        let mut filtered: Vec<_> = models
            .iter()
            .filter(|m| {
                if self.provider_filter != "all" && m.provider != self.provider_filter {
                    return false;
                }
                if q.is_empty() {
                    return true;
                }
                format!(
                    "{} {} {}",
                    m.id,
                    m.name.as_deref().unwrap_or(""),
                    m.provider
                )
                .to_lowercase()
                .contains(&q)
            })
            .cloned()
            .collect();
        filtered.sort_by(|a, b| b.configured.cmp(&a.configured));
        let model_count = filtered.len();
        self.visible_models = filtered;

        let save_def = self.save_default;

        deferred(
            div()
                .id("model-picker-backdrop")
                .absolute()
                .inset_0()
                .bg(theme.bg_scrim)
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.0))
                .font_family(FONT_SANS)
                .text_color(theme.text_primary)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.app.update(cx, |s, cx| {
                        s.model_picker_open = false;
                        cx.notify();
                    });
                }))
                .child(
                    div()
                        .id("model-picker-modal")
                        .w(px(640.0))
                        .max_w_full()
                        .max_h(px(680.0))
                        .rounded(px(22.0))
                        .bg(theme.bg_elevated)
                        .border_1()
                        .border_color(theme.border_default)
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        // Header
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
                                        .child("Choose Hermes Model"),
                                )
                                .child(
                                    gb_mini_icon_btn("close-model-picker", &theme, false)
                                        .child(cursor_icon("close", 14.0, theme.text_secondary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| {
                                                s.model_picker_open = false;
                                                cx.notify();
                                            });
                                        })),
                                ),
                        )
                        // Scrollable Body
                        .child(
                            div()
                                .id("model-picker-body")
                                .flex_1()
                                .overflow_y_scroll()
                                .p(px(16.0))
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                // Search + Refresh/Restart row
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(div().flex_1().child(Input::new(&self.search_input).h(px(34.0))))
                                        .child(
                                            gb_btn(
                                                "mp-refresh",
                                                &theme,
                                                BtnVariant::Secondary,
                                                true,
                                                refreshing || restarting,
                                            )
                                            .child(cursor_icon("refresh", 12.0, theme.text_primary))
                                            .child(if refreshing { "Refreshing…" } else { "Refresh" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |s, cx| s.refresh_models(cx));
                                            })),
                                        )
                                        .child(
                                            gb_btn(
                                                "mp-restart",
                                                &theme,
                                                BtnVariant::Secondary,
                                                true,
                                                restarting,
                                            )
                                            .child(cursor_icon("refresh", 12.0, theme.text_primary))
                                            .child(if restarting { "Restarting…" } else { "Restart" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |s, cx| s.restart_backend(cx));
                                            })),
                                        ),
                                )
                                // Provider chips
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(5.0))
                                        .children(providers.into_iter().map(|(name, count, _)| {
                                            let active = self.provider_filter == name;
                                            let label = if name == "all" {
                                                format!("All Providers ({count})")
                                            } else {
                                                format!("{name} ({count})")
                                            };
                                            let pname = name.clone();
                                            gb_board_pill(
                                                SharedString::from(format!("mp-prov-{name}")),
                                                &theme,
                                                active,
                                            )
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.provider_filter = pname.clone();
                                                cx.notify();
                                            }))
                                        })),
                                )
                                // Virtualized 120 FPS Model list (uniform_list + SmoothScroller + scrollbar)
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(260.0))
                                        .relative()
                                        .child(if model_count == 0 {
                                            div()
                                                .py(px(18.0))
                                                .text_center()
                                                .text_size(px(13.0))
                                                .text_color(theme.text_tertiary)
                                                .child("No matching models. Enter a custom model ID below.")
                                                .into_any_element()
                                        } else {
                                            let theme_u = theme;
                                            let active_m = active_model.clone();
                                            let active_p = active_provider.clone();
                                            uniform_list(
                                                "mp-list-uniform",
                                                model_count,
                                                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                                                    let mut out = Vec::with_capacity(range.len());
                                                    for idx in range {
                                                        let Some(m) = this.visible_models.get(idx) else {
                                                            continue;
                                                        };
                                                        let selected = m.id == active_m
                                                            && (active_p.is_empty()
                                                                || active_p == "auto"
                                                                || m.provider == active_p);
                                                        let mid = m.id.clone();
                                                        let mprov = m.provider.clone();
                                                        let sub = if let Some(ctx) = m.context_length {
                                                            format!("{} · {} ctx", m.id, format_tokens(ctx))
                                                        } else {
                                                            m.id.clone()
                                                        };
                                                        out.push(
                                                            div()
                                                                .id(SharedString::from(format!("mp-row-{idx}")))
                                                                .w_full()
                                                                .h(px(54.0))
                                                                .px(px(11.0))
                                                                .rounded(px(11.0))
                                                                .bg(if selected {
                                                                    theme_u.accent_soft
                                                                } else {
                                                                    SandTheme::transparent()
                                                                })
                                                                .flex()
                                                                .items_center()
                                                                .justify_between()
                                                                .gap(px(10.0))
                                                                .cursor_pointer()
                                                                .hover(move |st| st.bg(theme_u.bg_hover))
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    this.app.update(cx, |s, cx| {
                                                                        s.select_model(
                                                                            mid.clone(),
                                                                            Some(mprov.clone()),
                                                                            save_def,
                                                                            cx,
                                                                        );
                                                                        s.model_picker_open = false;
                                                                        cx.notify();
                                                                    });
                                                                }))
                                                                .child(
                                                                    div()
                                                                        .flex_1()
                                                                        .min_w_0()
                                                                        .flex()
                                                                        .flex_col()
                                                                        .gap(px(2.0))
                                                                        .child(
                                                                            div()
                                                                                .flex()
                                                                                .items_center()
                                                                                .gap(px(7.0))
                                                                                .child(
                                                                                    div()
                                                                                        .text_size(px(13.5))
                                                                                        .font_weight(FontWeight::MEDIUM)
                                                                                        .truncate()
                                                                                        .child(
                                                                                            m.name
                                                                                                .clone()
                                                                                                .unwrap_or_else(|| m.id.clone()),
                                                                                        ),
                                                                                )
                                                                                .child(gb_badge(&theme_u, BadgeTone::Muted, m.provider.clone()))
                                                                                .when(m.configured, |d| {
                                                                                    d.child(gb_badge(&theme_u, BadgeTone::Success, "Ready"))
                                                                                }),
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .font_family(FONT_MONO)
                                                                                .text_size(px(11.5))
                                                                                .text_color(theme_u.text_tertiary)
                                                                                .truncate()
                                                                                .child(sub),
                                                                        ),
                                                                )
                                                                .when(selected, |d| {
                                                                    d.child(cursor_icon("check-circle", 16.0, theme_u.accent_text))
                                                                }),
                                                        );
                                                    }
                                                    out
                                                }),
                                            )
                                            .size_full()
                                            .track_scroll(&self.list_scroll)
                                            .into_any_element()
                                        })
                                        .vertical_scrollbar(&self.list_scroll)
                                        .child(smooth_scroll_layer(&self.list_smooth)),
                                )
                                // Custom model entry
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(div().flex_1().child(Input::new(&self.custom_input).h(px(34.0))))
                                        .child(
                                            gb_btn("mp-use-custom", &theme, BtnVariant::Secondary, false, false)
                                                .child("Use")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    let val = this.custom_input.read(cx).value().trim().to_string();
                                                    if !val.is_empty() {
                                                        let save_def = this.save_default;
                                                        this.custom_input.update(cx, |st, cx| st.set_value("", window, cx));
                                                        this.app.update(cx, |s, cx| {
                                                            let prov = s.active_provider.clone();
                                                            s.select_model(val, Some(prov), save_def, cx);
                                                            s.model_picker_open = false;
                                                            cx.notify();
                                                        });
                                                    }
                                                })),
                                        ),
                                )
                                // Reasoning Effort & Runtime Controls Card
                                .child(
                                    gb_section_card(&theme)
                                        .child(
                                            gb_row(&theme)
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Reasoning Effort"))
                                                        .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child("Controls extended thinking depth per turn")),
                                                )
                                                .child(gb_segmented_shell(&theme).children(
                                                    ReasoningEffort::ALL.iter().map(|&ef| {
                                                        gb_segmented_item(
                                                            SharedString::from(format!("mp-eff-{}", ef.as_str())),
                                                            &theme,
                                                            effort == ef,
                                                            ef.as_str(),
                                                        )
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.app.update(cx, |s, cx| {
                                                                s.reasoning_effort = ef;
                                                                cx.notify();
                                                            });
                                                        }))
                                                    }),
                                                )),
                                        )
                                        .child(
                                            gb_row(&theme)
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Fast / Priority Tier"))
                                                        .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child("Prefer low-latency priority endpoints when supported")),
                                                )
                                                .child(gb_toggle_switch("mp-fast", &theme, fast).on_click(
                                                    cx.listener(|this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            s.fast_mode = !s.fast_mode;
                                                            cx.notify();
                                                        });
                                                    }),
                                                )),
                                        )
                                        .child(
                                            gb_row(&theme)
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("YOLO Auto-Approve"))
                                                        .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child("Skip interactive approval prompts for shell & tool calls")),
                                                )
                                                .child(gb_toggle_switch("mp-yolo", &theme, yolo).on_click(
                                                    cx.listener(|this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            s.yolo_mode = !s.yolo_mode;
                                                            cx.notify();
                                                        });
                                                    }),
                                                )),
                                        )
                                        .child(
                                            gb_row(&theme)
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .child(div().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Save as Hermes Default"))
                                                        .child(div().text_size(px(12.0)).text_color(theme.text_tertiary).child("Persist selection to ~/.hermes/config.yaml")),
                                                )
                                                .child(
                                                    gb_toggle_switch("mp-save-def", &theme, self.save_default)
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.save_default = !this.save_default;
                                                            cx.notify();
                                                        })),
                                                ),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
    }
}
