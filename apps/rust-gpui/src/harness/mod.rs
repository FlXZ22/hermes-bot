pub mod events;
pub mod goal_worker;
pub mod metrics;
pub mod projects;
pub mod ptc;
pub mod store;

#[cfg(test)]
mod live_hermes_tests {
    use super::projects::{browse_server_directory, validate_server_directory};
    use crate::backend::hermes_backend::HermesSupervisor;
    use crate::hermes::client::HermesClient;
    use crate::hermes::types::{HermesAuthType, HermesConnection, HermesServerMode};
    use crate::hermes::ws::HermesRealtimeSession;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    #[ignore = "starts local Hermes Agent v0.21.0 runtime on loopback for live verification"]
    fn live_hermes_runtime_smoke_verification() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async {
            let sup = HermesSupervisor::new();
            sup.start_in_background().await;
            let info = sup
                .wait_until_ready(Duration::from_secs(28))
                .await
                .expect("Hermes local backend should become ready");
            let url = info.url.clone().expect("ready backend URL");
            let token = info.token.clone().expect("ready backend session token");
            eprintln!(
                "[live-hermes] ready url={url} boot_ms={:?} pid={:?}",
                info.boot_ms, info.pid
            );

            let conn = HermesConnection {
                id: "local-rust-live-check".into(),
                name: "Live Verification Host".into(),
                url,
                mode: HermesServerMode::Dashboard,
                auth_type: HermesAuthType::SessionToken,
                token: Some(token),
                ..Default::default()
            };
            let client = HermesClient::new(conn.clone(), Some("default".into()));
            let (status, profiles) = client
                .test_connection()
                .await
                .expect("HTTP /api/status + /api/profiles");
            eprintln!(
                "[live-hermes] status ok={:?} model={:?} profiles={}",
                status.ok,
                status.model,
                profiles.len()
            );

            // Live /api/fs/default-cwd + /api/fs/list browse
            let default_dir =
                browse_server_directory(Some(client.clone()), conn.clone(), None)
                    .await
                    .expect("browse default server directory");
            eprintln!(
                "[live-hermes] default_cwd={} entries={} is_git={}",
                default_dir.canonical_path,
                default_dir.entries.len(),
                default_dir.is_git_repo
            );

            // Live path validation on this repository
            let repo_dir = validate_server_directory(
                Some(client.clone()),
                conn.clone(),
                "/home/metis/Projects/grok-bot-ui",
            )
            .await
            .expect("validate repository directory via live Hermes /api/fs");
            assert_eq!(repo_dir.canonical_path, "/home/metis/Projects/grok-bot-ui");
            assert!(repo_dir.is_git_repo);

            // Live WebSocket RPCs: gateway.ping, commands.catalog, projects.list
            let ws = HermesRealtimeSession::new(client.clone());
            ws.connect(Some("default".into()))
                .await
                .expect("WS /api/ws connect");
            let ping = ws
                .rpc("gateway.ping", json!({}), 10_000)
                .await
                .expect("WS gateway.ping");
            let catalog = ws
                .commands_catalog()
                .await
                .expect("WS commands.catalog");
            let parsed_cmds = super::events::parse_hermes_commands_catalog(&catalog);
            let live_cmd_count = parsed_cmds
                .iter()
                .filter(|c| c.source == "live-catalog")
                .count();
            let projs = ws
                .projects_list()
                .await
                .expect("WS projects.list");
            let proj_count = projs
                .get("projects")
                .and_then(|v| v.as_array())
                .or_else(|| projs.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            eprintln!(
                "[live-hermes] WS ping={ping} parsed_cmds={} live_from_catalog={} projects_list={proj_count}",
                parsed_cmds.len(),
                live_cmd_count
            );
            assert!(
                live_cmd_count >= 20,
                "expected live Hermes v0.21.0 commands catalog to populate commands, got {live_cmd_count}"
            );

            ws.disconnect().await;
            sup.stop().await;
        });
    }
}
