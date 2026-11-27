use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_dot, gb_row, gb_section_card, gb_section_card_head,
    BadgeTone, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::ui::motion::{gpu_spinner, ModalMotion};
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;

pub struct GatewayView {
    app: Entity<AppState>,
    busy_action: Option<&'static str>,
    webhook_modal: ModalMotion,
    saving_webhook: bool,
    platform_tests: std::collections::HashSet<String>,
    hook_name_input: Entity<InputState>,
    hook_url_input: Entity<InputState>,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
}

impl GatewayView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let hook_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Deploy notifications"));
        let hook_url_input = cx
            .new(|cx| InputState::new(window, cx).placeholder("https://hooks.example.com/hermes"));
        cx.observe(&app, |_, app_e, cx| {
            let app = app_e.read(cx);
            if !app.setup_open && !app.initializing && app.active_view == AppViewId::Gateway {
                cx.notify();
            }
        })
        .detach();
        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());
        Self {
            app,
            busy_action: None,
            webhook_modal: ModalMotion::default(),
            saving_webhook: false,
            platform_tests: Default::default(),
            hook_name_input,
            hook_url_input,
            scroll_handle,
            smooth_scroller,
        }
    }

    fn run_gateway_action(&mut self, action: &'static str, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            let running = s
                .hermes_status
                .as_ref()
                .and_then(|status| status.gateway_running);
            if self.busy_action.is_some()
                || (action == "start" && running == Some(true))
                || (action == "stop" && running != Some(true))
            {
                return;
            }
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
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.show_toast(format!("Gateway {action} requested"), cx);
                        s.refresh_gateway_data(cx);
                    }
                    Err(e) => s.show_toast(e, cx),
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
        if !self.platform_tests.insert(id.clone()) {
            return;
        }
        let id_c = id.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.test_messaging_platform(&id_c).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.platform_tests.remove(&id);
                cx.notify();
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.refresh_gateway_data(cx);
                        s.show_toast(format!("Tested {id}"), cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn create_webhook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving_webhook {
            return;
        }
        let name = self.hook_name_input.read(cx).value().trim().to_string();
        let url = self.hook_url_input.read(cx).value().trim().to_string();
        if !reqwest::Url::parse(&url)
            .is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https") && parsed.has_host())
        {
            self.app.update(cx, |app, cx| {
                app.show_toast("Enter a valid HTTP or HTTPS webhook URL.", cx)
            });
            return;
        }
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let rt = app.tokio_rt.clone();
        let source = app.profile_generation;
        let name = if name.is_empty() {
            "Webhook".to_string()
        } else {
            name
        };
        self.saving_webhook = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = rt
                .spawn(async move { client.create_webhook(&name, &url).await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update_in(cx, |view, window, cx| {
                view.saving_webhook = false;
                cx.notify();
                if view.app.read(cx).profile_generation != source {
                    return;
                }
                match result {
                    Ok(_) => {
                        view.webhook_modal.set_open(false);
                        view.hook_name_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        view.hook_url_input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                        view.app.update(cx, |app, cx| {
                            app.refresh_gateway_data(cx);
                            app.show_toast("Webhook created", cx);
                        });
                    }
                    Err(error) => view.app.update(cx, |app, cx| app.show_toast(error, cx)),
                }
            });
        })
        .detach();
    }

    fn delete_webhook(&mut self, id: String, cx: &mut Context<Self>) {
        self.app.update(cx, |s, cx| {
            if s.client.is_some() {
                s.delete_webhook_optimistic(id, cx);
            }
        });
    }
}

impl Render for GatewayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modal_height = (window.viewport_size().height - px(48.0)).max(px(0.0));
        let starting = self.busy_action == Some("start");
        let restarting = self.busy_action == Some("restart");
        let stopping = self.busy_action == Some("stop");
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
            .unwrap_or_else(|| {
                if gw_running {
                    "running".into()
                } else {
                    "stopped".into()
                }
            });
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
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(
                        div()
                            .id("gateway-scroll")
                            .track_scroll(&self.scroll_handle)
                            .size_full()
                            .overflow_y_scroll()
                            .p(px(28.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(940.0))
                            .flex_shrink_0()
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
                                            .flex_wrap()
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
                                                .when(starting, |d| d.child(gpu_spinner(13.0, theme.text_on_primary)))
                                                .when(!starting, |d| d.child(cursor_icon("play", 13.0, theme.text_on_primary)))
                                                .child(if starting {
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
                                                .when(restarting, |d| d.child(gpu_spinner(13.0, theme.text_primary)))
                                                .when(!restarting, |d| d.child(cursor_icon("refresh", 13.0, theme.text_primary)))
                                                .child(if restarting { "Restarting…" } else { "Restart" })
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
                                                .when(stopping, |d| d.child(gpu_spinner(13.0, theme.danger_text)))
                                                .when(!stopping, |d| d.child(cursor_icon("stop", 13.0, theme.danger_text)))
                                                .child(if stopping { "Stopping…" } else { "Stop" })
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
                                    .flex()
                                    .flex_wrap()
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
                                                        v.app.update(cx, |s, cx| s.refresh_gateway_data(cx));
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
                                        gb_row(&theme).flex_wrap()
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
                                                        v.webhook_modal.set_open(true);
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
                                        gb_row(&theme).flex_wrap()
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
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(smooth_scroll_layer(&self.smooth_scroller)),
            )
            // Add Webhook Modal
            .when(self.webhook_modal.is_open(), |d| {
                d.child(deferred(
                    div()
                        .id("hook-modal-backdrop")
                        .occlude()
                        .absolute()
                        .inset_0()
                        .child(self.webhook_modal.backdrop("webhook-scrim-enter", theme.bg_scrim))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.webhook_modal.set_open(false);
                            cx.notify();
                        }))
                        .child(self.webhook_modal.card("webhook-card-enter",
                            div()
                                .id("hook-modal-card")
                                .w(px(460.0))
                                .max_w_full()
                                .max_h(modal_height)
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
                                                    v.webhook_modal.set_open(false);
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            gb_btn("hook-save", &theme, BtnVariant::Primary, false, !has_client || self.saving_webhook)
                                                .child("Save Webhook")
                                                .on_click(cx.listener(|v, _, window, cx| {
                                                    v.create_webhook(window, cx);
                                                })),
                                        ),
                                )
                                .overflow_y_scrollbar()
                        )),
                ))
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
        .flex_1()
        .min_w(px(135.0))
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
