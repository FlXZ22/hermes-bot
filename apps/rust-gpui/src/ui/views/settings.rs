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
use crate::ui::smooth_scroll::{smooth_scroll_layer, NestedScrollRegions, SmoothScroller};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement;
use std::collections::HashMap;

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
    config_source: (String, String),
    config_drafts: HashMap<(String, String), String>,
    config_saving: bool,
    env_saving: bool,
    profile_creating: bool,

    // SSH connection form (for remote mode)
    ssh_host_input: Entity<InputState>,
    ssh_user_input: Entity<InputState>,
    ssh_port_input: Entity<InputState>,
    ssh_key_input: Entity<InputState>,
    synced_ssh: [String; 4],
    ssh_saving: bool,

    // Updater controls
    repo_path_input: Entity<InputState>,
    branch_input: Entity<InputState>,
    force_native_rebuild: bool,
    synced_updater: [String; 2],
    updater_busy: bool,
    diag_busy: bool,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
    nested_scroll: NestedScrollRegions,
    log_scroll: [ScrollHandle; 3],
}

impl SettingsView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (cfg, conn, profile, upd) = {
            let s = app.read(cx);
            (
                s.raw_config.clone(),
                s.connection.clone(),
                s.active_profile.clone(),
                s.update_status.clone(),
            )
        };
        if upd.is_none() {
            app.update(cx, |s, cx| s.refresh_updater_status(cx));
        }
        let config_source = (conn.id.clone(), profile);
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
                ssh.port
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "22".into()),
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
            st.set_value(
                upd.as_ref()
                    .and_then(|status| status.repo_path.clone())
                    .unwrap_or_default(),
                window,
                cx,
            );
            st
        });
        let branch_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("main");
            st.set_value(
                upd.as_ref()
                    .map(|status| status.target_branch.clone())
                    .unwrap_or_default(),
                window,
                cx,
            );
            st
        });

        let synced_ssh = ssh_fields(conn.ssh.as_ref());
        let synced_updater = [
            repo_path_input.read(cx).value().to_string(),
            branch_input.read(cx).value().to_string(),
        ];

        cx.observe(&app, |_, app_e, cx| {
            let app = app_e.read(cx);
            if !app.setup_open && !app.initializing && app.active_view == AppViewId::Settings {
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
            config_source,
            config_drafts: HashMap::new(),
            config_saving: false,
            env_saving: false,
            profile_creating: false,
            ssh_host_input,
            ssh_user_input,
            ssh_port_input,
            ssh_key_input,
            synced_ssh,
            ssh_saving: false,
            repo_path_input,
            branch_input,
            force_native_rebuild: false,
            synced_updater,
            updater_busy: false,
            diag_busy: false,
            scroll_handle,
            smooth_scroller,
            nested_scroll: NestedScrollRegions::default(),
            log_scroll: std::array::from_fn(|_| ScrollHandle::new()),
        }
    }

    fn save_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.env_saving {
            return;
        }
        let key = self.env_key_input.read(cx).value().trim().to_string();
        let value = self.env_val_input.read(cx).value().trim().to_string();
        if key.is_empty() || value.is_empty() {
            self.app
                .update(cx, |s, cx| s.show_toast("Enter a key and value.", cx));
            return;
        }
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let rt = app.tokio_rt.clone();
        let generation = app.profile_generation;
        let submitted_key = key.clone();
        let submitted_value = value.clone();
        self.env_saving = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = rt
                .spawn(async move { client.set_env(&submitted_key, &submitted_value).await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update_in(cx, |view, window, cx| {
                view.env_saving = false;
                cx.notify();
                if view.app.read(cx).profile_generation != generation {
                    return;
                }
                match result {
                    Ok(_) => {
                        if view.env_key_input.read(cx).value().trim() == key
                            && view.env_val_input.read(cx).value().trim() == value
                        {
                            view.env_key_input
                                .update(cx, |st, cx| st.set_value("", window, cx));
                            view.env_val_input
                                .update(cx, |st, cx| st.set_value("", window, cx));
                        }
                        view.app.update(cx, |s, cx| {
                            s.refresh_env(cx);
                            s.show_toast(format!("Saved {key}"), cx);
                        });
                    }
                    Err(error) => view.app.update(cx, |s, cx| s.show_toast(error, cx)),
                }
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
                        s.refresh_env(cx);
                    }
                    Err(e) => s.show_toast(e, cx),
                });
            });
        })
        .detach();
    }

    fn create_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.profile_creating {
            return;
        }
        let name = self.profile_name_input.read(cx).value().trim().to_string();
        let description = self.profile_desc_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.app
                .update(cx, |s, cx| s.show_toast("Enter a profile name.", cx));
            return;
        }
        let app = self.app.read(cx);
        let Some(client) = app.client.clone() else {
            return;
        };
        let rt = app.tokio_rt.clone();
        let generation = app.profile_generation;
        let submitted_name = name.clone();
        let submitted_description = description.clone();
        self.profile_creating = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = rt
                .spawn(async move {
                    let description = (!submitted_description.is_empty())
                        .then_some(submitted_description.as_str());
                    client.create_profile(&submitted_name, description).await
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update_in(cx, |view, window, cx| {
                view.profile_creating = false;
                cx.notify();
                if view.app.read(cx).profile_generation != generation {
                    return;
                }
                match result {
                    Ok(_) => {
                        if view.profile_name_input.read(cx).value().trim() == name
                            && view.profile_desc_input.read(cx).value().trim() == description
                        {
                            view.profile_name_input
                                .update(cx, |st, cx| st.set_value("", window, cx));
                            view.profile_desc_input
                                .update(cx, |st, cx| st.set_value("", window, cx));
                        }
                        view.app.update(cx, |s, cx| {
                            s.refresh_profiles(cx);
                            s.show_toast(format!("Created bot profile \"{name}\""), cx);
                        });
                    }
                    Err(error) => view.app.update(cx, |s, cx| s.show_toast(error, cx)),
                }
            });
        })
        .detach();
    }

    fn save_config_yaml(&mut self, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        let source = (app.connection.id.clone(), app.active_profile.clone());
        if self.config_saving || source != self.config_source {
            return;
        }
        let Some(client) = app.client.clone() else {
            return;
        };
        let content = self.config_input.read(cx).value().to_string();
        let submitted = content.clone();
        let rt = app.tokio_rt.clone();
        let generation = app.profile_generation;
        self.config_saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = rt
                .spawn(async move { client.save_raw_config(&submitted).await })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                view.config_saving = false;
                cx.notify();
                if view.app.read(cx).profile_generation != generation {
                    return;
                }
                view.app.update(cx, |s, cx| match result {
                    Ok(_) => {
                        s.raw_config = content;
                        s.show_toast("Saved config.yaml", cx);
                    }
                    Err(error) => s.show_toast(error, cx),
                });
            });
        })
        .detach();
    }

    fn save_ssh_settings(&mut self, cx: &mut Context<Self>) {
        if self.ssh_saving {
            return;
        }
        let [host, user, port_raw, key] = self.ssh_draft(cx).map(|value| value.trim().to_string());
        let port = if host.is_empty() || port_raw.is_empty() {
            None
        } else if let Ok(port) = port_raw.parse::<u16>() {
            if port == 0 {
                self.app.update(cx, |s, cx| {
                    s.show_toast("SSH port must be between 1 and 65535.", cx)
                });
                return;
            }
            Some(port)
        } else {
            self.app.update(cx, |s, cx| {
                s.show_toast("SSH port must be between 1 and 65535.", cx)
            });
            return;
        };
        let app = self.app.read(cx);
        let mut connection = app.connection.clone();
        let source = connection.id.clone();
        connection.ssh = (!host.is_empty()).then(|| SshTarget {
            host,
            user: (!user.is_empty()).then_some(user),
            port,
            identity_file: (!key.is_empty()).then_some(key),
            config_file: connection
                .ssh
                .as_ref()
                .and_then(|old| old.config_file.clone()),
        });
        let rt = app.tokio_rt.clone();
        self.ssh_saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let saved = rt
                .spawn_blocking(move || {
                    let path = crate::backend::remote::connection_path()?;
                    let value =
                        serde_json::to_value(&connection).map_err(|error| error.to_string())?;
                    crate::backend::remote::save_connection(&path, &value)?;
                    Ok::<_, String>(connection.ssh)
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                view.ssh_saving = false;
                cx.notify();
                view.app.update(cx, |s, cx| match saved {
                    Ok(ssh) if s.connection.id == source => {
                        s.connection.ssh = ssh;
                        s.show_toast("Saved SSH target for remote workspace & terminal", cx);
                    }
                    Ok(_) => {}
                    Err(error) => s.show_toast(error, cx),
                });
            });
        })
        .detach();
    }

    fn ssh_draft(&self, cx: &App) -> [String; 4] {
        [
            self.ssh_host_input.read(cx).value().to_string(),
            self.ssh_user_input.read(cx).value().to_string(),
            self.ssh_port_input.read(cx).value().to_string(),
            self.ssh_key_input.read(cx).value().to_string(),
        ]
    }

    fn run_diag(&mut self, security: bool, cx: &mut Context<Self>) {
        if self.diag_busy {
            return;
        }
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
                    let report = if security {
                        client.run_security_audit().await
                    } else {
                        client.run_doctor().await
                    }?;
                    serde_json::to_string_pretty(&report).map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let _ = this.update(cx, |v, cx| {
                v.diag_busy = false;
                v.app.update(cx, |s, cx| match res {
                    Ok(val) => {
                        s.diag_output = val;
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
        if self.updater_busy
            || self
                .app
                .read(cx)
                .update_status
                .as_ref()
                .is_some_and(|status| update_in_progress(&status.phase))
        {
            return;
        }
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
        let app = self.app.read(cx);
        if self.updater_busy
            || app
                .update_status
                .as_ref()
                .is_some_and(|status| update_in_progress(&status.phase))
        {
            return;
        }
        let patch = if let Some(enabled) = auto_check {
            UpdateSettingsPatch {
                target_branch: None,
                repo_path: None,
                auto_check: Some(enabled),
            }
        } else {
            let repo_path = self.repo_path_input.read(cx).value().trim().to_string();
            let branch = self.branch_input.read(cx).value().trim().to_string();
            UpdateSettingsPatch {
                target_branch: (!branch.is_empty()).then_some(branch),
                repo_path: Some(repo_path),
                auto_check: None,
            }
        };
        let rt = app.tokio_rt.clone();
        let updater = app.updater.clone();
        self.updater_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = rt
                .spawn_blocking(move || updater.update_settings(patch))
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let _ = this.update(cx, |view, cx| {
                view.updater_busy = false;
                cx.notify();
                view.app.update(cx, |s, cx| match result {
                    Ok(status) => {
                        s.update_status = Some(status);
                        s.show_toast("Updater settings saved", cx);
                    }
                    Err(error) => s.show_toast(error, cx),
                });
            });
        })
        .detach();
    }

    fn apply_update(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        if self.updater_busy
            || !app.update_status.as_ref().is_some_and(|status| {
                status.repo_valid
                    && !update_in_progress(&status.phase)
                    && status.phase != "restart_required"
            })
        {
            return;
        }
        let force = self.force_native_rebuild;
        let rt = app.tokio_rt.clone();
        let handle = rt.handle().clone();
        let updater = app.updater.clone();
        self.updater_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let status = rt
                .spawn_blocking(move || updater.apply_update_on(&handle, mode.to_string(), force))
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            let started = this
                .update(cx, |view, cx| {
                    view.updater_busy = false;
                    cx.notify();
                    view.app.update(cx, |s, cx| match status {
                        Ok(status) => {
                            let running = update_in_progress(&status.phase);
                            s.update_status = Some(status);
                            s.show_toast("Update started", cx);
                            running
                        }
                        Err(error) => {
                            s.show_toast(error, cx);
                            false
                        }
                    })
                })
                .unwrap_or(false);
            if !started {
                return;
            }
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1000))
                    .await;
                let running = this
                    .update(cx, |view, cx| {
                        if view
                            .app
                            .read(cx)
                            .update_status
                            .as_ref()
                            .is_some_and(|status| update_in_progress(&status.phase))
                        {
                            view.app.update(cx, |s, cx| s.refresh_updater_status(cx));
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
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
            let tabs = self.tab;
            let backend_info = if tabs == SettingsTab::Connections {
                s.rust_backend.as_ref().map(|backend| (backend.pid, backend.url.clone()))
            } else {
                None
            };
            let mut server_logs = Vec::new();
            let mut sup_logs = Vec::new();
            if tabs == SettingsTab::Logs {
                server_logs.extend_from_slice(&s.server_logs[s.server_logs.len().saturating_sub(120)..]);
                if let Some(backend) = s.rust_backend.as_ref() {
                    let start = backend.logs.len().saturating_sub(60);
                    sup_logs.extend(backend.logs.iter().skip(start).cloned());
                }
            }
            (
                s.sand_theme(),
                s.theme_mode(),
                s.active_model.clone(),
                s.connection.clone(),
                backend_info,
                (tabs == SettingsTab::Appearance).then(|| s.system_stats.clone()).flatten(),
                s.bootstrap_ms,
                s.restarting_backend,
                if tabs == SettingsTab::ApiKeys { s.env_entries.clone() } else { Vec::new() },
                if tabs == SettingsTab::Profiles { s.profiles.clone() } else { Vec::new() },
                s.active_profile.clone(),
                if tabs == SettingsTab::Profiles { s.bot_avatars.clone() } else { HashMap::new() },
                (tabs == SettingsTab::Config).then(|| s.raw_config.clone()),
                server_logs,
                sup_logs,
                if tabs == SettingsTab::Logs { s.diag_output.clone() } else { String::new() },
                (tabs == SettingsTab::Updates).then(|| s.update_status.clone()).flatten(),
                s.client.is_some(),
            )
        };

        let source = (conn.id.clone(), active_profile.clone());
        let source_changed = source != self.config_source;
        if let Some(raw_config) = raw_config {
            if source_changed {
                let previous = std::mem::replace(&mut self.config_source, source);
                let draft = self.config_input.read(cx).value().to_string();
                if draft != self.synced_config {
                    self.config_drafts.insert(previous, draft);
                }
                let draft = self.config_drafts.remove(&self.config_source).unwrap_or_else(|| raw_config.clone());
                self.synced_config.clone_from(&raw_config);
                self.config_input.update(cx, |st, cx| st.set_value(draft, window, cx));
            } else if raw_config != self.synced_config {
                let clean = self.config_input.read(cx).value().to_string() == self.synced_config;
                self.synced_config.clone_from(&raw_config);
                if clean {
                    self.config_input.update(cx, |st, cx| st.set_value(raw_config, window, cx));
                }
            }
        }

        let ssh = ssh_fields(conn.ssh.as_ref());
        if ssh != self.synced_ssh {
            let clean = self.ssh_draft(cx) == self.synced_ssh;
            self.synced_ssh = ssh.clone();
            if clean {
                for (input, value) in [
                    &self.ssh_host_input, &self.ssh_user_input, &self.ssh_port_input, &self.ssh_key_input,
                ].into_iter().zip(ssh) {
                    input.update(cx, |st, cx| st.set_value(value, window, cx));
                }
            }
        }

        if let Some(status) = upd_status.as_ref() {
            let next = [status.repo_path.clone().unwrap_or_default(), status.target_branch.clone()];
            if next != self.synced_updater {
                for ((input, old), value) in [&self.repo_path_input, &self.branch_input]
                    .into_iter().zip(&self.synced_updater).zip(&next)
                {
                    if input.read(cx).value().to_string() == *old {
                        input.update(cx, |st, cx| st.set_value(value.clone(), window, cx));
                    }
                }
                self.synced_updater = next;
            }
        }

        let sidebar_width = if self.app.read(cx).sidebar_collapsed { px(68.0) } else { px(280.0) };
        let inspector_reserve = if self.app.read(cx).right_pane_open { px(360.0) } else { px(0.0) };
        let narrow = window.viewport_size().width - sidebar_width - inspector_reserve < px(580.0);
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
                    .child(self.nested_scroll.reset_marker())
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
                            .flex_shrink_0()
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
                                            if v.tab != tab_val {
                                                v.tab = tab_val;
                                                v.smooth_scroller.cancel();
                                                v.scroll_handle.set_offset(Point::default());
                                                if tab_val == SettingsTab::Updates && v.app.read(cx).update_status.is_none() {
                                                    v.app.update(cx, |s, cx| s.refresh_updater_status(cx));
                                                }
                                                cx.notify();
                                            }
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
                                                .child({
                                                    let reduced = self.app.read(cx).reduced_motion();
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
                                                                        .child("Reduced Motion"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.0))
                                                                        .text_color(theme.text_secondary)
                                                                        .child("Freeze inline face animations on static state expressions while preserving state cues."),
                                                                ),
                                                        )
                                                        .child(
                                                            gb_toggle_switch(
                                                                "settings-reduced-motion",
                                                                &theme,
                                                                reduced,
                                                            )
                                                            .on_click(cx.listener(move |v, _, _, cx| {
                                                                v.app.update(cx, |s, cx| {
                                                                    s.set_reduced_motion(!reduced, cx);
                                                                });
                                                            })),
                                                        )
                                                })
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
                                                                            Some((pid, url)) => format!(
                                                                                "Managed PID {:?} · {}",
                                                                                pid,
                                                                                url.as_deref().unwrap_or("starting")
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
                                                                                s.bump_shell();
                                                                                cx.notify();
                                                                            });
                                                                        })),
                                                                )
                                                                .child(
                                                                    gb_btn("refresh-conn-btn", &theme, BtnVariant::Secondary, true, !has_client)
                                                                        .child("Refresh")
                                                                        .on_click(cx.listener(|v, _, _, cx| {
                                                                            v.app.update(cx, |s, cx| s.refresh_core_capabilities(true, cx));
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
                                                                .grid_cols(if narrow { 1 } else { 2 })
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
                                                                    gb_btn("save-ssh-btn", &theme, BtnVariant::Primary, true, self.ssh_saving)
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
                                                    let seed = pname.clone();
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
                                                                            s.bump_shell();
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
                                                        gb_btn("save-yaml-btn", &theme, BtnVariant::Primary, true, !has_client || self.config_saving)
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
                                                    .relative()
                                                    .child(self.nested_scroll.mark_nested())
                                                    .child(Textarea::new(&self.config_input)),
                                            ),
                                    )
                                }
                                SettingsTab::Updates => {
                                    if let Some(upd_status) = upd_status.as_ref() {
                                    let updating = update_in_progress(&upd_status.phase);
                                    let restart_required = upd_status.phase == "restart_required";
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
                                                            if restart_required || upd_status.update_available {
                                                                BadgeTone::Warning
                                                            } else {
                                                                BadgeTone::Success
                                                            },
                                                            if restart_required {
                                                                "Restart required".into()
                                                            } else if upd_status.update_available {
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
                                                                        self.updater_busy || updating || restart_required || !upd_status.repo_valid,
                                                                    )
                                                                    .when(updating, |d| d.child(gpu_spinner(12.0, theme.text_on_primary)))
                                                                    .child(if updating { "Updating…" } else { "Update Now" })
                                                                    .on_click(cx.listener(|v, _, _, cx| {
                                                                        v.apply_update("remote", cx);
                                                                    })),
                                                                )
                                                                .child(
                                                                    gb_btn(
                                                                        "upd-local",
                                                                        &theme,
                                                                        BtnVariant::Secondary,
                                                                        true,
                                                                        self.updater_busy || updating || restart_required || !upd_status.repo_valid,
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
                                                                .grid_cols(if narrow { 1 } else { 2 })
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
                                                                                .child({
                                                                                    let next_auto_check = !upd_status.auto_check;
                                                                                    gb_toggle_switch(
                                                                                        "upd-auto-check",
                                                                                        &theme,
                                                                                        upd_status.auto_check,
                                                                                    )
                                                                                    .on_click(cx.listener(
                                                                                        move |v, _, _, cx| {
                                                                                            v.save_updater_settings(
                                                                                                Some(next_auto_check),
                                                                                                cx,
                                                                                            );
                                                                                        },
                                                                                    ))
                                                                                })
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
                                                        log_panel("upd-logs-box", upd_status.logs.join("\n"), px(220.0), theme.bg_tertiary, theme.text_secondary, &self.log_scroll[0], &self.nested_scroll),
                                                    ),
                                            )
                                        })
                                    } else {
                                        gb_section_card(&theme).child(div().p(px(16.0)).child("Loading updater status…"))
                                    }
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
                                                        log_panel("diag-out-scroll", diag_output, px(240.0), theme.bg_tertiary, theme.text_primary, &self.log_scroll[1], &self.nested_scroll),
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
                                                                    v.app.update(cx, |s, cx| s.refresh_logs(cx));
                                                                })),
                                                        ),
                                                )
                                                .child(
                                                    log_panel("server-logs-scroll", if combined.is_empty() { "No log lines collected yet.".to_string() } else { combined.join("\n") }, px(340.0), theme.bg_tertiary, theme.text_secondary, &self.log_scroll[2], &self.nested_scroll),
                                                ),
                                        )
                                }
                            }),
                    ),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                    .child(smooth_scroll_layer(&self.smooth_scroller).yield_to(&self.nested_scroll)),
            )
    }
}

fn update_in_progress(phase: &str) -> bool {
    !matches!(phase, "idle" | "done" | "error" | "restart_required")
}

fn ssh_fields(ssh: Option<&SshTarget>) -> [String; 4] {
    [
        ssh.map(|target| target.host.clone()).unwrap_or_default(),
        ssh.and_then(|target| target.user.clone()).unwrap_or_default(),
        ssh.and_then(|target| target.port).unwrap_or(22).to_string(),
        ssh.and_then(|target| target.identity_file.clone()).unwrap_or_default(),
    ]
}

fn log_panel(
    id: &'static str,
    text: String,
    max_height: Pixels,
    background: Hsla,
    foreground: Hsla,
    scroll: &ScrollHandle,
    nested: &NestedScrollRegions,
) -> impl IntoElement {
    let handle = scroll.clone();
    div()
        .id(id)
        .relative()
        .w_full()
        .on_scroll(move |event, window, cx| {
            let delta = event.delta.pixel_delta(window.line_height()).y;
            let max = handle.max_offset().y;
            let offset = handle.offset().y;
            // The inner Div has already applied the delta in the bubble phase.
            let previous = offset - delta;
            if max > px(0.0) && ((delta < px(0.0) && previous > -max) || (delta > px(0.0) && previous < px(0.0))) {
                cx.stop_propagation();
            }
        })
        .child(nested.mark_nested())
        .child(
            div()
                .id("content")
                .track_scroll(scroll)
                .w_full()
                .max_h(max_height)
                .overflow_y_scroll()
                .restrict_scroll_to_axis()
                .p(px(12.0))
                .bg(background)
                .font_family(FONT_MONO)
                .text_size(px(11.5))
                .text_color(foreground)
                .child(text),
        )
        .vertical_scrollbar(scroll)
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
