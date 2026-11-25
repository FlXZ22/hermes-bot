mod hermes_backend;
mod sqlite_store;
mod workspace;
mod remote;
mod ssh;
mod terminal;
#[cfg(feature = "tauri-host")]
mod legacy_storage;
pub mod updater;
#[cfg(feature = "tauri-host")]
mod tauri_host;
#[cfg(feature = "tauri-host")]
pub use tauri_host::run;
pub mod native_ipc;

use hermes_backend::{HermesSupervisor, NativeHttpRequest};
use std::fs;

fn profile_home(
    root: &std::path::Path,
    profile: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    match profile.filter(|name| !name.is_empty() && *name != "default") {
        None => Ok(root.to_path_buf()),
        Some(name) => {
            let mut components = std::path::Path::new(name).components();
            if !matches!(components.next(), Some(std::path::Component::Normal(_)))
                || components.next().is_some()
            {
                return Err("Invalid Hermes profile name".into());
            }
            let home = root.join("profiles").join(name);
            if !home.is_dir() {
                return Err(format!("Hermes profile does not exist: {name}"));
            }
            Ok(home)
        }
    }
}

fn save_local_memory_file(
    sup: &HermesSupervisor,
    kind: String,
    content: String,
    profile: Option<String>,
) -> Result<bool, String> {
    let home = profile_home(sup.hermes_home(), profile.as_deref())?;
    let target = match kind.as_str() {
        "soul" => home.join("SOUL.md"),
        "memory" => home.join("memories/MEMORY.md"),
        "user" => home.join("memories/USER.md"),
        _ => return Err(format!("Unsupported memory file kind: {kind}")),
    };
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&target, content).map_err(|e| format!("Failed saving {}: {e}", target.display()))?;
    Ok(true)
}

/// Verify the installed Hermes runtime through the same Rust path used by the
/// desktop app, without requiring a display or submitting an inference request.
pub fn diagnose_backend() -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let supervisor = HermesSupervisor::new();
        let local = sqlite_store::load_local_bootstrap(supervisor.hermes_home());
        supervisor.start_in_background().await;
        let result = async {
            for _ in 0..150 {
                let info = supervisor.info().await;
                if info.status == "error" { return Err(info.error.unwrap_or_else(|| "Backend failed".into())); }
                if info.status == "ready" {
                    let url = info.url.ok_or("Missing backend URL")?;
                    let mut headers = std::collections::HashMap::new();
                    headers.insert("X-Hermes-Session-Token".into(), info.token.ok_or("Missing session token")?);
                    let response = supervisor.proxy_http(NativeHttpRequest {
                        url: format!("{url}/api/model/info"), method: "GET".into(), headers: Some(headers), body: None,
                    }).await?;
                    if response.status != 200 { return Err(format!("Authenticated model API returned {}", response.status)); }
                    println!("{}", serde_json::json!({
                        "backend": "ready", "authenticated_http": true,
                        "boot_ms": info.boot_ms, "local_load_ms": local.elapsed_ms,
                        "sessions": local.sessions.len(), "skills": local.skills.len(), "models": local.models.len(),
                    }));
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            Err("Backend startup timed out".into())
        }.await;
        supervisor.stop().await;
        result
    })
}

#[cfg(test)]
mod tests {
    use super::profile_home;
    #[test]
    fn memory_and_history_resolve_inside_the_selected_profile() {
        let root =
            std::env::temp_dir().join(format!("hermes-ui-profiles-{}", rand::random::<u64>()));
        let named = root.join("profiles/research");
        std::fs::create_dir_all(&named).unwrap();
        assert_eq!(profile_home(&root, Some("default")).unwrap(), root);
        assert_eq!(profile_home(&root, Some("research")).unwrap(), named);
        for invalid in ["..", "../default", "/tmp", "missing"] {
            assert!(profile_home(&root, Some(invalid)).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
