use crate::state::app_state::AppState;
use crate::theme::bot_face::{
    bot_face, resolve_bot_shape, BotFaceState, ALL_GROK_BOT_SHAPES,
};
use crate::theme::components::{cursor_icon, gb_btn, gb_mini_icon_btn, BtnVariant, FONT_SANS};
use crate::theme::tokens::{resolve_bot_color, SandTheme, BOT_COLOR_PRESETS};
use gpui::*;
use gpui_component::input::{Input, InputState};

const PREVIEW_STATES: &[(BotFaceState, &str)] = &[
    (BotFaceState::Idle, "Idle"),
    (BotFaceState::Thinking, "Thinking"),
    (BotFaceState::Writing, "Writing"),
    (BotFaceState::Searching, "Searching"),
    (BotFaceState::Streaming, "Streaming"),
    (BotFaceState::Happy, "Happy"),
    (BotFaceState::Celebrate, "Celebrate"),
    (BotFaceState::Sleeping, "Sleeping"),
];

pub struct AvatarEditorModal {
    app: Entity<AppState>,
    synced_seed: Option<String>,
    name_input: Entity<InputState>,
    shape_id: String,
    color_id: String,
    preview_state: BotFaceState,
}

impl AvatarEditorModal {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Bot name"));
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            synced_seed: None,
            name_input,
            shape_id: "blob".into(),
            color_id: "blue".into(),
            preview_state: BotFaceState::Idle,
        }
    }
}

impl Render for AvatarEditorModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (target_opt, theme, tick, existing) = {
            let s = self.app.read(cx);
            let t = s.avatar_editor_target.clone();
            let ex = t.as_ref().and_then(|x| s.bot_avatars.get(&x.seed).cloned());
            (t, s.sand_theme(), s.anim_tick, ex)
        };

        let Some(target) = target_opt else {
            self.synced_seed = None;
            return div().into_any_element();
        };

        if self.synced_seed.as_deref() != Some(target.seed.as_str()) {
            self.synced_seed = Some(target.seed.clone());
            let init_name = existing
                .as_ref()
                .and_then(|e| e.display_name.clone())
                .unwrap_or_else(|| target.title.clone());
            self.name_input
                .update(cx, |st, cx| st.set_value(init_name, window, cx));
            self.shape_id = resolve_bot_shape(
                existing.as_ref().and_then(|e| e.shape_id.as_deref()),
                &target.seed,
            )
            .to_string();
            self.color_id = existing
                .as_ref()
                .and_then(|e| e.color_id.clone())
                .unwrap_or_else(|| resolve_bot_color(&target.seed, None).id.to_string());
            self.preview_state = BotFaceState::Idle;
        }

        let seed = target.seed.clone();
        let fallback_title = target.title.clone();

        deferred(
            div()
                .id("avatar-editor-backdrop")
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
                        s.avatar_editor_target = None;
                        cx.notify();
                    });
                }))
                .child(
                    div()
                        .id("avatar-editor-modal")
                        .w(px(480.0))
                        .max_w_full()
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
                                        .child("Customize bot"),
                                )
                                .child(
                                    gb_mini_icon_btn("close-avatar-modal", &theme, false)
                                        .child(cursor_icon("close", 14.0, theme.text_secondary))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.app.update(cx, |s, cx| {
                                                s.avatar_editor_target = None;
                                                cx.notify();
                                            });
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("avatar-editor-body")
                                .flex_1()
                                .overflow_y_scroll()
                                .p(px(18.0))
                                .flex()
                                .flex_col()
                                .gap(px(16.0))
                                // Live preview + display name + state buttons
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(16.0))
                                        .child(bot_face(
                                            &theme,
                                            &seed,
                                            Some(&self.shape_id),
                                            Some(&self.color_id),
                                            72.0,
                                            self.preview_state,
                                            tick,
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex()
                                                .flex_col()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(4.0))
                                                        .child(
                                                            div()
                                                                .text_size(px(12.0))
                                                                .text_color(theme.text_secondary)
                                                                .child("Display Name"),
                                                        )
                                                        .child(Input::new(&self.name_input).h(px(34.0))),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_wrap()
                                                        .gap(px(4.0))
                                                        .children(PREVIEW_STATES.iter().map(|&(st, lbl)| {
                                                            let active = self.preview_state == st;
                                                            gb_btn(
                                                                SharedString::from(format!("av-st-{lbl}")),
                                                                &theme,
                                                                if active {
                                                                    BtnVariant::Primary
                                                                } else {
                                                                    BtnVariant::Ghost
                                                                },
                                                                true,
                                                                false,
                                                            )
                                                            .h(px(22.0))
                                                            .px(px(8.0))
                                                            .text_size(px(11.0))
                                                            .child(lbl)
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                this.preview_state = st;
                                                                cx.notify();
                                                            }))
                                                        })),
                                                ),
                                        ),
                                )
                                // All 26 Grok Bot shapes grid (5 columns)
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_secondary)
                                                .child(format!("Shape ({})", ALL_GROK_BOT_SHAPES.len())),
                                        )
                                        .child(
                                            div()
                                                .id("av-shape-grid")
                                                .max_h(px(210.0))
                                                .overflow_y_scroll()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(6.0))
                                                .children(ALL_GROK_BOT_SHAPES.iter().map(|&(sid, _lbl)| {
                                                    let active = self.shape_id == sid;
                                                    div()
                                                        .id(SharedString::from(format!("av-shape-{sid}")))
                                                        .w(px(78.0))
                                                        .h(px(44.0))
                                                        .rounded(px(10.0))
                                                        .bg(if active {
                                                            theme.accent_soft
                                                        } else {
                                                            theme.bg_tertiary
                                                        })
                                                        .border_1()
                                                        .border_color(if active {
                                                            theme.accent_primary
                                                        } else {
                                                            SandTheme::transparent()
                                                        })
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .hover(|st| st.bg(theme.bg_hover))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.shape_id = sid.to_string();
                                                            cx.notify();
                                                        }))
                                                        .child(bot_face(
                                                            &theme,
                                                            &seed,
                                                            Some(sid),
                                                            Some(&self.color_id),
                                                            32.0,
                                                            BotFaceState::Idle,
                                                            0,
                                                        ))
                                                })),
                                        ),
                                )
                                // All 11 official bot colors
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_secondary)
                                                .child(format!("Color ({})", BOT_COLOR_PRESETS.len())),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap(px(8.0))
                                                .children(BOT_COLOR_PRESETS.iter().map(|preset| {
                                                    let cid = preset.id;
                                                    let active = self.color_id == cid;
                                                    div()
                                                        .id(SharedString::from(format!("av-col-{cid}")))
                                                        .w(px(24.0))
                                                        .h(px(24.0))
                                                        .rounded_full()
                                                        .bg(rgb(preset.hex))
                                                        .border_2()
                                                        .border_color(if active {
                                                            theme.text_primary
                                                        } else {
                                                            SandTheme::transparent()
                                                        })
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.color_id = cid.to_string();
                                                            cx.notify();
                                                        }))
                                                })),
                                        ),
                                )
                                // Footer buttons
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .child(
                                            gb_btn("av-cancel", &theme, BtnVariant::Ghost, false, false)
                                                .child("Cancel")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.app.update(cx, |s, cx| {
                                                        s.avatar_editor_target = None;
                                                        cx.notify();
                                                    });
                                                })),
                                        )
                                        .child(
                                            gb_btn("av-save", &theme, BtnVariant::Primary, false, false)
                                                .child("Save Appearance")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    let nm = this.name_input.read(cx).value().trim().to_string();
                                                    let final_name = if nm.is_empty() {
                                                        fallback_title.clone()
                                                    } else {
                                                        nm
                                                    };
                                                    let sh = this.shape_id.clone();
                                                    let co = this.color_id.clone();
                                                    this.app.update(cx, |s, cx| {
                                                        s.update_bot_avatar(
                                                            seed.clone(),
                                                            Some(final_name),
                                                            Some(sh),
                                                            Some(co),
                                                            cx,
                                                        );
                                                        s.avatar_editor_target = None;
                                                        cx.notify();
                                                    });
                                                })),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
    }
}
