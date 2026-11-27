use crate::backend::ssh::SshTarget;
use crate::hermes::client::HermesClient;
use crate::hermes::types::HermesConnection;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// A single directory entry returned by the agent server filesystem browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
}

/// Complete response from browsing a directory on the server executing the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerBrowseResult {
    pub server_id: String,
    pub server_name: String,
    pub requested_path: String,
    pub canonical_path: String,
    pub parent_path: Option<String>,
    pub home_path: Option<String>,
    pub is_home_dir: bool,
    pub is_git_repo: bool,
    pub entries: Vec<ServerDirEntry>,
}

/// Server-validated directory result before saving a Work project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedServerPath {
    pub canonical_path: String,
    pub is_home_dir: bool,
    pub is_git_repo: bool,
    pub scope_notice: Option<String>,
}

/// Normalize a path string lexically without touching the filesystem (used for remote fallback).
pub fn normalize_lexical_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }
    let path = Path::new(trimmed);
    let mut stack: Vec<String> = Vec::new();
    let is_abs = path.is_absolute();
    for comp in path.components() {
        match comp {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                if !stack.is_empty() {
                    stack.pop();
                }
            }
            Component::Normal(os) => stack.push(os.to_string_lossy().to_string()),
            Component::Prefix(p) => stack.push(p.as_os_str().to_string_lossy().to_string()),
        }
    }
    if is_abs {
        if stack.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", stack.join("/"))
        }
    } else if stack.is_empty() {
        ".".to_string()
    } else {
        stack.join("/")
    }
}

pub fn parent_of(canonical: &str) -> Option<String> {
    let p = Path::new(canonical);
    let parent = p.parent()?;
    let s = parent.to_string_lossy().to_string();
    if s.is_empty() || s == canonical {
        None
    } else {
        Some(s)
    }
}

pub fn home_scope_notice() -> String {
    "Home directory selected: agent tools run within your user account's normal permissions (does not grant privileged/root or unrestricted system access).".to_string()
}

/// Browse a directory on the agent server.
/// - When connected to a Hermes server (`client` present), prefers Hermes's `/api/fs/list`
///   and `/api/fs/default-cwd` endpoints so remote and local servers use identical server-side policy.
/// - When running locally (or if Hermes `/api/fs/*` is not reachable yet), performs canonicalized
///   local filesystem inspection with explicit symlink & permission error reporting.
/// - When an SSH target is configured without `/api/fs/list`, browses via SSH.
pub async fn browse_server_directory(
    client: Option<HermesClient>,
    connection: HermesConnection,
    raw_path: Option<String>,
) -> Result<ServerBrowseResult, String> {
    let server_id = connection.id.clone();
    let server_name = if connection.name.trim().is_empty() {
        connection.url.clone()
    } else {
        connection.name.clone()
    };

    // 1. Prefer Hermes server `/api/fs/*` when connected to a non-local server (or whenever available)
    if let Some(c) = client.clone() {
        if !connection.is_local() {
            return browse_via_hermes_api(&c, &server_id, &server_name, raw_path.as_deref()).await;
        }
        // For local Hermes server, try Hermes `/api/fs/list` if path is provided, else fall back cleanly to local canonicalized FS
        if let Ok(res) = browse_via_hermes_api(&c, &server_id, &server_name, raw_path.as_deref()).await {
            return Ok(res);
        }
    }

    // 2. SSH fallback for remote connections without Hermes HTTP `/api/fs/list`
    if let Some(ssh) = connection.ssh.as_ref() {
        return browse_via_ssh(ssh, &server_id, &server_name, raw_path.as_deref()).await;
    }

    // 3. Local canonicalized filesystem browse (run off UI thread by caller)
    let raw_clone = raw_path.clone();
    tokio::task::spawn_blocking(move || {
        browse_local_fs(&server_id, &server_name, raw_clone.as_deref())
    })
    .await
    .map_err(|e| format!("Filesystem browse worker failed: {e}"))?
}

/// Validate a target directory on the agent server before persisting a Work project.
/// Never runs `git init` — non-Git folders are valid and explicitly flagged `is_git_repo: false`.
pub async fn validate_server_directory(
    client: Option<HermesClient>,
    connection: HermesConnection,
    raw_path: &str,
) -> Result<ValidatedServerPath, String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("Enter or select a directory path.".into());
    }
    let res = browse_server_directory(client, connection, Some(trimmed.to_string())).await?;
    let scope_notice = res.is_home_dir.then(home_scope_notice);
    Ok(ValidatedServerPath {
        canonical_path: res.canonical_path,
        is_home_dir: res.is_home_dir,
        is_git_repo: res.is_git_repo,
        scope_notice,
    })
}

async fn browse_via_hermes_api(
    client: &HermesClient,
    server_id: &str,
    server_name: &str,
    raw_path: Option<&str>,
) -> Result<ServerBrowseResult, String> {
    let default_info = client
        .request_json(reqwest::Method::GET, "/api/fs/default-cwd", &[], None)
        .await
        .ok();
    let default_cwd = default_info
        .as_ref()
        .and_then(|v| v.get("cwd").and_then(|c| c.as_str()))
        .map(String::from);

    let home_path = default_info
        .as_ref()
        .and_then(|v| {
            v.get("home")
                .or_else(|| v.get("home_dir"))
                .and_then(|c| c.as_str())
        })
        .map(String::from)
        .or_else(|| {
            if server_id == "local-rust" {
                dirs::home_dir().map(|p| p.to_string_lossy().to_string())
            } else {
                None
            }
        });

    let requested = raw_path
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s == "~" {
                home_path.clone().or_else(|| default_cwd.clone()).unwrap_or_else(|| "~".into())
            } else if let Some(rest) = s.strip_prefix("~/") {
                if let Some(h) = home_path.as_deref() {
                    format!("{}/{}", h.trim_end_matches('/'), rest)
                } else {
                    s.to_string()
                }
            } else {
                s.to_string()
            }
        })
        .or_else(|| default_cwd.clone())
        .or_else(|| home_path.clone())
        .unwrap_or_else(|| "/".to_string());

    let listed = client
        .request_json(
            reqwest::Method::GET,
            "/api/fs/list",
            &[("path", requested.clone())],
            None,
        )
        .await?;

    let canonical_path = listed
        .get("path")
        .or_else(|| listed.get("cwd"))
        .and_then(|v| v.as_str())
        .map(normalize_lexical_path)
        .unwrap_or_else(|| normalize_lexical_path(&requested));

    let mut entries: Vec<ServerDirEntry> = listed
        .get("entries")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.to_string();
                    let path = item
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| {
                            format!("{}/{}", canonical_path.trim_end_matches('/'), name)
                        });
                    let is_dir = item
                        .get("isDirectory")
                        .or_else(|| item.get("is_dir"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let is_symlink = item
                        .get("isSymlink")
                        .or_else(|| item.get("is_symlink"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let symlink_target = item
                        .get("symlinkTarget")
                        .or_else(|| item.get("target"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    Some(ServerDirEntry {
                        name,
                        path,
                        is_dir,
                        is_symlink,
                        symlink_target,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let git_root = client
        .request_json(
            reqwest::Method::GET,
            "/api/fs/git-root",
            &[("path", canonical_path.clone())],
            None,
        )
        .await
        .ok()
        .and_then(|v| v.get("root").and_then(|r| r.as_str()).map(String::from))
        .filter(|s| !s.is_empty());

    let is_home_dir = home_path
        .as_deref()
        .is_some_and(|h| normalize_lexical_path(h) == canonical_path);

    Ok(ServerBrowseResult {
        server_id: server_id.to_string(),
        server_name: server_name.to_string(),
        requested_path: requested,
        canonical_path: canonical_path.clone(),
        parent_path: parent_of(&canonical_path),
        home_path,
        is_home_dir,
        is_git_repo: git_root.is_some(),
        entries,
    })
}

async fn browse_via_ssh(
    ssh: &SshTarget,
    server_id: &str,
    server_name: &str,
    raw_path: Option<&str>,
) -> Result<ServerBrowseResult, String> {
    let target = raw_path
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("~");
    let script = format!(
        "python3 -c 'import os, json, sys; p = os.path.expanduser(sys.argv[1]); real = os.path.realpath(p); home = os.path.realpath(os.path.expanduser(\"~\")); entries = [];\n\
for e in os.scandir(real):\n\
    try:\n\
        is_link = e.is_symlink()\n\
        is_dir = e.is_dir(follow_symlinks=True)\n\
        tgt = os.readlink(e.path) if is_link else None\n\
        entries.append({{\"name\": e.name, \"path\": e.path, \"is_dir\": is_dir, \"is_symlink\": is_link, \"symlink_target\": tgt}})\n\
    except OSError:\n\
        pass\n\
print(json.dumps({{\"canonical\": real, \"home\": home, \"is_git\": os.path.exists(os.path.join(real, \".git\")), \"entries\": entries}}))' {}",
        crate::backend::ssh::quote(target)
    );
    let output = crate::backend::ssh::exec(ssh, "/", &script).await?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("SSH directory browse failed for {target}")
        } else {
            err
        });
    }
    let val: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid SSH directory payload: {e}"))?;
    let canonical_path = val
        .get("canonical")
        .and_then(|v| v.as_str())
        .unwrap_or(target)
        .to_string();
    let home_path = val.get("home").and_then(|v| v.as_str()).map(String::from);
    let is_git_repo = val.get("is_git").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut entries: Vec<ServerDirEntry> = val
        .get("entries")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(ServerDirEntry {
                        name: item.get("name")?.as_str()?.to_string(),
                        path: item.get("path")?.as_str()?.to_string(),
                        is_dir: item.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false),
                        is_symlink: item
                            .get("is_symlink")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        symlink_target: item
                            .get("symlink_target")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let is_home_dir = home_path.as_deref() == Some(canonical_path.as_str());
    Ok(ServerBrowseResult {
        server_id: server_id.to_string(),
        server_name: server_name.to_string(),
        requested_path: target.to_string(),
        canonical_path: canonical_path.clone(),
        parent_path: parent_of(&canonical_path),
        home_path,
        is_home_dir,
        is_git_repo,
        entries,
    })
}

pub fn browse_local_fs(
    server_id: &str,
    server_name: &str,
    raw_path: Option<&str>,
) -> Result<ServerBrowseResult, String> {
    let home = dirs::home_dir();
    let home_canonical = home
        .as_ref()
        .and_then(|h| std::fs::canonicalize(h).ok())
        .or(home)
        .map(|p| p.to_string_lossy().to_string());

    let expanded: PathBuf = match raw_path.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("~") => home_canonical
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/")),
        Some(p) => {
            if let Some(rest) = p.strip_prefix("~/") {
                home_canonical
                    .as_deref()
                    .map(|h| PathBuf::from(h).join(rest))
                    .unwrap_or_else(|| PathBuf::from(p))
            } else {
                PathBuf::from(p)
            }
        }
    };

    let canonical = std::fs::canonicalize(&expanded).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("Directory does not exist: {}", expanded.display()),
        std::io::ErrorKind::PermissionDenied => {
            format!("Permission denied accessing: {}", expanded.display())
        }
        _ => format!("Cannot resolve path {}: {e}", expanded.display()),
    })?;

    if !canonical.is_dir() {
        return Err(format!("Path is not a directory: {}", canonical.display()));
    }

    let read_dir = std::fs::read_dir(&canonical).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            format!("Permission denied reading directory: {}", canonical.display())
        }
        _ => format!("Failed reading directory {}: {e}", canonical.display()),
    })?;

    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        let entry_path = item.path();
        let sym_meta = std::fs::symlink_metadata(&entry_path).ok();
        let is_symlink = sym_meta
            .as_ref()
            .is_some_and(|m| m.file_type().is_symlink());
        let symlink_target = if is_symlink {
            std::fs::read_link(&entry_path)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };
        let is_dir = std::fs::metadata(&entry_path)
            .map(|m| m.is_dir())
            .unwrap_or(false);
        entries.push(ServerDirEntry {
            name,
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            is_symlink,
            symlink_target,
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let canonical_str = canonical.to_string_lossy().to_string();
    let is_git_repo = has_enclosing_git_dir(&canonical);
    let is_home_dir = home_canonical.as_deref() == Some(canonical_str.as_str());

    Ok(ServerBrowseResult {
        server_id: server_id.to_string(),
        server_name: server_name.to_string(),
        requested_path: raw_path.unwrap_or(&canonical_str).to_string(),
        canonical_path: canonical_str.clone(),
        parent_path: parent_of(&canonical_str),
        home_path: home_canonical,
        is_home_dir,
        is_git_repo,
        entries,
    })
}

fn has_enclosing_git_dir(dir: &Path) -> bool {
    let mut cur = Some(dir);
    while let Some(p) = cur {
        if p.join(".git").exists() {
            return true;
        }
        cur = p.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_browse_canonicalizes_symlinks_handles_empty_dirs_and_never_runs_git_init() {
        let base = std::env::temp_dir().join(format!("hermes-fs-test-{}", uuid::Uuid::new_v4()));
        let empty = base.join("empty_project");
        std::fs::create_dir_all(&empty).expect("mkdir");

        let res = browse_local_fs("local-rust", "This computer", Some(empty.to_str().unwrap()))
            .expect("browse empty dir");
        assert!(!res.is_git_repo);
        assert!(res.entries.is_empty());
        assert!(!empty.join(".git").exists());

        // Non-existent path returns explicit error
        let missing = base.join("does-not-exist");
        let err = browse_local_fs("local-rust", "This computer", Some(missing.to_str().unwrap()))
            .unwrap_err();
        assert!(err.contains("does not exist"), "unexpected err: {err}");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn lexical_normalization_handles_parent_segments() {
        assert_eq!(normalize_lexical_path("/a/b/../c/./"), "/a/c");
        assert_eq!(normalize_lexical_path("/"), "/");
        assert_eq!(parent_of("/a/c").as_deref(), Some("/a"));
        assert_eq!(parent_of("/"), None);
    }
}
