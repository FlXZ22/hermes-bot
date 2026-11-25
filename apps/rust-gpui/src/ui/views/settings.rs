use crate::backend::ssh::SshTarget;
use crate::backend::updater::UpdateSettingsPatch;
use crate::state::app_state::{AppState, AppViewId, AvatarEditorTarget};
use crate::theme::bot_face::{bot_face_with_options, BotFaceOptions, BotFaceState};
use crate::theme::components::{
    cursor_icon, gb_badge, gb_btn, gb_dot, gb_row, gb_section_card, gb_section_card_head,
    gb_segmented_item, gb_segmented_shell, gb_toggle_switch, BadgeTone, BtnVariant, FONT_MONO,
    FONT_SANS,
};
use crate::theme::tokens::ThemeMode;
use crate::ui::motion::gpu_spinner;
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Appearance,
    Connections,
    ApiKeys,
    Profiles,
    Config,
    Updates,
    Logs,
}

pub struct SettingsView {
    app: Entity<AppState>,
    tab: SettingsTab,

    // API Keys form
    env_key_input: Entity<InputState>,
    env_val_input: Entity<InputState>,

    // Profiles form
    profile_name_input: Entity<InputState>,
    profile_desc_input: Entity<InputState>,

    // Config YAML editor
    config_input: Entity<TextareaState>,
    synced_config: String,

    // SSH connection form (for remote mode)
    ssh_host_input: Entity<InputState>,
    ssh_user_input: Entity<InputState>,
    ssh_port_input: Entity<InputState>,
    ssh_key_input: Entity<InputState>,
    synced_ssh_conn: String,

    // Updater controls
    repo_path_input: Entity<InputState>,
    branch_input: Entity<InputState>,
    force_native_rebuild: bool,
    synced_updater_key: String,
    updater_busy: bool,
    diag_busy: bool,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
}

impl SettingsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (cfg, conn, upd) = {
            let s = app.read(cx);
            (
                s.raw_config.clone(),
                s.connection.clone(),
                s.updater.status(),
            )
        };
        let env_key_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. OPENROUTER_API_KEY"));
        let env_val_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("sk-...")
                .masked(true)
        });
        let profile_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. researcher"));
        let profile_desc_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Optional description"));
        let config_input = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .placeholder("# ~/.hermes/config.yaml")
                .rows(18);
            st.set_value(cfg.clone(), window, cx);
            st
        });

        let ssh = conn.ssh.clone().unwrap_or(SshTarget {
            host: String::new(),
            user: None,
            port: Some(22),
            identity_file: None,
            config_file: None,
        });
        let ssh_host_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("e.g. server.example.com");
            st.set_value(ssh.host.clone(), window, cx);
            st
        });
        let ssh_user_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("ubuntu");
            st.set_value(ssh.user.clone().unwrap_or_default(), window, cx);
            st
        });
        let ssh_port_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("22");
            st.set_value(
                ssh.port.map(|p| p.to_string()).unwrap_or_else(|| "22".into()),
                window,
                cx,
            );
            st
        });
        let ssh_key_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("~/.ssh/id_ed25519");
            st.set_value(ssh.identity_file.clone().unwrap_or_default(), window, cx);
            st
        });

        let repo_path_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("/path/to/hermes-bot");
            st.set_value(upd.repo_path.clone().unwrap_or_default(), window, cx);
            st
        });
        let branch_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("main");
            st.set_value(upd.target_branch.clone(), window, cx);
            st
        });

        let synced_ssh_conn = format!("{}:{:?}", conn.url, conn.ssh);
        let synced_updater_key = format!("{:?}:{}", upd.repo_path, upd.target_branch);

        cx.observe(&app, |_, app_e, cx| {
            if app_e.read(cx).active_view == AppViewId::Settings {
                cx.notify();
            }
        })
        .detach();
        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());
        Self {
            app,
            tab: SettingsTab::Appearance,
            env_key_input,
            env_val_input,
            profile_name_input,
            profile_desc_input,
            config_input,
            synced_config: cfg,
            ssh_host_input,
            ssh_user_input,
            ssh_port_input,
            ssh_key_input,
            synced_ssh_conn,
            repo_path_input,
            branch_input,
            force_native_rebuild: false,
            synced_updater_key,
            updater_busy: false,
            diag_busy: false,
            scroll_handle,
            smooth_scroller,
        }
    }

    fn save_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.env_key_input.read(cx).value().trim().to_string();
        let val = self.env_val_input.read(cx).value().trim().to_string();
        if key.is_empty() || val.is_empty() {
            return;
        }
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        self.env_key_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.env_val_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        let k2 = key.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.set_env(&k2, &val).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.show_toast(format!("Saved {key}"), cx);
                        s.refresh_capabilities(false, cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn delete_env(&mut self, key: String, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        let k2 = key.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.delete_env(&k2).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.show_toast(format!("Removed {key}"), cx);
                        s.refresh_capabilities(false, cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn create_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.profile_name_input.read(cx).value().trim().to_string();
        let desc = self.profile_desc_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        self.profile_name_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        self.profile_desc_input
            .update(cx, |st, cx| st.set_value("", window, cx));
        let n2 = name.clone();
        let desc_opt = if desc.is_empty() { None } else { Some(desc) };
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.create_profile(&n2, desc_opt.as_deref()).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.show_toast(format!("Created bot profile \"{name}\""), cx);
                        s.refresh_capabilities(false, cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn save_config_yaml(&mut self, cx: &mut Context<Self>) {
        let content = self.config_input.read(cx).value().to_string();
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        let c2 = content.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { client.save_raw_config(&c2).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.app.update(cx, |s, cx| match res {
                    Ok(_) => {
                        s.raw_config = content;
                        s.show_toast("Saved config.yaml", cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn save_ssh_settings(&mut self, cx: &mut Context<Self>) {
        let host = self.ssh_host_input.read(cx).value().trim().to_string();
        let user = self.ssh_user_input.read(cx).value().trim().to_string();
        let port_raw = self.ssh_port_input.read(cx).value().trim().to_string();
        let key = self.ssh_key_input.read(cx).value().trim().to_string();
        let ssh = if host.is_empty() {
            None
        } else {
            Some(SshTarget {
                host,
                user: if user.is_empty() { None } else { Some(user) },
                port: port_raw.parse::<u16>().ok(),
                identity_file: if key.is_empty() { None } else { Some(key) },
                config_file: None,
            })
        };
        self.app.update(cx, |s, cx| {
            let mut next = s.connection.clone();
            next.ssh = ssh;
            s.connection = next.clone();
            if let Ok(path) = crate::backend::remote::connection_path() {
                if let Ok(val) = serde_json::to_value(&next) {
                    let _ = crate::backend::remote::save_connection(&path, &val);
                }
            }
            s.show_toast("Saved SSH target for remote workspace & terminal", cx);
            cx.notify();
        });
    }

    fn run_diag(&mut self, security: bool, cx: &mut Context<Self>) {
        let (client, rt) = {
            let s = self.app.read(cx);
            (s.client.clone(), s.tokio_rt.clone())
        };
        let Some(client) = client else {
            return;
        };
        self.diag_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    if security {
                        client.run_security_audit().await
                    } else {
                        client.run_doctor().await
                    }
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.diag_busy = false;
                v.app.update(cx, |s, cx| match res {
                    Ok(val) => {
                        s.diag_output = serde_json::to_string_pretty(&val).unwrap_or_default();
                        s.show_toast(
                            if security {
                                "Security audit completed"
                            } else {
                                "Hermes Doctor completed"
                            },
                            cx,
                        );
                    }
                    Err(e) => {
                        s.diag_output = format!("Error: {e}");
                        s.show_toast(e, cx);
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn check_updates(&mut self, cx: &mut Context<Self>) {
        let (updater, rt) = {
            let s = self.app.read(cx);
            (s.updater.clone(), s.tokio_rt.clone())
        };
        self.updater_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { updater.check_for_updates().await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.updater_busy = false;
                v.app.update(cx, |s, cx| match res {
                    Ok(st) => {
                        let avail = st.update_available;
                        s.update_status = Some(st);
                        s.show_toast(
                            if avail {
                                "Update available!"
                            } else {
                                "You are up to date"
                            },
                            cx,
                        );
                    }
                    Err(e) => s.show_toast(e, cx),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn save_updater_settings(&mut self, auto_check: Option<bool>, cx: &mut Context<Self>) {
        let repo_path = self.repo_path_input.read(cx).value().trim().to_string();
        let branch = self.branch_input.read(cx).value().trim().to_string();
        let patch = UpdateSettingsPatch {
            target_branch: if branch.is_empty() { None } else { Some(branch) },
            repo_path: Some(repo_path),
            auto_check,
        };
        let res = self.app.read(cx).updater.update_settings(patch);
        self.app.update(cx, |s, cx| match res {
            Ok(st) => {
                s.update_status = Some(st);
                s.show_toast("Updater settings saved", cx);
            }
            Err(e) => s.show_toast(e, cx),
        });
    }

    fn apply_update(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        let force = self.force_native_rebuild;
        let updater = self.app.read(cx).updater.clone();
        match updater.apply_update(mode.to_string(), force) {
            Ok(st) => {
                self.app.update(cx, |s, cx| {
                    s.update_status = Some(st);
                    s.show_toast("Update started — watching build logs", cx);
                });
                // Poll updater status while running
                cx.spawn(async move |this, cx| {
                    for _ in 0..180 {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(800))
                            .await;
                        let done = this
                            .update(cx, |v, cx| {
                                let st = v.app.read(cx).updater.status();
                                let finished =
                                    matches!(st.phase.as_str(), "done" | "error" | "idle");
                                v.app.update(cx, |s, cx| {
                                    s.update_status = Some(st);
                                    cx.notify();
                                });
                                finished
                            })
                            .unwrap_or(true);
                        if done {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(e) => {
                self.app.update(cx, |s, cx| s.show_toast(e, cx));
            }
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (
            theme,
            mode,
            active_model,
            conn,
            backend_info,
            sys_stats,
            boot_ms,
            restarting,
            env_entries,
            profiles,
            active_profile,
            avatars,
            raw_config,
            server_logs,
            sup_logs,
            diag_output,
            upd_status,
            has_client,
        ) = {
            let s = self.app.read(cx);
            (
                s.sand_theme(),
                s.theme_mode(),
                s.active_model.clone(),
                s.connection.clone(),
                s.rust_backend.clone(),
                s.system_stats.clone(),
                s.bootstrap_ms,
                s.restarting_backend,
                s.env_entries.clone(),
                s.profiles.clone(),
                s.active_profile.clone(),
                s.bot_avatars.clone(),
                s.raw_config.clone(),
                s.server_logs.clone(),
                s.rust_backend
                    .as_ref()
                    .map(|b| b.logs.clone())
                    .unwrap_or_default(),
                s.diag_output.clone(),
                s.update_status.clone().unwrap_or_else(|| s.updater.status()),
                s.client.is_some(),
            )
        };

        if raw_config != self.synced_config {
            self.synced_config = raw_config.clone();
            self.config_input
                .update(cx, |st, cx| st.set_value(raw_config.clone(), window, cx));
        }
        let ssh_key = format!("{}:{:?}", conn.url, conn.ssh);
        if ssh_key != self.synced_ssh_conn {
            self.synced_ssh_conn = ssh_key;
            let ssh = conn.ssh.clone().unwrap_or(SshTarget {
                host: String::new(),
                user: None,
                port: Some(22),
                identity_file: None,
                config_file: None,
            });
            self.ssh_host_input
                .update(cx, |st, cx| st.set_value(ssh.host, window, cx));
            self.ssh_user_input.update(cx, |st, cx| {
                st.set_value(ssh.user.unwrap_or_default(), window, cx)
            });
            self.ssh_port_input.update(cx, |st, cx| {
                st.set_value(
                    ssh.port.map(|p| p.to_string()).unwrap_or_else(|| "22".into()),
                    window,
                    cx,
                )
            });
            self.ssh_key_input.update(cx, |st, cx| {
                st.set_value(ssh.identity_file.unwrap_or_default(), window, cx)
            });
        }
        let upd_key = format!("{:?}:{}", upd_status.repo_path, upd_status.target_branch);
        if upd_key != self.synced_updater_key {
            self.synced_updater_key = upd_key;
            self.repo_path_input.update(cx, |st, cx| {
                st.set_value(upd_status.repo_path.clone().unwrap_or_default(), window, cx)
            });
            self.branch_input.update(cx, |st, cx| {
                st.set_value(upd_status.target_branch.clone(), window, cx)
            });
        }

        let tabs: &[(SettingsTab, &str)] = &[
            (SettingsTab::Appearance, "Appearance"),
            (SettingsTab::Connections, "Connections"),
            (SettingsTab::ApiKeys, "API Keys"),
            (SettingsTab::Profiles, "Profiles"),
            (SettingsTab::Config, "Config"),
            (SettingsTab::Updates, "Updates"),
            (SettingsTab::Logs, "Logs"),
        ];

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_pane)
            .font_family(FONT_SANS)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .child(
                        div()
                            .id("settings-scroll")
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
                            .max_w(px(900.0))
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            // Header
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
                                            .child("Settings"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(theme.text_secondary)
                                            .child("Configure theme, Hermes backend connection, secrets, bot profiles, YAML config, updates, and diagnostics."),
                                    ),
                            )
                            // Segmented tab bar
                            .child(
                                gb_segmented_shell(&theme).flex_wrap().children(
                                    tabs.iter().map(|(t, label)| {
                                        let tab_val = *t;
                                        gb_segmented_item(
                                            SharedString::from(format!("set-tab-{label}")),
                                            &theme,
                                            self.tab == tab_val,
                                            *label,
                                        )
                                        .on_click(cx.listener(move |v, _, _, cx| {
                                            v.tab = tab_val;
                                            cx.notify();
                                        }))
                                    }),
                                ),
                            )
                            // Tab Content
                            .child(match self.tab {
                                SettingsTab::Appearance => {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        // Theme & Model card
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Theme & Active Model"),
                                                    ),
                                                )
                                                .child(
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
                                                                        .child("Grok Bot Sand Theme"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.0))
                                                                        .text_color(theme.text_secondary)
                                                                        .child("Warm stone Sand Light & Sand Dark surfaces rendered natively via GPUI."),
                                                                ),
                                                        )
                                                        .child(
                                                            gb_segmented_shell(&theme)
                                                                .child(
                                                                    gb_segmented_item(
                                                                        "theme-light",
                                                                        &theme,
                                                                        mode == ThemeMode::Light,
                                                                        "Sand Light",
                                                                    )
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.app.update(cx, |s, cx| {
                                                                            s.set_theme(ThemeMode::Light, cx);
                                                                        });
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_segmented_item(
                                                                        "theme-dark",
                                                                        &theme,
                                                                        mode == ThemeMode::Dark,
                                                                        "Sand Dark",
                                                                    )
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.app.update(cx, |s, cx| {
                                                                            s.set_theme(ThemeMode::Dark, cx);
                                                                        });
                                                                    })),
                                                                ),
                                                        ),
                                                )
                                                .child(
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
                                                                        .child("Active Model"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(12.0))
                                                                        .text_color(theme.text_secondary)
                                                                        .child(active_model),
                                                                ),
                                                        )
                                                        .child(
                                                            gb_btn("open-model-picker-set", &theme, BtnVariant::Secondary, true, false)
                                                                .child("Change Model")
                                                                .on_click(cx.listener(|v, _, _, cx| {
                                                                    v.app.update(cx, |s, cx| {
                                                                        s.model_picker_open = true;
                                                                        cx.notify();
                                                                    });
                                                                })),
                                                        ),
                                                ),
                                        )
                                        // Desktop Runtime card
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme)
                                                        .child(
                                                            div()
                                                                .text_size(px(13.5))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(theme.text_primary)
                                                                .child("Native Rust GPUI Runtime"),
                                                        )
                                                        .child(gb_badge(&theme, BadgeTone::Success, "GPUI GPU-Accelerated")),
                                                )
                                                .child(
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(2.0))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(13.0))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .text_color(theme.text_primary)
                                                                        .child("System & Bootstrap"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(11.5))
                                                                        .text_color(theme.text_secondary)
                                                                        .child(match (&sys_stats, boot_ms) {
                                                                            (Some(st), Some(ms)) => format!(
                                                                                "{} · {} · RAM {} / {} MB ({:.0}%) · Boot {}ms",
                                                                                st.hostname, st.os_name, st.memory_used_mb, st.memory_total_mb, st.memory_percent, ms
                                                                            ),
                                                                            (Some(st), None) => format!(
                                                                                "{} · {} · RAM {} / {} MB ({:.0}%)",
                                                                                st.hostname, st.os_name, st.memory_used_mb, st.memory_total_mb, st.memory_percent
                                                                            ),
                                                                            _ => "Linux Native GPUI".into(),
                                                                        }),
                                                                ),
                                                        )
                                                        .child(
                                                            gb_btn("restart-be-set", &theme, BtnVariant::Secondary, true, restarting)
                                                                .when(restarting, |d| d.child(gpu_spinner(12.5, theme.text_primary)))
                                                                .when(!restarting, |d| d.child(cursor_icon("refresh", 12.5, theme.text_primary)))
                                                                .child(if restarting { "Restarting…" } else { "Restart Backend" })
                                                                .on_click(cx.listener(|v, _, _, cx| {
                                                                    v.app.update(cx, |s, cx| s.restart_backend(cx));
                                                                })),
                                                        ),
                                                ),
                                        )
                                }
                                SettingsTab::Connections => {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme)
                                                        .child(
                                                            div()
                                                                .text_size(px(13.5))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(theme.text_primary)
                                                                .child("Active Connection"),
                                                        )
                                                        .child(gb_badge(
                                                            &theme,
                                                            if conn.is_local() { BadgeTone::Info } else { BadgeTone::Accent },
                                                            if conn.is_local() { "Local" } else { "Remote" },
                                                        )),
                                                )
                                                .child(
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(3.0))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(13.5))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .text_color(theme.text_primary)
                                                                        .child(if conn.url.is_empty() {
                                                                            "Local managed Hermes backend".to_string()
                                                                        } else {
                                                                            conn.url.clone()
                                                                        }),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(11.5))
                                                                        .text_color(theme.text_secondary)
                                                                        .child(match &backend_info {
                                                                            Some(bi) => format!(
                                                                                "Managed PID {:?} · {}",
                                                                                bi.pid,
                                                                                bi.url.as_deref().unwrap_or("starting")
                                                                            ),
                                                                            None => format!("Mode: {:?}", conn.mode),
                                                                        }),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap(px(8.0))
                                                                .child(
                                                                    gb_btn("manage-conn-btn", &theme, BtnVariant::Primary, true, false)
                                                                        .child("Manage Connection")
                                                                        .on_click(cx.listener(|v, _, _, cx| {
                                                                            v.app.update(cx, |s, cx| {
                                                                                s.setup_open = true;
                                                                                cx.notify();
                                                                            });
                                                                        })),
                                                                )
                                                                .child(
                                                                    gb_btn("refresh-conn-btn", &theme, BtnVariant::Secondary, true, !has_client)
                                                                        .child("Refresh")
                                                                        .on_click(cx.listener(|v, _, _, cx| {
                                                                            v.app.update(cx, |s, cx| s.refresh_capabilities(true, cx));
                                                                        })),
                                                                ),
                                                        ),
                                                ),
                                        )
                                        // SSH configuration card
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Remote SSH Target (Terminal & Git)"),
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .p(px(16.0))
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(12.0))
                                                        .child(
                                                            div()
                                                                .grid()
                                                                .grid_cols(2)
                                                                .gap(px(10.0))
                                                                .child(labeled_input(&theme, "SSH Host", &self.ssh_host_input))
                                                                .child(labeled_input(&theme, "SSH User", &self.ssh_user_input))
                                                                .child(labeled_input(&theme, "SSH Port", &self.ssh_port_input))
                                                                .child(labeled_input(&theme, "Identity File", &self.ssh_key_input)),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .justify_end()
                                                                .child(
                                                                    gb_btn("save-ssh-btn", &theme, BtnVariant::Primary, true, false)
                                                                        .child("Save SSH Target")
                                                                        .on_click(cx.listener(|v, _, _, cx| {
                                                                            v.save_ssh_settings(cx);
                                                                        })),
                                                                ),
                                                        ),
                                                ),
                                        )
                                }
                                SettingsTab::ApiKeys => {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Add Environment Secret (~/.hermes/.env)"),
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .p(px(16.0))
                                                        .flex()
                                                        .items_end()
                                                        .gap(px(10.0))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .child(labeled_input(&theme, "Key Name", &self.env_key_input)),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .child(labeled_input(&theme, "Secret Value", &self.env_val_input)),
                                                        )
                                                        .child(
                                                            gb_btn("add-env-btn", &theme, BtnVariant::Primary, false, !has_client)
                                                                .child("Save")
                                                                .on_click(cx.listener(|v, _, window, cx| {
                                                                    v.save_env(window, cx);
                                                                })),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child(format!("Configured Keys ({})", env_entries.len())),
                                                    ),
                                                )
                                                .when(env_entries.is_empty(), |d| {
                                                    d.child(
                                                        div()
                                                            .p(px(22.0))
                                                            .text_size(px(13.0))
                                                            .text_color(theme.text_secondary)
                                                            .child("No API keys found in ~/.hermes/.env."),
                                                    )
                                                })
                                                .children(env_entries.iter().enumerate().map(|(idx, e)| {
                                                    let key = e.key.clone();
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(2.0))
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(13.0))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .text_color(theme.text_primary)
                                                                        .child(e.key.clone()),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(11.5))
                                                                        .text_color(theme.text_tertiary)
                                                                        .child(
                                                                            e.masked
                                                                                .clone()
                                                                                .unwrap_or_else(|| "••••••••".into()),
                                                                        ),
                                                                ),
                                                        )
                                                        .child(
                                                            gb_btn(
                                                                SharedString::from(format!("del-env-{idx}")),
                                                                &theme,
                                                                BtnVariant::Danger,
                                                                true,
                                                                !has_client,
                                                            )
                                                            .child("Delete")
                                                            .on_click(cx.listener(move |v, _, _, cx| {
                                                                v.delete_env(key.clone(), cx);
                                                            })),
                                                        )
                                                })),
                                        )
                                }
                                SettingsTab::Profiles => {
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Create Hermes Bot Profile"),
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .p(px(16.0))
                                                        .flex()
                                                        .items_end()
                                                        .gap(px(10.0))
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .child(labeled_input(&theme, "Profile Name", &self.profile_name_input)),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_1()
                                                                .child(labeled_input(&theme, "Description", &self.profile_desc_input)),
                                                        )
                                                        .child(
                                                            gb_btn("create-prof-btn", &theme, BtnVariant::Primary, false, !has_client)
                                                                .child("Create")
                                                                .on_click(cx.listener(|v, _, window, cx| {
                                                                    v.create_profile(window, cx);
                                                                })),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child(format!("Bot Profiles ({})", profiles.len())),
                                                    ),
                                                )
                                                .children(profiles.iter().enumerate().map(|(idx, prof)| {
                                                    let pname = prof.name.clone();
                                                    let seed = format!("profile:{pname}");
                                                    let ov = avatars.get(&seed);
                                                    let active = pname == active_profile;
                                                    let display = ov
                                                        .and_then(|o| o.display_name.clone())
                                                        .unwrap_or_else(|| pname.clone());
                                                    let p_switch = pname.clone();
                                                    let seed_edit = seed.clone();
                                                    let title_edit = display.clone();
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap(px(12.0))
                                                                .child(bot_face_with_options(
                                                                    &theme,
                                                                    &seed,
                                                                    ov.and_then(|o| o.shape_id.as_deref()),
                                                                    ov.and_then(|o| o.color_id.as_deref()),
                                                                    32.0,
                                                                    BotFaceState::Idle,
                                                                    BotFaceOptions { slot_key: "settings-profile", is_static: true, ..Default::default() },
                                                                ))
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
                                                                                        .child(display),
                                                                                )
                                                                                .when(active, |d| {
                                                                                    d.child(gb_badge(
                                                                                        &theme,
                                                                                        BadgeTone::Accent,
                                                                                        "Active",
                                                                                    ))
                                                                                }),
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .text_size(px(12.0))
                                                                                .text_color(theme.text_secondary)
                                                                                .child(
                                                                                    prof.description
                                                                                        .clone()
                                                                                        .unwrap_or_else(|| pname.clone()),
                                                                                ),
                                                                        ),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap(px(6.0))
                                                                .child(
                                                                    gb_btn(
                                                                        SharedString::from(format!("av-prof-{idx}")),
                                                                        &theme,
                                                                        BtnVariant::Ghost,
                                                                        true,
                                                                        false,
                                                                    )
                                                                    .child("Customize")
                                                                    .on_click(cx.listener(move |v, _, _, cx| {
                                                                        v.app.update(cx, |s, cx| {
                                                                            s.avatar_editor_target = Some(AvatarEditorTarget {
                                                                                seed: seed_edit.clone(),
                                                                                title: title_edit.clone(),
                                                                            });
                                                                            cx.notify();
                                                                        });
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_btn(
                                                                        SharedString::from(format!("sw-prof-{idx}")),
                                                                        &theme,
                                                                        if active { BtnVariant::Secondary } else { BtnVariant::Primary },
                                                                        true,
                                                                        active,
                                                                    )
                                                                    .child(if active { "Active" } else { "Switch" })
                                                                    .on_click(cx.listener(move |v, _, _, cx| {
                                                                        v.app.update(cx, |s, cx| {
                                                                            s.switch_profile(p_switch.clone(), cx);
                                                                        });
                                                                    })),
                                                                ),
                                                        )
                                                })),
                                        )
                                }
                                SettingsTab::Config => {
                                    div().child(
                                        gb_section_card(&theme)
                                            .child(
                                                gb_section_card_head(&theme)
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("~/.hermes/config.yaml"),
                                                    )
                                                    .child(
                                                        gb_btn("save-yaml-btn", &theme, BtnVariant::Primary, true, !has_client)
                                                            .child("Save YAML")
                                                            .on_click(cx.listener(|v, _, _, cx| {
                                                                v.save_config_yaml(cx);
                                                            })),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .p(px(14.0))
                                                    .font_family(FONT_MONO)
                                                    .child(Textarea::new(&self.config_input)),
                                            ),
                                    )
                                }
                                SettingsTab::Updates => {
                                    let updating = !matches!(
                                        upd_status.phase.as_str(),
                                        "idle" | "done" | "error"
                                    );
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        // Version & status card
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme)
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .gap(px(8.0))
                                                                .child(gb_dot(
                                                                    if upd_status.update_available {
                                                                        0xff9800
                                                                    } else {
                                                                        0x00c972
                                                                    },
                                                                    8.0,
                                                                ))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(13.5))
                                                                        .font_weight(FontWeight::SEMIBOLD)
                                                                        .text_color(theme.text_primary)
                                                                        .child(format!(
                                                                            "Hermes Grok Bot v{} ({})",
                                                                            upd_status.current_version,
                                                                            upd_status.current_commit
                                                                        )),
                                                                ),
                                                        )
                                                        .child(gb_badge(
                                                            &theme,
                                                            if upd_status.update_available {
                                                                BadgeTone::Warning
                                                            } else {
                                                                BadgeTone::Success
                                                            },
                                                            if upd_status.update_available {
                                                                format!("{} commits behind", upd_status.commits_behind)
                                                            } else {
                                                                "Up to date".into()
                                                            },
                                                        )),
                                                )
                                                .child(
                                                    gb_row(&theme)
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(3.0))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.5))
                                                                        .text_color(theme.text_primary)
                                                                        .child(format!(
                                                                            "Branch: {} · Host: rust-gpui · Built {}",
                                                                            upd_status.current_branch,
                                                                            upd_status.built_at
                                                                        )),
                                                                )
                                                                .when_some(upd_status.latest_commit_message.clone(), |d, msg| {
                                                                    d.child(
                                                                        div()
                                                                            .text_size(px(12.0))
                                                                            .text_color(theme.text_secondary)
                                                                            .child(format!("Latest: {msg}")),
                                                                    )
                                                                })
                                                                .when_some(upd_status.error.clone(), |d, err| {
                                                                    d.child(
                                                                        div()
                                                                            .text_size(px(12.0))
                                                                            .text_color(theme.danger_text)
                                                                            .child(err),
                                                                    )
                                                                }),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap(px(8.0))
                                                                .child(
                                                                    gb_btn(
                                                                        "upd-check",
                                                                        &theme,
                                                                        BtnVariant::Secondary,
                                                                        true,
                                                                        self.updater_busy || updating,
                                                                    )
                                                                    .when(self.updater_busy, |d| d.child(gpu_spinner(12.0, theme.text_primary)))
                                                                    .child(if self.updater_busy {
                                                                        "Checking…"
                                                                    } else {
                                                                        "Check for Updates"
                                                                    })
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.check_updates(cx);
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_btn(
                                                                        "upd-apply",
                                                                        &theme,
                                                                        BtnVariant::Primary,
                                                                        true,
                                                                        updating || !upd_status.repo_valid,
                                                                    )
                                                                    .when(updating, |d| d.child(gpu_spinner(12.0, theme.text_on_primary)))
                                                                    .child(if updating { "Updating…" } else { "Update Now" })
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.apply_update("git", cx);
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_btn(
                                                                        "upd-local",
                                                                        &theme,
                                                                        BtnVariant::Secondary,
                                                                        true,
                                                                        updating || !upd_status.repo_valid,
                                                                    )
                                                                    .child("Install Local")
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.apply_update("local", cx);
                                                                    })),
                                                                ),
                                                        ),
                                                ),
                                        )
                                        // Updater Configuration card
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme).child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Update Source & Options"),
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .p(px(16.0))
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(12.0))
                                                        .child(
                                                            div()
                                                                .grid()
                                                                .grid_cols(2)
                                                                .gap(px(10.0))
                                                                .child(labeled_input(&theme, "Local Git Repository", &self.repo_path_input))
                                                                .child(labeled_input(&theme, "Target Branch", &self.branch_input)),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_between()
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap(px(16.0))
                                                                        .child(
                                                                            div()
                                                                                .flex()
                                                                                .items_center()
                                                                                .gap(px(8.0))
                                                                                .child(
                                                                                    gb_toggle_switch(
                                                                                        "upd-auto-check",
                                                                                        &theme,
                                                                                        upd_status.auto_check,
                                                                                    )
                                                                                    .on_click(cx.listener(
                                                                                        move |v, _, _, cx| {
                                                                                            v.save_updater_settings(
                                                                                                Some(!upd_status.auto_check),
                                                                                                cx,
                                                                                            );
                                                                                        },
                                                                                    )),
                                                                                )
                                                                                .child(
                                                                                    div()
                                                                                        .text_size(px(12.5))
                                                                                        .text_color(theme.text_primary)
                                                                                        .child("Auto-check on launch"),
                                                                                ),
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .flex()
                                                                                .items_center()
                                                                                .gap(px(8.0))
                                                                                .child(
                                                                                    gb_toggle_switch(
                                                                                        "upd-force-rebuild",
                                                                                        &theme,
                                                                                        self.force_native_rebuild,
                                                                                    )
                                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                                        v.force_native_rebuild = !v.force_native_rebuild;
                                                                                        cx.notify();
                                                                                    })),
                                                                                )
                                                                                .child(
                                                                                    div()
                                                                                        .text_size(px(12.5))
                                                                                        .text_color(theme.text_primary)
                                                                                        .child("Force native rebuild"),
                                                                                ),
                                                                        ),
                                                                )
                                                                .child(
                                                                    gb_btn("upd-save-cfg", &theme, BtnVariant::Secondary, true, false)
                                                                        .child("Save Source")
                                                                        .on_click(cx.listener(|v, _, _, cx| {
                                                                            v.save_updater_settings(None, cx);
                                                                        })),
                                                                ),
                                                        ),
                                                ),
                                        )
                                        // Update Activity Log
                                        .when(!upd_status.logs.is_empty(), |d| {
                                            d.child(
                                                gb_section_card(&theme)
                                                    .child(
                                                        gb_section_card_head(&theme).child(
                                                            div()
                                                                .text_size(px(13.0))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(theme.text_primary)
                                                                .child(
                                                                    upd_status
                                                                        .step
                                                                        .clone()
                                                                        .unwrap_or_else(|| "Updater Log".into()),
                                                                ),
                                                        ),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("upd-logs-box")
                                                            .max_h(px(220.0))
                                                            .overflow_y_scroll()
                                                            .p(px(12.0))
                                                            .bg(theme.bg_tertiary)
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(11.5))
                                                            .text_color(theme.text_secondary)
                                                            .child(upd_status.logs.join("\n")),
                                                    ),
                                            )
                                        })
                                }
                                SettingsTab::Logs => {
                                    let mut combined = Vec::new();
                                    for l in sup_logs.iter().rev().take(60).rev() {
                                        combined.push(format!("[rust] {l}"));
                                    }
                                    for l in server_logs.iter().rev().take(120).rev() {
                                        combined.push(l.clone());
                                    }
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme)
                                                        .child(
                                                            div()
                                                                .text_size(px(13.5))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(theme.text_primary)
                                                                .child("Diagnostics"),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap(px(8.0))
                                                                .child(
                                                                    gb_btn(
                                                                        "run-doctor-btn",
                                                                        &theme,
                                                                        BtnVariant::Primary,
                                                                        true,
                                                                        !has_client || self.diag_busy,
                                                                    )
                                                                    .child("Run Hermes Doctor")
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.run_diag(false, cx);
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_btn(
                                                                        "run-audit-btn",
                                                                        &theme,
                                                                        BtnVariant::Secondary,
                                                                        true,
                                                                        !has_client || self.diag_busy,
                                                                    )
                                                                    .child("Security Audit")
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.run_diag(true, cx);
                                                                    })),
                                                                ),
                                                        ),
                                                )
                                                .when(!diag_output.is_empty(), |d| {
                                                    d.child(
                                                        div()
                                                            .id("diag-out-scroll")
                                                            .max_h(px(240.0))
                                                            .overflow_y_scroll()
                                                            .p(px(14.0))
                                                            .bg(theme.bg_tertiary)
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(11.5))
                                                            .text_color(theme.text_primary)
                                                            .child(diag_output),
                                                    )
                                                }),
                                        )
                                        .child(
                                            gb_section_card(&theme)
                                                .child(
                                                    gb_section_card_head(&theme)
                                                        .child(
                                                            div()
                                                                .text_size(px(13.5))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(theme.text_primary)
                                                                .child("Backend & Supervisor Logs"),
                                                        )
                                                        .child(
                                                            gb_btn("refresh-logs-btn", &theme, BtnVariant::Ghost, true, !has_client)
                                                                .child(cursor_icon("refresh", 12.5, theme.text_secondary))
                                                                .child("Refresh")
                                                                .on_click(cx.listener(|v, _, _, cx| {
                                                                    v.app.update(cx, |s, cx| s.refresh_capabilities(false, cx));
                                                                })),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id("server-logs-scroll")
                                                        .max_h(px(340.0))
                                                        .overflow_y_scroll()
                                                        .p(px(14.0))
                                                        .bg(theme.bg_tertiary)
                                                        .font_family(FONT_MONO)
                                                        .text_size(px(11.5))
                                                        .text_color(theme.text_secondary)
                                                        .child(if combined.is_empty() {
                                                            "No log lines collected yet.".to_string()
                                                        } else {
                                                            combined.join("\n")
                                                        }),
                                                ),
                                        )
                                }
                            }),
                    ),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(smooth_scroll_layer(&self.smooth_scroller)),
            )
    }
}

fn labeled_input(
    theme: &crate::theme::tokens::SandTheme,
    label: &str,
    input: &Entity<InputState>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(Input::new(input))
}
