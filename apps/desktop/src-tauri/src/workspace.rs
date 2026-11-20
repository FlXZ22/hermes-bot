use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tokio::process::Command;

#[derive(Debug, Serialize)]
pub struct WorkspaceFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

#[derive(Debug, Serialize)]
pub struct GitWorkspaceStatus {
    pub is_repo: bool,
    pub branch: String,
    pub changed_files: Vec<GitFileChange>,
    pub worktrees: Vec<String>,
    pub diff: String,
}

#[derive(Debug, Serialize)]
pub struct GitFileChange {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub cwd: String,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct LinuxSystemStats {
    pub hostname: String,
    pub os_name: String,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub memory_percent: f64,
    pub load_avg: String,
    pub home_dir: String,
}

pub fn expand_dir(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

pub fn list_workspace_files(dir_path: &str) -> Result<Vec<WorkspaceFileEntry>, String> {
    let target = expand_dir(dir_path);
    let entries = fs::read_dir(&target)
        .map_err(|e| format!("Failed reading directory {}: {e}", target.display()))?;

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        out.push(WorkspaceFileEntry {
            name,
            path: path.to_string_lossy().to_string(),
            is_dir,
            size,
            modified,
        });
    }

    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

pub fn read_workspace_file(file_path: &str) -> Result<String, String> {
    let target = expand_dir(file_path);
    let meta =
        fs::metadata(&target).map_err(|e| format!("Cannot read {}: {e}", target.display()))?;
    if meta.len() > 512 * 1024 {
        return Err(format!(
            "File is too large for inline preview ({:.1} KB). Open it in your editor instead.",
            meta.len() as f64 / 1024.0
        ));
    }
    let bytes = fs::read(&target).map_err(|e| format!("Read failed: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

async fn git_output(dir: &str, ssh: Option<&crate::ssh::SshTarget>, args: &[&str]) -> Result<std::process::Output, String> {
    if let Some(ssh) = ssh {
        let command = format!("git {}", args.iter().map(|arg| crate::ssh::quote(arg)).collect::<Vec<_>>().join(" "));
        crate::ssh::exec(ssh, dir, &command).await
    } else {
        tokio::time::timeout(Duration::from_secs(15), Command::new("git").args(args).current_dir(expand_dir(dir)).kill_on_drop(true).output()).await
            .map_err(|_| "Git command timed out".to_string())?.map_err(|e| format!("Git could not start: {e}"))
    }
}

fn parse_git_changes(bytes: &[u8]) -> Vec<GitFileChange> {
    let text = String::from_utf8_lossy(bytes);
    let mut records = text.split('\0').filter(|entry| !entry.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 3 || !record.is_char_boundary(3) { continue; }
        let status = record[..2].trim().to_string();
        let mut path = record[3..].to_string();
        if status.contains('R') || status.contains('C') {
            if let Some(original) = records.next() { path = format!("{original} → {path}"); }
        }
        changes.push(GitFileChange { status, path });
    }
    changes
}

pub async fn git_workspace_status(dir_path: &str, ssh: Option<&crate::ssh::SshTarget>) -> Result<GitWorkspaceStatus, String> {
    let empty = || GitWorkspaceStatus { is_repo: false, branch: String::new(), changed_files: vec![], worktrees: vec![], diff: String::new() };
    let branch = git_output(dir_path, ssh, &["branch", "--show-current"]).await?;
    if !branch.status.success() {
        if ssh.is_some() && branch.status.code() == Some(255) {
            return Err(format!("SSH could not read Git status. Verify the host in Terminal, and use an SSH key or agent. {}", String::from_utf8_lossy(&branch.stderr).trim()));
        }
        return Ok(empty());
    }
    let (status, worktrees, diff) = tokio::try_join!(
        git_output(dir_path, ssh, &["status", "--porcelain=v1", "-z"]),
        git_output(dir_path, ssh, &["worktree", "list"]),
        git_output(dir_path, ssh, &["diff", "--stat", "--patch"]),
    )?;
    for output in [&status, &worktrees, &diff] {
        if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    }
    let diff = String::from_utf8_lossy(&diff.stdout);
    let diff = if diff.len() > 64_000 { format!("{}\n\n… Diff truncated at 64 KB", &diff[..diff.floor_char_boundary(64_000)]) } else { diff.into_owned() };
    Ok(GitWorkspaceStatus {
        is_repo: true, branch: { let name = String::from_utf8_lossy(&branch.stdout).trim().to_string(); if name.is_empty() { "Detached HEAD".into() } else { name } },
        changed_files: parse_git_changes(&status.stdout),
        worktrees: String::from_utf8_lossy(&worktrees.stdout).lines().map(str::to_string).collect(), diff,
    })
}

pub async fn run_shell_command(cwd: &str, command: &str) -> Result<ShellExecResult, String> {
    let dir = expand_dir(cwd);
    let working_dir = if dir.is_dir() {
        dir
    } else {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    };

    let started = Instant::now();
    let child = Command::new("bash")
        .args(["-lc", command])
        .kill_on_drop(true)
        .current_dir(&working_dir)
        .output();

    let output = tokio::time::timeout(Duration::from_secs(30), child)
        .await
        .map_err(|_| "Command timed out after 30s".to_string())?
        .map_err(|e| format!("Failed executing command: {e}"))?;

    Ok(ShellExecResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        cwd: working_dir.to_string_lossy().to_string(),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn get_linux_system_stats() -> LinuxSystemStats {
    let hostname = fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "linux".into())
        .trim()
        .to_string();

    let os_name = fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|txt| {
            txt.lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| {
                    l.trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string()
                })
        })
        .unwrap_or_else(|| "Arch Linux".into());

    let mut mem_total_kb = 0u64;
    let mut mem_avail_kb = 0u64;
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                mem_total_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                mem_avail_kb = rest
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
        }
    }

    let total_mb = mem_total_kb / 1024;
    let used_mb = mem_total_kb.saturating_sub(mem_avail_kb) / 1024;
    let mem_pct = if total_mb > 0 {
        (used_mb as f64 / total_mb as f64) * 100.0
    } else {
        0.0
    };

    let load_avg = fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");

    let home_dir = dirs::home_dir()
        .unwrap_or_else(|| Path::new("/").to_path_buf())
        .to_string_lossy()
        .to_string();

    LinuxSystemStats {
        hostname,
        os_name,
        memory_used_mb: used_mb,
        memory_total_mb: total_mb,
        memory_percent: (mem_pct * 10.0).round() / 10.0,
        load_avg,
        home_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn git_changes_keep_spaces_newlines_unicode_and_rename_pairs() {
        let changes = parse_git_changes(" M ordinary file\0R  renamed ü.txt\0old\nname.txt\0?? new file\0".as_bytes());
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].path, "ordinary file");
        assert_eq!(changes[1].path, "old\nname.txt → renamed ü.txt");
        assert_eq!(changes[2].path, "new file");
    }
}
