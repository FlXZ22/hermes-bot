mod assets;
mod backend;
mod face;
mod harness;
mod hermes;
mod state;
mod theme;
mod ui;

pub use backend::{hermes_backend, remote, sqlite_store, ssh, terminal, updater, workspace};

use crate::state::app_state::AppState;
use crate::ui::shell::HermesShell;
use gpui::*;
use gpui_component::Root;
use std::sync::Arc;

fn main() {
    let tokio_rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("create tokio runtime"),
    );

    let app = gpui_platform::application().with_assets(assets::AppAssets);
    let rt_for_app = tokio_rt.clone();

    app.run(move |cx| {
        gpui_component::init(cx);
        assets::load_embedded_fonts(cx);

        let app_state = cx.new(|cx| AppState::new(rt_for_app.clone(), cx));

        // Ensure managed Hermes backend and PTYs shut down cleanly on quit
        let state_for_quit = app_state.clone();
        let rt_for_quit = rt_for_app.clone();
        cx.on_app_quit(move |cx| {
            let (supervisor, terminals) = {
                let s = state_for_quit.read(cx);
                (s.supervisor.clone(), s.terminals.clone())
            };
            let rt = rt_for_quit.clone();
            async move {
                terminals.stop_all();
                let _ = rt.spawn(async move { supervisor.stop().await }).await;
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1340.0), px(860.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(900.0), px(600.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("Hermes Grok Bot".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some("hermes-grok-bot".into()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            let shell = cx.new(|cx| HermesShell::new(app_state.clone(), window, cx));
            cx.new(|cx| Root::new(shell, window, cx))
        })
        .expect("open Hermes Grok Bot window");

        cx.activate(true);
    });
}
