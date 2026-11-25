use crate::backend::ssh::SshTarget;
use crate::hermes::client::HermesClient;
use crate::hermes::types::{AuthProviderInfo, HermesAuthType, HermesConnection, HermesServerMode};
use crate::state::app_state::{AppState, AppViewId, ConnectionStatus};
use crate::theme::bot_face::{bot_face, BotFaceState};
use crate::theme::components::{
    cursor_icon, gb_btn, gb_traffic_lights, BtnVariant, FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::{SandTheme, ThemeMode};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::input::{Input, InputState};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WelcomeStep {
    Welcome,
    Choose,
    Remote,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteAuthKind {
    Browser,
    Bearer,
    SessionToken,
    None,
}

impl RemoteAuthKind {
    fn label(self) -> &'static str {
        match self {
            Self::Browser => "Sign in with browser · Recommended",
            Self::Bearer => "Access token",
            Self::SessionToken => "Session token · Local or SSH tunnel",
            Self::None => "No authentication · Trusted network",
        }
    }
}

pub struct WelcomeView {
    app: Entity<AppState>,
    step: WelcomeStep,
    auth: RemoteAuthKind,
    url_input: Entity<InputState>,
    name_input: Entity<InputState>,
    token_input: Entity<InputState>,
    ssh_enabled: bool,
    ssh_open: bool,
    ssh_host: Entity<InputState>,
    ssh_user: Entity<InputState>,
    ssh_port: Entity<InputState>,
    ssh_identity: Entity<InputState>,
    ssh_config: Entity<InputState>,
    help_open: bool,
    busy: String,
    error: String,
    login_url: String,
    providers: Vec<AuthProviderInfo>,
    selected_provider: String,
    copied_login: bool,
    op_gen: u64,
}

impl WelcomeView {
    pub fn new(app: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (init_step, init_url, init_name, init_ssh) = {
            let s = app.read(cx);
            let st = if s.connection_status == ConnectionStatus::Connected {
                WelcomeStep::Choose
            } else if s.connection_status == ConnectionStatus::Error && !s.connection.is_local() {
                WelcomeStep::Remote
            } else {
                WelcomeStep::Welcome
            };
            let u = if s.connection.is_local() {
                String::new()
            } else {
                s.connection.url.clone()
            };
            let n = if s.connection.is_local() {
                String::new()
            } else {
                s.connection.name.clone()
            };
            (st, u, n, s.connection.ssh.clone())
        };

        let url_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("https://hermes.example.com");
            if !init_url.is_empty() {
                st.set_value(init_url, window, cx);
            }
            st
        });
        let name_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("My server");
            if !init_name.is_empty() {
                st.set_value(init_name, window, cx);
            }
            st
        });
        let token_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Paste your server token")
                .masked(true)
        });
        let ssh_host = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("my-server or 192.168.1.20");
            if let Some(ref ssh) = init_ssh {
                st.set_value(ssh.host.clone(), window, cx);
            }
            st
        });
        let ssh_user = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("optional (uses SSH config)");
            if let Some(ref u) = init_ssh.as_ref().and_then(|s| s.user.clone()) {
                st.set_value(u, window, cx);
            }
            st
        });
        let ssh_port = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("22");
            if let Some(p) = init_ssh.as_ref().and_then(|s| s.port) {
                st.set_value(p.to_string(), window, cx);
            }
            st
        });
        let ssh_identity = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("~/.ssh/id_ed25519 (optional)");
            if let Some(ref i) = init_ssh.as_ref().and_then(|s| s.identity_file.clone()) {
                st.set_value(i, window, cx);
            }
            st
        });
        let ssh_config = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("~/.ssh/config (optional)");
            if let Some(ref c) = init_ssh.as_ref().and_then(|s| s.config_file.clone()) {
                st.set_value(c, window, cx);
            }
            st
        });

        cx.observe(&app, |this, app, cx| {
            let s = app.read(cx);
            if !s.initializing && s.connection_status == ConnectionStatus::Connected {
                if !this.busy.is_empty() {
                    this.busy.clear();
                    this.error.clear();
                    this.step = WelcomeStep::Ready;
                }
            } else if !s.initializing && s.connection_status == ConnectionStatus::Error {
                if !this.busy.is_empty() {
                    this.busy.clear();
                    this.error = s.connection_detail.clone();
                }
            }
            cx.notify();
        })
        .detach();

        Self {
            app,
            step: init_step,
            auth: RemoteAuthKind::Browser,
            url_input,
            name_input,
            token_input,
            ssh_enabled: init_ssh.is_some(),
            ssh_open: false,
            ssh_host,
            ssh_user,
            ssh_port,
            ssh_identity,
            ssh_config,
            help_open: false,
            busy: String::new(),
            error: String::new(),
            login_url: String::new(),
            providers: Vec::new(),
            selected_provider: String::new(),
            copied_login: false,
            op_gen: 0,
        }
    }

    fn use_local(&mut self, cx: &mut Context<Self>) {
        self.error.clear();
        self.busy = "Starting Hermes…".into();
        self.app
            .update(cx, |s, cx| s.start_local_connection(false, cx));
        cx.notify();
    }

    fn connect_remote(&mut self, cx: &mut Context<Self>) {
        let raw_url = self.url_input.read(cx).value().trim().to_string();
        if raw_url.is_empty() {
            self.error = "Enter a complete server URL, such as https://hermes.example.com".into();
            cx.notify();
            return;
        }
        let normalized_url = match crate::remote::server_url(&raw_url) {
            Ok(u) => u.as_str().trim_end_matches('/').to_string(),
            Err(e) => {
                self.error = e;
                cx.notify();
                return;
            }
        };
        let host_label = reqwest::Url::parse(&normalized_url)
            .ok()
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_else(|| "Server".into());
        let name_val = self.name_input.read(cx).value().trim().to_string();
        let conn_name = if name_val.is_empty() {
            host_label
        } else {
            name_val
        };
        let token_val = self.token_input.read(cx).value().trim().to_string();
        let ssh = if self.ssh_enabled {
            let h = self.ssh_host.read(cx).value().trim().to_string();
            if h.is_empty() {
                self.error = "Enter an SSH hostname or disable SSH workspace tools.".into();
                cx.notify();
                return;
            }
            let port_str = self.ssh_port.read(cx).value().trim().to_string();
            let port = if port_str.is_empty() {
                None
            } else {
                match port_str.parse::<u16>() {
                    Ok(p) if p > 0 => Some(p),
                    _ => {
                        self.error = "SSH port must be between 1 and 65535.".into();
                        cx.notify();
                        return;
                    }
                }
            };
            let opt = |e: &Entity<InputState>| {
                let v = e.read(cx).value().trim().to_string();
                if v.is_empty() { None } else { Some(v) }
            };
            Some(SshTarget {
                host: h,
                user: opt(&self.ssh_user),
                port,
                identity_file: opt(&self.ssh_identity),
                config_file: opt(&self.ssh_config),
            })
        } else {
            None
        };

        let auth = self.auth;
        if matches!(auth, RemoteAuthKind::Bearer | RemoteAuthKind::SessionToken)
            && token_val.is_empty()
        {
            self.error = "Paste the access token from your Hermes server.".into();
            cx.notify();
            return;
        }

        self.error.clear();
        self.copied_login = false;
        self.op_gen = self.op_gen.wrapping_add(1);
        let gen = self.op_gen;

        let mut candidate = HermesConnection {
            id: format!("remote:{normalized_url}"),
            name: conn_name,
            url: normalized_url.clone(),
            mode: HermesServerMode::Dashboard,
            auth_type: match auth {
                RemoteAuthKind::Browser | RemoteAuthKind::Bearer => HermesAuthType::Bearer,
                RemoteAuthKind::SessionToken => HermesAuthType::SessionToken,
                RemoteAuthKind::None => HermesAuthType::None,
            },
            token: match auth {
                RemoteAuthKind::Bearer | RemoteAuthKind::SessionToken => Some(token_val),
                _ => None,
            },
            ssh,
            ..Default::default()
        };

        if auth != RemoteAuthKind::Browser {
            self.busy = "Checking access and live chat…".into();
            self.app
                .update(cx, |s, cx| s.connect_to_hermes(candidate, false, cx));
            cx.notify();
            return;
        }

        // Native PKCE browser login
        self.busy = "Finding sign-in options…".into();
        let rt = self.app.read(cx).tokio_rt.clone();
        let remote_login = self.app.read(cx).remote_login.clone();
        let preselected = self.selected_provider.clone();
        let existing_providers = self.providers.clone();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let c_check = candidate.clone();
            let provider_res = rt
                .spawn(async move {
                    if !preselected.is_empty() && !existing_providers.is_empty() {
                        return Ok((existing_providers, preselected));
                    }
                    let client = HermesClient::new(c_check, None);
                    let list = client.get_auth_providers().await?;
                    if list.is_empty() {
                        return Err("This server has no browser sign-in configured. Use an access token or an SSH tunnel.".into());
                    }
                    let first = list[0].name.clone();
                    Ok((list, first))
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let (list, selected) = match provider_res {
                Ok(v) => v,
                Err(err) => {
                    let _ = this.update(cx, |v, cx| {
                        if v.op_gen == gen {
                            v.busy.clear();
                            v.error = err;
                            cx.notify();
                        }
                    });
                    return;
                }
            };

            let needs_pick = list.len() > 1 && this.read_with(cx, |v, _| v.selected_provider.is_empty()).unwrap_or(false);
            if needs_pick {
                let _ = this.update(cx, |v, cx| {
                    if v.op_gen == gen {
                        v.providers = list;
                        v.selected_provider = selected;
                        v.busy.clear();
                        cx.notify();
                    }
                });
                return;
            }

            let addr_c = normalized_url.clone();
            let sel_c = selected.clone();
            let rl = remote_login.clone();
            let login_res = rt
                .spawn(async move { rl.begin(addr_c, sel_c).await })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));

            let auth_url = match login_res {
                Ok(u) => u,
                Err(err) => {
                    let _ = this.update(cx, |v, cx| {
                        if v.op_gen == gen {
                            v.busy.clear();
                            v.error = err;
                            cx.notify();
                        }
                    });
                    return;
                }
            };

            let _ = this.update(cx, |v, cx| {
                if v.op_gen == gen {
                    v.providers = list;
                    v.selected_provider = selected;
                    v.login_url = auth_url;
                    v.busy = "Finish signing in in your browser…".into();
                    cx.notify();
                }
            });

            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(600))
                    .await;
                let still_active = this.read_with(cx, |v, _| v.op_gen == gen).unwrap_or(false);
                if !still_active {
                    return;
                }
                let rl2 = remote_login.clone();
                let polled = rt
                    .spawn(async move { rl2.poll().await })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                match polled {
                    Ok(Some(tokens)) => {
                        candidate.token = tokens
                            .get("access_token")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        candidate.refresh_token = tokens
                            .get("refresh_token")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        candidate.expires_at =
                            tokens.get("expires_at").and_then(|v| v.as_f64());
                        candidate.auth_provider = tokens
                            .get("provider")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        let _ = this.update(cx, |v, cx| {
                            if v.op_gen == gen {
                                v.login_url.clear();
                                v.busy = "Checking access and live chat…".into();
                                v.app.update(cx, |s, cx| s.connect_to_hermes(candidate, false, cx));
                                cx.notify();
                            }
                        });
                        return;
                    }
                    Ok(None) => continue,
                    Err(err) => {
                        let _ = this.update(cx, |v, cx| {
                            if v.op_gen == gen {
                                v.login_url.clear();
                                v.busy.clear();
                                v.error = err;
                                cx.notify();
                            }
                        });
                        return;
                    }
                }
            }
        })
        .detach();
    }

    fn cancel_login(&mut self, cx: &mut Context<Self>) {
        self.op_gen = self.op_gen.wrapping_add(1);
        self.login_url.clear();
        self.busy.clear();
        let rl = self.app.read(cx).remote_login.clone();
        let rt = self.app.read(cx).tokio_rt.clone();
        rt.spawn(async move {
            rl.cancel().await;
        });
        cx.notify();
    }
}

impl Render for WelcomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.app.read(cx);
        let theme = s.sand_theme();
        let mode = s.theme_mode();
        let initializing = s.initializing;
        let conn_status = s.connection_status;
        let conn_detail = s.connection_detail.clone();
        let conn_name = s.connection.name.clone();
        let sessions_count = s.sessions.len();
        let profiles_count = s.profiles.len();
        let active_model = s.active_model.clone();
        let active_provider = s.active_provider.clone();
        let model_ready = s.models.iter().any(|m| {
            m.configured
                && (m.id == active_model || m.id.ends_with(&format!("/{active_model}")))
                && (active_provider == "auto" || m.provider == active_provider)
        });
        let tick = s.anim_tick;

        let step = self.step;
        let is_busy = !self.busy.is_empty();
        let title = if initializing {
            "Opening Hermes…"
        } else {
            match step {
                WelcomeStep::Welcome => "Meet Hermes.",
                WelcomeStep::Choose => "Where does Hermes live?",
                WelcomeStep::Remote => "Your Hermes. Anywhere.",
                WelcomeStep::Ready => "You’re connected.",
            }
        };
        let subtitle = if initializing {
            conn_detail.clone()
        } else {
            match step {
                WelcomeStep::Welcome => "A little help. A lot of possibility.".into(),
                WelcomeStep::Choose => {
                    "Work on this computer, or pick up with your agent on a server.".into()
                }
                WelcomeStep::Remote => {
                    "Bring your sessions, boards, skills, and models with you.".into()
                }
                WelcomeStep::Ready => {
                    format!("Connected to {conn_name}. Your workspace is ready to explore.")
                }
            }
        };

        let show_friends = matches!(step, WelcomeStep::Welcome | WelcomeStep::Choose);
        let hero_size = if step == WelcomeStep::Welcome {
            96.0
        } else {
            74.0
        };
        let hero_color = if step == WelcomeStep::Ready {
            "blue"
        } else {
            "black"
        };
        let hero_state = if is_busy || initializing {
            BotFaceState::Orbit
        } else if step == WelcomeStep::Ready {
            BotFaceState::Celebrate
        } else {
            BotFaceState::Idle
        };

        div()
            .size_full()
            .bg(theme.bg_chrome)
            .text_color(theme.text_primary)
            .font_family(FONT_SANS)
            .flex()
            .flex_col()
            .overflow_hidden()
            // Top window bar
            .child(
                div()
                    .h(px(52.0))
                    .w_full()
                    .px(px(22.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_shrink_0()
                    .child(gb_traffic_lights())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .when(
                                conn_status == ConnectionStatus::Connected
                                    && step != WelcomeStep::Ready
                                    && !is_busy,
                                |d| {
                                    d.child(
                                        gb_btn("welcome-back-app", &theme, BtnVariant::Ghost, true, false)
                                            .child("Back to Hermes")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app.update(cx, |s, cx| {
                                                    s.setup_open = false;
                                                    cx.notify();
                                                });
                                            })),
                                    )
                                },
                            )
                            .child(
                                div()
                                    .id("welcome-theme-toggle")
                                    .w(px(30.0))
                                    .h(px(30.0))
                                    .rounded(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme.bg_hover))
                                    .child(cursor_icon(
                                        if mode == ThemeMode::Dark { "sun" } else { "moon" },
                                        16.0,
                                        theme.text_primary,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app.update(cx, |s, cx| s.toggle_theme(cx));
                                    })),
                            ),
                    ),
            )
            // Scrollable centered onboarding stage
            .child(
                div()
                    .id("welcome-scroll")
                    .flex_1()
                    .w_full()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px(px(24.0))
                    .py(px(24.0))
                    .child(
                        div()
                            .w(px(460.0))
                            .max_w_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            // Back button
                            .when(
                                !initializing
                                    && step != WelcomeStep::Welcome
                                    && step != WelcomeStep::Ready,
                                |d| {
                                    d.child(
                                        div()
                                            .w_full()
                                            .mb(px(12.0))
                                            .flex()
                                            .justify_start()
                                            .child(
                                                div()
                                                    .id("welcome-step-back")
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(7.0))
                                                    .text_size(px(12.0))
                                                    .text_color(theme.text_secondary)
                                                    .cursor_pointer()
                                                    .child(cursor_icon("arrow-left", 14.0, theme.text_secondary))
                                                    .child("Back")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        if !this.busy.is_empty() {
                                                            return;
                                                        }
                                                        this.error.clear();
                                                        this.step = if this.step == WelcomeStep::Remote {
                                                            WelcomeStep::Choose
                                                        } else {
                                                            WelcomeStep::Welcome
                                                        };
                                                        cx.notify();
                                                    })),
                                            ),
                                    )
                                },
                            )
                            // Animated 3-bot hero cluster (identical to verification-welcome-dark.png)
                            .child(
                                div()
                                    .w(px(250.0))
                                    .h(px(if step == WelcomeStep::Remote { 86.0 } else { 112.0 }))
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(show_friends, |d| {
                                        d.child(
                                            div()
                                                .absolute()
                                                .left(px(18.0))
                                                .top(px(9.0))
                                                .child(bot_face(
                                                    &theme,
                                                    "welcome-red",
                                                    Some("blob"),
                                                    Some("red"),
                                                    42.0,
                                                    BotFaceState::Idle,
                                                    tick,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .right(px(18.0))
                                                .top(px(62.0))
                                                .child(bot_face(
                                                    &theme,
                                                    "welcome-cyan",
                                                    Some("blob"),
                                                    Some("cyan"),
                                                    34.0,
                                                    BotFaceState::Idle,
                                                    tick,
                                                )),
                                        )
                                    })
                                    .child(bot_face(
                                        &theme,
                                        "welcome",
                                        Some("blob"),
                                        Some(hero_color),
                                        hero_size,
                                        hero_state,
                                        tick,
                                    )),
                            )
                            // Heading
                            .child(
                                div()
                                    .mt(px(14.0))
                                    .mb(px(10.0))
                                    .text_size(px(if step == WelcomeStep::Remote { 32.0 } else { 36.0 }))
                                    .line_height(px(40.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_center()
                                    .child(title),
                            )
                            // Subtitle
                            .child(
                                div()
                                    .mb(px(24.0))
                                    .max_w(px(390.0))
                                    .text_size(px(15.0))
                                    .line_height(px(23.0))
                                    .text_color(theme.text_secondary)
                                    .text_center()
                                    .child(subtitle),
                            )
                            // Step body
                            .when(!initializing && step == WelcomeStep::Welcome, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(px(18.0))
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .text_color(theme.text_secondary)
                                                .text_center()
                                                .child("Chat, build, and get things done with your own agent."),
                                        )
                                        .child(
                                            gb_btn("welcome-get-started", &theme, BtnVariant::Primary, false, false)
                                                .min_w(px(180.0))
                                                .h(px(42.0))
                                                .px(px(22.0))
                                                .gap(px(12.0))
                                                .child("Get started")
                                                .child(cursor_icon("arrow-right", 15.0, theme.text_on_primary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.step = WelcomeStep::Choose;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("welcome-connect-link")
                                                .text_size(px(12.0))
                                                .text_color(theme.text_secondary)
                                                .cursor_pointer()
                                                .hover(|s| s.text_color(theme.text_primary))
                                                .child("Already running Hermes? Connect your server")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.step = WelcomeStep::Remote;
                                                    cx.notify();
                                                })),
                                        ),
                                )
                            })
                            .when(!initializing && step == WelcomeStep::Choose, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .id("welcome-choice-local")
                                                .w_full()
                                                .p(px(22.0))
                                                .rounded(px(16.0))
                                                .bg(theme.bg_pane)
                                                .border_1()
                                                .border_color(theme.border_default)
                                                .flex()
                                                .items_center()
                                                .gap(px(18.0))
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme.bg_hover).border_color(theme.border_primary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    if this.busy.is_empty() {
                                                        this.use_local(cx);
                                                    }
                                                }))
                                                .child(
                                                    div()
                                                        .w(px(42.0))
                                                        .h(px(42.0))
                                                        .rounded(px(12.0))
                                                        .bg(theme.bg_card)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(cursor_icon("laptop", 22.0, theme.text_primary)),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(4.0))
                                                        .child(
                                                            div()
                                                                .text_size(px(15.0))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .child("This computer"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(12.0))
                                                                .line_height(px(18.0))
                                                                .text_color(theme.text_secondary)
                                                                .child("Start your installed Hermes Agent. Your work stays on this device."),
                                                        ),
                                                )
                                                .child(cursor_icon("arrow-right", 16.0, theme.text_secondary)),
                                        )
                                        .child(
                                            div()
                                                .id("welcome-choice-remote")
                                                .w_full()
                                                .p(px(22.0))
                                                .rounded(px(16.0))
                                                .bg(theme.bg_pane)
                                                .border_1()
                                                .border_color(theme.border_default)
                                                .flex()
                                                .items_center()
                                                .gap(px(18.0))
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme.bg_hover).border_color(theme.border_primary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    if this.busy.is_empty() {
                                                        this.error.clear();
                                                        this.step = WelcomeStep::Remote;
                                                        cx.notify();
                                                    }
                                                }))
                                                .child(
                                                    div()
                                                        .w(px(42.0))
                                                        .h(px(42.0))
                                                        .rounded(px(12.0))
                                                        .bg(theme.bg_card)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(cursor_icon("server", 22.0, theme.text_primary)),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(4.0))
                                                        .child(
                                                            div()
                                                                .text_size(px(15.0))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .child("Connect a server"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(12.0))
                                                                .line_height(px(18.0))
                                                                .text_color(theme.text_secondary)
                                                                .child("Your own server, VPS, or home lab. Continue where your agent left off."),
                                                        ),
                                                )
                                                .child(cursor_icon("arrow-right", 16.0, theme.text_secondary)),
                                        ),
                                )
                            })
                            .when(!initializing && step == WelcomeStep::Remote, |d| {
                                d.child(self.render_remote_form(&theme, is_busy, cx))
                            })
                            .when(!initializing && step == WelcomeStep::Ready, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.0))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(12.0))
                                                .text_size(px(13.0))
                                                .child(cursor_icon("check", 16.0, theme.success_text))
                                                .child("Server access and live chat verified"),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(12.0))
                                                .text_size(px(13.0))
                                                .child(cursor_icon("check", 16.0, theme.success_text))
                                                .child(format!(
                                                    "{sessions_count} saved sessions · {profiles_count} {}",
                                                    if profiles_count == 1 { "profile" } else { "profiles" }
                                                )),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(12.0))
                                                .text_size(px(13.0))
                                                .text_color(theme.text_secondary)
                                                .child(cursor_icon(
                                                    if model_ready { "check" } else { "info" },
                                                    16.0,
                                                    if model_ready { theme.success_text } else { theme.text_secondary },
                                                ))
                                                .child(if model_ready {
                                                    format!("Model: {active_model}")
                                                } else {
                                                    "Choose a configured model before your first message.".into()
                                                }),
                                        )
                                        .child(
                                            gb_btn("welcome-open-app", &theme, BtnVariant::Primary, false, false)
                                                .w_full()
                                                .h(px(42.0))
                                                .mt(px(8.0))
                                                .child("Open Hermes")
                                                .child(cursor_icon("arrow-right", 15.0, theme.text_on_primary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.app.update(cx, |s, cx| {
                                                        s.active_view = AppViewId::Chat;
                                                        s.setup_open = false;
                                                        cx.notify();
                                                    });
                                                })),
                                        )
                                        .when(!model_ready, |d| {
                                            d.child(
                                                div()
                                                    .id("welcome-choose-model")
                                                    .w_full()
                                                    .text_center()
                                                    .text_size(px(12.0))
                                                    .text_color(theme.text_secondary)
                                                    .cursor_pointer()
                                                    .child("Choose a model")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.app.update(cx, |s, cx| {
                                                            s.active_view = AppViewId::Chat;
                                                            s.setup_open = false;
                                                            s.model_picker_open = true;
                                                            cx.notify();
                                                        });
                                                    })),
                                            )
                                        }),
                                )
                            })
                            // Progress line
                            .when(is_busy || initializing, |d| {
                                d.child(
                                    div()
                                        .mt(px(20.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(10.0))
                                        .text_size(px(13.0))
                                        .text_color(theme.text_secondary)
                                        .child(cursor_icon("spinner", 16.0, theme.text_secondary))
                                        .child(if is_busy { self.busy.clone() } else { conn_detail }),
                                )
                            })
                            // PKCE sign-in actions
                            .when(!self.login_url.is_empty(), |d| {
                                d.child(
                                    div()
                                        .mt(px(14.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(24.0))
                                        .child(
                                            div()
                                                .id("welcome-copy-login")
                                                .text_size(px(12.0))
                                                .text_color(theme.text_secondary)
                                                .cursor_pointer()
                                                .child(if self.copied_login {
                                                    "Sign-in link copied"
                                                } else {
                                                    "Copy sign-in link"
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                                        this.login_url.clone(),
                                                    ));
                                                    this.copied_login = true;
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("welcome-cancel-login")
                                                .text_size(px(12.0))
                                                .text_color(theme.text_secondary)
                                                .cursor_pointer()
                                                .child("Cancel sign-in")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.cancel_login(cx);
                                                })),
                                        ),
                                )
                            })
                            // Error banner
                            .when(!self.error.is_empty() && !initializing, |d| {
                                d.child(
                                    div()
                                        .w_full()
                                        .mt(px(18.0))
                                        .p(px(14.0))
                                        .rounded(px(10.0))
                                        .bg(theme.danger_soft)
                                        .text_color(theme.danger_text)
                                        .text_size(px(12.0))
                                        .line_height(px(19.0))
                                        .flex()
                                        .items_start()
                                        .gap(px(10.0))
                                        .child(cursor_icon("info", 16.0, theme.danger_text))
                                        .child(div().flex_1().child(self.error.clone())),
                                )
                            }),
                    ),
            )
            // Footer
            .child(
                div()
                    .w_full()
                    .px(px(24.0))
                    .py(px(18.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .flex_shrink_0()
                    .child("Hermes for Linux")
                    .child("Your agent. Your infrastructure."),
            )
    }
}

impl WelcomeView {
    fn render_remote_form(
        &self,
        theme: &SandTheme,
        is_busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let auth_options = [
            RemoteAuthKind::Browser,
            RemoteAuthKind::Bearer,
            RemoteAuthKind::SessionToken,
            RemoteAuthKind::None,
        ];

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(15.0))
            // Server address
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
                            .child("Server address"),
                    )
                    .child(Input::new(&self.url_input).h(px(40.0)).disabled(is_busy))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text_tertiary)
                            .child("Use your Hermes dashboard address. An SSH tunnel works too."),
                    ),
            )
            // Connection name
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_secondary)
                                    .child("Connection name"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_tertiary)
                                    .child("optional"),
                            ),
                    )
                    .child(Input::new(&self.name_input).h(px(40.0)).disabled(is_busy)),
            )
            // Sign-in method selector pills
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
                            .child("Sign-in method"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .children(auth_options.into_iter().map(|opt| {
                                let active = self.auth == opt;
                                div()
                                    .id(SharedString::from(format!("auth-opt-{:?}", opt)))
                                    .h(px(36.0))
                                    .px(px(12.0))
                                    .rounded(px(10.0))
                                    .bg(if active { theme.accent_soft } else { theme.bg_pane })
                                    .border_1()
                                    .border_color(if active {
                                        theme.accent_primary
                                    } else {
                                        theme.border_default
                                    })
                                    .text_size(px(13.0))
                                    .text_color(if active {
                                        theme.accent_text
                                    } else {
                                        theme.text_primary
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .cursor_pointer()
                                    .child(opt.label())
                                    .when(active, |d| {
                                        d.child(cursor_icon("check", 14.0, theme.accent_text))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.busy.is_empty() {
                                            this.auth = opt;
                                            cx.notify();
                                        }
                                    }))
                            })),
                    ),
            )
            // Browser hint OR Token field
            .when(self.auth == RemoteAuthKind::Browser, |d| {
                d.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .text_color(theme.text_tertiary)
                        .child("Your browser will open for your server’s password or SSO sign-in. Return here when you’re done."),
                )
            })
            .when(
                matches!(self.auth, RemoteAuthKind::Bearer | RemoteAuthKind::SessionToken),
                |d| {
                    let lbl = if self.auth == RemoteAuthKind::Bearer {
                        "Access token"
                    } else {
                        "Hermes session token"
                    };
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_secondary)
                                    .child(lbl),
                            )
                            .child(Input::new(&self.token_input).h(px(40.0)).disabled(is_busy)),
                    )
                },
            )
            // Optional SSH collapsible
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("welcome-ssh-toggle")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(12.0))
                            .text_color(theme.text_secondary)
                            .cursor_pointer()
                            .child(cursor_icon(
                                if self.ssh_open { "chevron-down" } else { "chevron-right" },
                                12.0,
                                theme.text_secondary,
                            ))
                            .child("SSH access for terminals and Git (optional)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ssh_open = !this.ssh_open;
                                cx.notify();
                            })),
                    )
                    .when(self.ssh_open, |d| {
                        d.child(
                            div()
                                .p(px(12.0))
                                .rounded(px(12.0))
                                .bg(theme.bg_pane)
                                .border_1()
                                .border_color(theme.border_default)
                                .flex()
                                .flex_col()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .id("welcome-ssh-enable")
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .cursor_pointer()
                                        .text_size(px(12.5))
                                        .child(cursor_icon(
                                            if self.ssh_enabled { "check-square" } else { "square" },
                                            15.0,
                                            if self.ssh_enabled {
                                                theme.accent_text
                                            } else {
                                                theme.text_tertiary
                                            },
                                        ))
                                        .child("Enable SSH workspace tools")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.ssh_enabled = !this.ssh_enabled;
                                            cx.notify();
                                        })),
                                )
                                .when(self.ssh_enabled, |d| {
                                    d.child(Input::new(&self.ssh_host).h(px(34.0)))
                                        .child(
                                            div()
                                                .flex()
                                                .gap(px(8.0))
                                                .child(div().flex_1().child(Input::new(&self.ssh_user).h(px(34.0))))
                                                .child(div().w(px(90.0)).child(Input::new(&self.ssh_port).h(px(34.0)))),
                                        )
                                        .child(Input::new(&self.ssh_identity).h(px(34.0)))
                                        .child(Input::new(&self.ssh_config).h(px(34.0)))
                                }),
                        )
                    }),
            )
            // Submit CTA
            .child(
                gb_btn("welcome-remote-submit", theme, BtnVariant::Primary, false, is_busy)
                    .w_full()
                    .h(px(42.0))
                    .child(if self.auth == RemoteAuthKind::Browser {
                        "Sign in with browser"
                    } else {
                        "Connect to Hermes"
                    })
                    .child(cursor_icon("arrow-right", 15.0, theme.text_on_primary))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.busy.is_empty() {
                            this.connect_remote(cx);
                        }
                    })),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .text_center()
                    .child("Connection details are saved in a private file on this computer."),
            )
            // Help collapsible
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("welcome-help-toggle")
                            .w_full()
                            .text_center()
                            .text_size(px(12.0))
                            .text_color(theme.text_secondary)
                            .cursor_pointer()
                            .child("Need help connecting?")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.help_open = !this.help_open;
                                cx.notify();
                            })),
                    )
                    .when(self.help_open, |d| {
                        d.child(
                            div()
                                .p(px(12.0))
                                .rounded(px(10.0))
                                .bg(theme.bg_card)
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(theme.text_secondary)
                                .flex()
                                .flex_col()
                                .gap(px(8.0))
                                .child("Start Hermes on your server with `hermes serve`. For a private server, forward loopback via SSH:")
                                .child(
                                    div()
                                        .p(px(10.0))
                                        .rounded(px(8.0))
                                        .bg(theme.bg_pane)
                                        .font_family(FONT_MONO)
                                        .text_size(px(11.0))
                                        .child("ssh -N -L 9119:127.0.0.1:9119 user@your-server"),
                                )
                                .child("Then enter http://127.0.0.1:9119 above, choose Session token, and paste HERMES_DASHBOARD_SESSION_TOKEN."),
                        )
                    }),
            )
    }
}
