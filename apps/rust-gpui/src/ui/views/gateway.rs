use crate::state::app_state::AppState;
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_dot, gb_row, gb_section_card, gb_section_card_head,
    BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};

pub struct GatewayView {
    app: Entity<AppState>,
    busy_action: Option<&'static str>,
    add_webhook_open: bool,
    hook_name_input: Entity<InputState>,
    hook_url_input: Entity<InputState>,
}

impl GatewayView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hook_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Deploy notifications"));
        let hook_url_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("https://hooks.example.com/hermes"));
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            busy_action: None,
            add_webhook_open: false,
            hook_name_input,
            hook_url_input,
        }
    }

    fn run_gateway_action(&mut self, action: &'static str, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            self.app
                .update(cx, |s, cx| s.show_toast("Connect to Hermes first", cx));
            return;
        };
        self.busy_action = Some(action);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.gateway_action(action).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.busy_action = None;
                v.app.update(cx, |s, cx| {
                    match res {
                        Ok(_) => {
                            s.show_toast(format!("Gateway {action} requested"), cx);
                            s.refresh_capabilities(false, cx);
                        }
                        Err(e) => s.show_toast(e, cx),
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn test_platform(&mut self, id: String, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        let id_c = id.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.test_messaging_platform(&id_c).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => s.show_toast(format!("Tested {id}"), cx),
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn create_webhook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.hook_name_input.read(cx).value().trim().to_string();
        let url = self.hook_url_input.read(cx).value().trim().to_string();
        if url.is_empty() {
            return;
        }
        let final_name = if name.is_empty() {
            "Webhook".to_string()
        } else {
            name
        };
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        self.add_webhook_open = false;
        self.hook_name_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.hook_url_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.create_webhook(&final_name, &url).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.show_toast("Webhook created", cx);
                        s.refresh_capabilities(false, cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn delete_webhook(&mut self, id: String, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.delete_webhook(&id).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.show_toast("Webhook deleted", cx);
                        s.refresh_capabilities(false, cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }
}

impl Render for GatewayView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (theme, status, platforms, webhooks, has_client) = {
            let s = self.app.read(cx);
            (
                s.sand_theme(),
                s.hermes_status.clone(),
                s.messaging_platforms.clone(),
                s.webhooks.clone(),
                s.client.is_some(),
            )
        };

        let gw_running = status
            .as_ref()
            .and_then(|st| st.gateway_running)
            .unwrap_or(false);
        let gw_pid = status.as_ref().and_then(|st| st.gateway_pid);
        let gw_state = status
            .as_ref()
            .and_then(|st| st.gateway_state.clone())
            .unwrap_or_else(|| if gw_running { "running".into() } else { "stopped".into() });
        let connected_platforms = platforms
            .iter()
            .filter(|p| p.connected || p.configured)
            .count();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_pane)
            .font_family(FONT_SANS)
            .relative()
            .child(
                div()
                    .id("gateway-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(28.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(940.0))
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            // Header
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .justify_between()
                                    .gap(px(14.0))
                                    .flex_wrap()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(px(22.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_primary)
                                                    .child("Gateway & Messaging"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child("Control the Hermes Gateway daemon, connected messaging bridges, and outbound webhooks."),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                gb_btn(
                                                    "gw-start",
                                                    &theme,
                                                    BtnVariant::Primary,
                                                    false,
                                                    !has_client || self.busy_action.is_some() || gw_running,
                                                )
                                                .child(cursor_icon("play", 13.0, theme.text_on_primary))
                                                .child(if self.busy_action == Some("start") {
                                                    "Starting…"
                                                } else {
                                                    "Start"
                                                })
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.run_gateway_action("start", cx);
                                                })),
                                            )
                                            .child(
                                                gb_btn(
                                                    "gw-restart",
                                                    &theme,
                                                    BtnVariant::Secondary,
                                                    false,
                                                    !has_client || self.busy_action.is_some(),
                                                )
                                                .child(cursor_icon("refresh", 13.0, theme.text_primary))
                                                .child("Restart")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.run_gateway_action("restart", cx);
                                                })),
                                            )
                                            .child(
                                                gb_btn(
                                                    "gw-stop",
                                                    &theme,
                                                    BtnVariant::Danger,
                                                    false,
                                                    !has_client || self.busy_action.is_some() || !gw_running,
                                                )
                                                .child(cursor_icon("stop", 13.0, theme.danger_text))
                                                .child("Stop")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.run_gateway_action("stop", cx);
                                                })),
                                            ),
                                    ),
                            )
                            // 3 Stat cards
                            .child(
                                div()
                                    .w_full()
                                    .grid()
                                    .grid_cols(3)
                                    .gap(px(12.0))
                                    .child(stat_card(
                                        &theme,
                                        "Gateway Daemon",
                                        gw_state.to_uppercase(),
                                        match gw_pid {
                                            Some(pid) => format!("PID {pid}"),
                                            None => if gw_running {
                                                "Running".into()
                                            } else {
                                                "Not active".into()
                                            },
                                        },
                                        Some(if gw_running { 0x00c972 } else { 0x8e8d8a }),
                                    ))
                                    .child(stat_card(
                                        &theme,
                                        "Messaging Platforms",
                                        format!("{connected_platforms} / {}", platforms.len()),
                                        "Telegram, Discord, Slack, WhatsApp, Signal".into(),
                                        None,
                                    ))
                                    .child(stat_card(
                                        &theme,
                                        "Webhooks",
                                        webhooks.len().to_string(),
                                        "Active automation endpoints".into(),
                                        None,
                                    )),
                            )
                            // Messaging platforms card
                            .child(
                                gb_section_card(&theme)
                                    .child(
                                        gb_section_card_head(&theme)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(cursor_icon("chat-bubble", 15.0, theme.text_secondary))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Messaging Platforms"),
                                                    ),
                                            )
                                            .child(
                                                gb_btn("gw-refresh", &theme, BtnVariant::Ghost, true, !has_client)
                                                    .child(cursor_icon("refresh", 12.5, theme.text_secondary))
                                                    .child("Refresh")
                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                        v.app.update(cx, |s, cx| s.refresh_capabilities(false, cx));
                                                    })),
                                            ),
                                    )
                                    .when(platforms.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .p(px(24.0))
                                                .text_size(px(13.0))
                                                .text_color(theme.text_secondary)
                                                .child("No messaging platforms reported by the Hermes backend."),
                                        )
                                    })
                                    .children(platforms.iter().enumerate().map(|(idx, p)| {
                                        let id = p.id.clone();
                                        let label = p.name.clone().unwrap_or_else(|| p.id.clone());
                                        let tone = if p.connected {
                                            BadgeTone::Success
                                        } else if p.configured {
                                            BadgeTone::Warning
                                        } else {
                                            BadgeTone::Muted
                                        };
                                        let badge_text = if p.connected {
                                            "Connected"
                                        } else if p.configured {
                                            "Configured"
                                        } else {
                                            "Unconfigured"
                                        };
                                        gb_row(&theme)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(8.0))
                                                            .child(
                                                                div()
                                                                    .text_size(px(13.5))
                                                                    .font_weight(FontWeight::MEDIUM)
                                                                    .text_color(theme.text_primary)
                                                                    .child(label),
                                                            )
                                                            .child(gb_badge(&theme, tone, badge_text)),
                                                    )
                                                    .when_some(p.status.clone(), |d, st| {
                                                        d.child(
                                                            div()
                                                                .text_size(px(12.0))
                                                                .text_color(theme.text_secondary)
                                                                .child(st),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                gb_btn(
                                                    SharedString::from(format!("test-plat-{idx}")),
                                                    &theme,
                                                    BtnVariant::Secondary,
                                                    true,
                                                    !has_client,
                                                )
                                                .child("Test")
                                                .on_click(cx.listener(move |v, _, _, cx| {
                                                    v.test_platform(id.clone(), cx);
                                                })),
                                            )
                                    })),
                            )
                            // Webhooks card
                            .child(
                                gb_section_card(&theme)
                                    .child(
                                        gb_section_card_head(&theme)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(cursor_icon("link", 15.0, theme.text_secondary))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Webhooks"),
                                                    ),
                                            )
                                            .child(
                                                gb_btn("gw-add-hook", &theme, BtnVariant::Primary, true, !has_client)
                                                    .child(cursor_icon("plus", 12.5, theme.text_on_primary))
                                                    .child("Add Webhook")
                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                        v.add_webhook_open = true;
                                                        cx.notify();
                                                    })),
                                            ),
                                    )
                                    .when(webhooks.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .p(px(24.0))
                                                .text_size(px(13.0))
                                                .text_color(theme.text_secondary)
                                                .child("No webhooks configured yet."),
                                        )
                                    })
                                    .children(webhooks.iter().enumerate().map(|(idx, hook)| {
                                        let hid = hook.id.clone();
                                        let name = hook.name.clone().unwrap_or_else(|| hook.id.clone());
                                        gb_row(&theme)
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(theme.text_primary)
                                                            .child(name),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(11.5))
                                                            .text_color(theme.text_secondary)
                                                            .child(hook.url.clone()),
                                                    ),
                                            )
                                            .child(
                                                gb_btn(
                                                    SharedString::from(format!("del-hook-{idx}")),
                                                    &theme,
                                                    BtnVariant::Danger,
                                                    true,
                                                    !has_client,
                                                )
                                                .child("Delete")
                                                .on_click(cx.listener(move |v, _, _, cx| {
                                                    v.delete_webhook(hid.clone(), cx);
                                                })),
                                            )
                                    })),
                            ),
                    ),
            )
            // Add Webhook Modal
            .when(self.add_webhook_open, |d| {
                d.child(
                    div()
                        .id("hook-modal-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(theme.bg_scrim)
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.add_webhook_open = false;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id("hook-modal-card")
                                .w(px(460.0))
                                .rounded(px(18.0))
                                .bg(theme.bg_elevated)
                                .border_1()
                                .border_color(theme.border_default)
                                .p(px(22.0))
                                .flex()
                                .flex_col()
                                .gap(px(14.0))
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text_primary)
                                        .child("Add Webhook"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_secondary)
                                                .child("Name"),
                                        )
                                        .child(Input::new(&self.hook_name_input)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text_secondary)
                                                .child("Target URL"),
                                        )
                                        .child(Input::new(&self.hook_url_input)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .justify_end()
                                        .gap(px(8.0))
                                        .pt(px(6.0))
                                        .child(
                                            gb_btn("hook-cancel", &theme, BtnVariant::Ghost, false, false)
                                                .child("Cancel")
                                                .on_click(cx.listener(|v, _, _, cx| {
                                                    v.add_webhook_open = false;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_btn("hook-save", &theme, BtnVariant::Primary, false, false)
                                                .child("Save Webhook")
                                                .on_click(cx.listener(|v, _, window, cx| {
                                                    v.create_webhook(window, cx);
                                                })),
                                        ),
                                ),
                        ),
                )
            })
    }
}

fn stat_card(
    theme: &crate::theme::tokens::SandTheme,
    label: &str,
    value: String,
    subtitle: String,
    dot_hex: Option<u32>,
) -> Div {
    div()
        .rounded(px(16.0))
        .bg(theme.bg_elevated)
        .border_1()
        .border_color(theme.border_default)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .when_some(dot_hex, |d, hex| d.child(gb_dot(hex, 8.0)))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary)
                        .child(label.to_string()),
                ),
        )
        .child(
            div()
                .text_size(px(22.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_primary)
                .child(value),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text_tertiary)
                .child(subtitle),
        )
}
