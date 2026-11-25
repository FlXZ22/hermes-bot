pub mod hermes_backend;
pub mod remote;
pub mod sqlite_store;
pub mod ssh;
pub mod terminal;
pub mod updater;
pub mod workspace;

use std::fs;
use std::path::{Path, PathBuf};

pub fn profile_home(root: &Path, profile: Option<&str>) -> Result<PathBuf, String> {
    match profile.filter(|name| !name.is_empty() && *name != "default") {
        None => Ok(root.to_path_buf()),
        Some(name) => {
            let mut components = Path::new(name).components();
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

pub fn save_local_memory_file(
    hermes_home: &Path,
    kind: &str,
    content: &str,
    profile: Option<&str>,
) -> Result<(), String> {
    let home = profile_home(hermes_home, profile)?;
    let target = match kind {
        "soul" => home.join("SOUL.md"),
        "memory" => home.join("memories/MEMORY.md"),
        "user" => home.join("memories/USER.md"),
        _ => return Err(format!("Unsupported memory file kind: {kind}")),
    };
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&target, content).map_err(|e| format!("Failed saving {}: {e}", target.display()))
}
