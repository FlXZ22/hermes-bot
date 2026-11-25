use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitWorkspaceStatus {
    pub is_repo: bool,
    pub branch: String,
    pub changed_files: Vec<GitFileChange>,
    pub worktrees: Vec<String>,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitFileChange {
    pub status: String,
    pub path: String,
    pub index_status: char,
    pub worktree_status: char,
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitDiffScope {
    #[default]
    Unstaged,
    Staged,
    Uncommitted,
    Branch,
    LastTurn,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GitWorkspaceAction {
    Stage { paths: Vec<String> },
    Unstage { paths: Vec<String> },
    Commit { message: String },
    Push,
    SwitchBranch { branch: String },
    Worktree { path: String, branch: String },
}

#[derive(Debug, Clone, Serialize)]
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
        let command = format!("GIT_TERMINAL_PROMPT=0 git {}", args.iter().map(|arg| crate::ssh::quote(arg)).collect::<Vec<_>>().join(" "));
        crate::ssh::exec(ssh, dir, &command).await
    } else {
        tokio::time::timeout(Duration::from_secs(15), Command::new("git").args(args).env("GIT_TERMINAL_PROMPT", "0").current_dir(expand_dir(dir)).kill_on_drop(true).output()).await
            .map_err(|_| "Git command timed out".to_string())?.map_err(|e| format!("Git could not start: {e}"))
    }
}

fn parse_git_changes(bytes: &[u8]) -> Vec<GitFileChange> {
    let mut records = bytes.split(|byte| *byte == 0).filter(|entry| !entry.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 3 || record[2] != b' ' { continue; }
        let index_status = char::from(record[0]);
        let worktree_status = char::from(record[1]);
        let status = String::from_utf8_lossy(&record[..2]).trim().to_string();
        let target = &record[3..];
        let mut path = String::from_utf8_lossy(target).into_owned();
        let mut paths = std::str::from_utf8(target).ok().map(|valid| vec![valid.to_owned()]);
        if status.contains('R') || status.contains('C') {
            if let Some(original) = records.next() {
                path = format!("{} → {path}", String::from_utf8_lossy(original));
                if status.contains('R') {
                    paths = paths.zip(std::str::from_utf8(original).ok()).map(|(mut valid, previous)| {
                        valid.push(previous.to_owned());
                        valid
                    });
                }
            }
        }
        changes.push(GitFileChange { status, path, index_status, worktree_status, paths });
    }
    changes
}

fn git_stdout(output: std::process::Output) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim_end_matches('\n').to_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(if error.trim().is_empty() { format!("Git exited with {}", output.status) } else { error.trim().to_owned() })
    }
}

pub async fn git_workspace_action(
    dir_path: &str,
    ssh: Option<&crate::ssh::SshTarget>,
    action: GitWorkspaceAction,
) -> Result<String, String> {
    if !Path::new(dir_path).is_absolute() {
        return Err("An absolute Git workspace path is required.".into());
    }
    let root = git_stdout(git_output(dir_path, ssh, &["rev-parse", "--show-toplevel"]).await?)?;
    let args: Vec<&str> = match &action {
        GitWorkspaceAction::Stage { paths } | GitWorkspaceAction::Unstage { paths } => {
            if paths.is_empty() || paths.iter().any(|path| {
                path.is_empty() || path.contains('\0') ||
                    Path::new(path).components().any(|part| !matches!(part, std::path::Component::Normal(_)))
            }) {
                return Err("Select files inside the repository.".into());
            }
            let mut args = if matches!(action, GitWorkspaceAction::Stage { .. }) {
                vec!["--literal-pathspecs", "add", "--all", "--"]
            } else {
                vec!["--literal-pathspecs", "reset", "--quiet", "--"]
            };
            args.extend(paths.iter().map(String::as_str));
            args
        }
        GitWorkspaceAction::Commit { message } => {
            if message.trim().is_empty() || message.contains('\0') {
                return Err("Enter a commit message.".into());
            }
            vec!["commit", "-m", message]
        }
        GitWorkspaceAction::Push => vec!["push", "--porcelain"],
        GitWorkspaceAction::SwitchBranch { branch } => {
            check_work_branch(&root, ssh, branch).await?;
            vec!["switch", branch]
        }
        GitWorkspaceAction::Worktree { path, branch } => {
            if !Path::new(path).is_absolute() || path.contains('\0') {
                return Err("Enter an absolute worktree folder.".into());
            }
            check_work_branch(&root, ssh, branch).await?;
            vec!["worktree", "add", "-b", branch, "--", path]
        }
    };
    git_stdout(git_output(&root, ssh, &args).await?)
}

async fn check_work_branch(root: &str, ssh: Option<&crate::ssh::SshTarget>, branch: &str) -> Result<(), String> {
    if branch.is_empty() || branch.starts_with('-') || branch.contains('\0') || branch.starts_with("@{") {
        return Err("Enter a valid branch name.".into());
    }
    git_stdout(git_output(root, ssh, &["check-ref-format", "--branch", branch]).await?)?;
    Ok(())
}

pub async fn git_workspace_status(
    dir_path: &str,
    ssh: Option<&crate::ssh::SshTarget>,
    scope: GitDiffScope,
    baseline: Option<&str>,
) -> Result<GitWorkspaceStatus, String> {
    let empty = || GitWorkspaceStatus { is_repo: false, branch: String::new(), changed_files: vec![], worktrees: vec![], diff: String::new() };
    let branch = git_output(dir_path, ssh, &["branch", "--show-current"]).await?;
    if !branch.status.success() {
        if ssh.is_some() && branch.status.code() == Some(255) {
            return Err(format!("SSH could not read Git status. Verify the host in Terminal, and use an SSH key or agent. {}", String::from_utf8_lossy(&branch.stderr).trim()));
        }
        return Ok(empty());
    }
    let branch_range;
    let mut diff_args = vec!["diff", "--stat", "--patch", "--no-ext-diff", "--no-textconv"];
    match scope {
        GitDiffScope::Unstaged => {}
        GitDiffScope::Staged => diff_args.push("--cached"),
        GitDiffScope::Uncommitted => {
            // Unborn repositories have no HEAD. Compare the index to the
            // empty tree there, so a project's staged first commit is visible.
            let head = git_output(dir_path, ssh, &["rev-parse", "--verify", "HEAD"]).await?;
            diff_args.push(if head.status.success() { "HEAD" } else { "--cached" });
        }
        GitDiffScope::Branch => {
            let mut base = None;
            for reference in ["origin/HEAD", "origin/main", "origin/master", "main", "master", "@{upstream}"] {
                let output = git_output(dir_path, ssh, &["merge-base", "HEAD", reference]).await?;
                if output.status.success() {
                    base = Some(git_stdout(output)?);
                    break;
                }
            }
            branch_range = format!("{}..HEAD", base.ok_or("Git could not find a branch base.")?);
            diff_args.push(&branch_range);
        }
        GitDiffScope::LastTurn => {
            let hash = baseline.filter(|hash| {
                matches!(hash.len(), 40 | 64) && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            }).ok_or("No Git baseline was captured for the last Work request.")?;
            diff_args.push(hash);
        }
    }
    diff_args.push("--");
    let (status, worktrees, diff) = tokio::try_join!(
        git_output(dir_path, ssh, &["status", "--porcelain=v1", "-z"]),
        git_output(dir_path, ssh, &["worktree", "list"]),
        git_output(dir_path, ssh, &diff_args),
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

pub async fn git_work_turn_baseline(dir_path: &str, ssh: Option<&crate::ssh::SshTarget>) -> Result<String, String> {
    let head = git_stdout(git_output(dir_path, ssh, &["rev-parse", "--verify", "HEAD"]).await?)?;
    // Unlike 'stash push', 'stash create' leaves both working tree and index
    // in place. Its commit includes tracked staged and unstaged changes.
    let snapshot = git_stdout(git_output(dir_path, ssh, &[
        "-c", "user.name=Hermes Work", "-c", "user.email=hermes-work@localhost",
        "stash", "create", "Hermes Work request baseline",
    ]).await?)?;
    Ok(if snapshot.is_empty() { head } else { snapshot })
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
        assert_eq!(changes[1].paths.as_ref().unwrap(), &["renamed ü.txt", "old\nname.txt"]);
        assert_eq!(changes[2].path, "new file");
    }

    #[test]
    fn work_git_actions_use_repository_root_and_keep_unstaged_changes() {
        let directory = std::env::temp_dir().join(format!(
            "hermes-work-git-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        struct TestDirectory(PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
        }
        let scratch = TestDirectory(directory);
        let repo = scratch.0.join("repo");
        let subdir = repo.join("nested");
        fs::create_dir_all(&subdir).unwrap();
        let repo_path = repo.to_str().unwrap();
        let subdir_path = subdir.to_str().unwrap();
        let run = |args: &[&str]| {
            let result = std::process::Command::new("git").current_dir(&repo).args(args).output().unwrap();
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        };
        run(&["init", "--quiet"]);
        run(&["config", "user.email", "work-test@example.invalid"]);
        run(&["config", "user.name", "Work test"]);
        run(&["config", "commit.gpgsign", "false"]);
        let special = "literal [file].txt";
        fs::write(repo.join(special), "first\n").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let stage = || GitWorkspaceAction::Stage { paths: vec![special.into()] };
            git_workspace_action(subdir_path, None, stage()).await.unwrap();
            let staged = git_workspace_status(repo_path, None, GitDiffScope::Staged, None).await.unwrap();
            assert!(staged.diff.contains("+first"));
            git_workspace_action(subdir_path, None, GitWorkspaceAction::Unstage { paths: vec![special.into()] }).await.unwrap();
            assert!(git_workspace_status(repo_path, None, GitDiffScope::Staged, None).await.unwrap().diff.is_empty());
            git_workspace_action(subdir_path, None, stage()).await.unwrap();
            git_workspace_action(repo_path, None, GitWorkspaceAction::Commit { message: "First commit".into() }).await.unwrap();
            fs::write(repo.join(special), "second\n").unwrap();
            git_workspace_action(subdir_path, None, stage()).await.unwrap();
            fs::write(repo.join(special), "third\n").unwrap();
            assert!(git_workspace_status(subdir_path, None, GitDiffScope::Staged, None).await.unwrap().diff.contains("+second"));
            assert!(git_workspace_status(subdir_path, None, GitDiffScope::Uncommitted, None).await.unwrap().diff.contains("+third"));
            let baseline = git_work_turn_baseline(subdir_path, None).await.unwrap();
            fs::write(repo.join(special), "fourth\n").unwrap();
            let last_turn = git_workspace_status(subdir_path, None, GitDiffScope::LastTurn, Some(&baseline)).await.unwrap();
            assert!(last_turn.diff.contains("-third"));
            assert!(last_turn.diff.contains("+fourth"));
            assert!(git_workspace_status(subdir_path, None, GitDiffScope::Staged, None).await.unwrap().diff.contains("+second"));
            let worktree = scratch.0.join("other checkout");
            git_workspace_action(repo_path, None, GitWorkspaceAction::Worktree {
                path: worktree.to_str().unwrap().into(),
                branch: "work-fixture".into(),
            }).await.unwrap();
            assert_eq!(fs::read_to_string(worktree.join(special)).unwrap(), "first\n");
            assert_eq!(fs::read_to_string(repo.join(special)).unwrap(), "fourth\n");
        });
    }
}
