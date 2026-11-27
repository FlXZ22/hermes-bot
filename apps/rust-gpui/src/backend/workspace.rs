use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use std::{collections::HashMap, fs};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GitWorkspaceStatus {
    pub is_repo: bool,
    pub branch: String,
    pub detached_head: bool,
    pub head_short: Option<String>,
    pub comparison_basis: String,
    pub total_added: u64,
    pub total_removed: u64,
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
    pub added: Option<u64>,
    pub removed: Option<u64>,
    pub is_binary: bool,
    pub file_diff: Option<String>,
}

impl Default for GitFileChange {
    fn default() -> Self {
        Self {
            status: String::new(),
            path: String::new(),
            index_status: ' ',
            worktree_status: ' ',
            paths: None,
            added: None,
            removed: None,
            is_binary: false,
            file_diff: None,
        }
    }
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

async fn git_output(
    dir: &str,
    ssh: Option<&crate::ssh::SshTarget>,
    args: &[&str],
) -> Result<std::process::Output, String> {
    if let Some(ssh) = ssh {
        let command = format!(
            "LC_ALL=C GIT_TERMINAL_PROMPT=0 git {}",
            args.iter()
                .map(|arg| crate::ssh::quote(arg))
                .collect::<Vec<_>>()
                .join(" ")
        );
        crate::ssh::exec(ssh, dir, &command).await
    } else {
        tokio::time::timeout(
            Duration::from_secs(15),
            Command::new("git")
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("LC_ALL", "C")
                .current_dir(expand_dir(dir))
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| "Git command timed out".to_string())?
        .map_err(|e| format!("Git could not start: {e}"))
    }
}

fn parse_git_changes(bytes: &[u8]) -> Vec<GitFileChange> {
    let mut records = bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 3 || record[2] != b' ' {
            continue;
        }
        let index_status = char::from(record[0]);
        let worktree_status = char::from(record[1]);
        let status = String::from_utf8_lossy(&record[..2]).trim().to_string();
        let target = &record[3..];
        let mut path = String::from_utf8_lossy(target).into_owned();
        let mut paths = std::str::from_utf8(target)
            .ok()
            .map(|valid| vec![valid.to_owned()]);
        if status.contains('R') || status.contains('C') {
            if let Some(original) = records.next() {
                path = format!("{} → {path}", String::from_utf8_lossy(original));
                if status.contains('R') {
                    paths = paths.zip(std::str::from_utf8(original).ok()).map(
                        |(mut valid, previous)| {
                            valid.push(previous.to_owned());
                            valid
                        },
                    );
                }
            }
        }
        changes.push(GitFileChange {
            status,
            path,
            index_status,
            worktree_status,
            paths,
            ..Default::default()
        });
    }
    changes
}

#[derive(Debug, PartialEq, Eq)]
struct GitNumstat<'a> {
    path: &'a [u8],
    previous_path: Option<&'a [u8]>,
    added: Option<u64>,
    removed: Option<u64>,
    is_binary: bool,
}

fn parse_git_numstat(bytes: &[u8]) -> Vec<GitNumstat<'_>> {
    let mut records = bytes.split(|&byte| byte == 0);
    let mut stats = Vec::new();
    while let Some(record) = records.next() {
        let mut fields = record.splitn(3, |&byte| byte == b'\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };

        // With -z an ordinary path follows the counts. Renames/copies have
        // an empty first path followed by separate NUL-terminated old and new paths.
        let (previous_path, path) = if path.is_empty() {
            let (Some(previous), Some(next)) = (records.next(), records.next()) else {
                break;
            };
            (Some(previous), next)
        } else {
            (None, path)
        };
        if path.is_empty() {
            continue;
        }
        let is_binary = added == b"-" && removed == b"-";
        let parse_count = |value| std::str::from_utf8(value).ok()?.parse().ok();
        stats.push(GitNumstat {
            path,
            previous_path,
            added: parse_count(added),
            removed: parse_count(removed),
            is_binary,
        });
    }
    stats
}

fn split_git_file_diffs(patch: &str) -> HashMap<&str, &str> {
    let mut files = HashMap::new();
    let mut current = None;
    let mut offset = 0;
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ")
            || line.starts_with("diff --cc ")
            || line.starts_with("diff --combined ")
        {
            if let Some((header, start)) = current {
                files.insert(header, &patch[start..offset]);
            }
            current = Some((line.trim_end_matches('\n'), offset));
        }
        offset += line.len();
    }
    if let Some((header, start)) = current {
        files.insert(header, &patch[start..]);
    }
    files
}

// Match Git's C-quoted paths. A diff header can have unquoted spaces, so
// splitting it on whitespace would misidentify files such as "a b/file".
fn push_git_patch_path(header: &mut String, prefix: &str, path: &[u8]) {
    let needs_quotes = path
        .iter()
        .any(|byte| matches!(byte, b'"' | b'\\' | 0..=31 | 127..=255));
    if !needs_quotes {
        header.push_str(prefix);
        header.push_str(std::str::from_utf8(path).expect("unquoted Git path is printable ASCII"));
        return;
    }
    header.push('"');
    header.push_str(prefix);
    for &byte in path {
        let escape = match byte {
            b'\x07' => "\\a",
            b'\x08' => "\\b",
            b'\t' => "\\t",
            b'\n' => "\\n",
            b'\x0b' => "\\v",
            b'\x0c' => "\\f",
            b'\r' => "\\r",
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0..=31 | 127..=255 => {
                use std::fmt::Write as _;
                let _ = write!(header, "\\{byte:03o}");
                continue;
            }
            _ => {
                header.push(char::from(byte));
                continue;
            }
        };
        header.push_str(escape);
    }
    header.push('"');
}

fn diff_for_numstat<'a>(
    files: &HashMap<&str, &'a str>,
    stat: &GitNumstat<'_>,
    header: &mut String,
) -> Option<&'a str> {
    header.clear();
    header.push_str("diff --git ");
    push_git_patch_path(header, "a/", stat.previous_path.unwrap_or(stat.path));
    header.push(' ');
    push_git_patch_path(header, "b/", stat.path);
    if let Some(&diff) = files.get(header.as_str()) {
        return Some(diff);
    }

    for prefix in ["diff --cc ", "diff --combined "] {
        header.clear();
        header.push_str(prefix);
        push_git_patch_path(header, "", stat.path);
        if let Some(&diff) = files.get(header.as_str()) {
            return Some(diff);
        }
    }
    None
}

const GIT_DIFF_LIMIT: usize = 64_000;
const GIT_DIFF_TRUNCATED: &str = "\n\n… Diff truncated at 64 KB";

fn cap_git_diff(diff: &str) -> String {
    if diff.len() > GIT_DIFF_LIMIT {
        format!(
            "{}{GIT_DIFF_TRUNCATED}",
            &diff[..diff.floor_char_boundary(GIT_DIFF_LIMIT)]
        )
    } else {
        diff.to_owned()
    }
}

fn merge_git_change_details(
    changes: &mut Vec<GitFileChange>,
    stats: &[GitNumstat<'_>],
    patch: &str,
) {
    let files = split_git_file_diffs(patch);
    let mut header = String::new();
    let mut index: HashMap<String, usize> = changes
        .iter()
        .enumerate()
        .map(|(index, change)| {
            let path = change
                .paths
                .as_ref()
                .and_then(|paths| paths.first())
                .unwrap_or(&change.path);
            (path.clone(), index)
        })
        .collect();

    for stat in stats {
        let path = String::from_utf8_lossy(stat.path);
        let previous = stat.previous_path.map(String::from_utf8_lossy);
        let preview = diff_for_numstat(&files, stat, &mut header);
        let entry = *index.entry(path.to_string()).or_insert_with(|| {
            let position = changes.len();
            let headers = preview
                .unwrap_or_default()
                .lines()
                .take_while(|line| !line.starts_with("@@"));
            let status = if let Some(previous) = &previous {
                if headers.clone().any(|line| line.starts_with("copy from ")) {
                    'C'
                } else if previous != &path {
                    'R'
                } else {
                    'M'
                }
            } else if headers
                .clone()
                .any(|line| line.starts_with("new file mode "))
            {
                'A'
            } else if headers
                .clone()
                .any(|line| line.starts_with("deleted file mode "))
            {
                'D'
            } else {
                'M'
            };
            changes.push(GitFileChange {
                status: status.to_string(),
                path: previous
                    .as_ref()
                    .map_or_else(|| path.to_string(), |old| format!("{old} → {path}")),
                // A comparison-only file needn't have a working-tree status.
                ..Default::default()
            });
            position
        });
        let change = &mut changes[entry];
        change.added = stat.added;
        change.removed = stat.removed;
        change.is_binary = stat.is_binary;
        change.file_diff = preview.map(cap_git_diff);
    }
}

fn git_check(output: std::process::Output) -> Result<std::process::Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(if error.trim().is_empty() {
            format!("Git exited with {}", output.status)
        } else {
            error.trim().to_owned()
        })
    }
}

fn git_stdout(output: std::process::Output) -> Result<String, String> {
    let output = git_check(output)?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_owned())
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
    let work_tree = git_output(dir_path, ssh, &["rev-parse", "--is-inside-work-tree"]).await?;
    if !work_tree.status.success() {
        if ssh.is_some() && work_tree.status.code() == Some(255) {
            return Err(format!("SSH could not read Git status. Verify the host in Terminal, and use an SSH key or agent. {}", String::from_utf8_lossy(&work_tree.stderr).trim()));
        }
        if String::from_utf8_lossy(&work_tree.stderr).contains("not a git repository") {
            return Ok(GitWorkspaceStatus::default());
        }
        return git_stdout(work_tree).map(|_| GitWorkspaceStatus::default());
    }
    if git_stdout(work_tree)?.trim() != "true" {
        return Ok(GitWorkspaceStatus::default());
    }

    let (symbolic_head, head) = tokio::try_join!(
        git_output(dir_path, ssh, &["symbolic-ref", "--short", "-q", "HEAD"]),
        git_output(dir_path, ssh, &["rev-parse", "--short", "HEAD"]),
    )?;
    let branch = if symbolic_head.status.code() == Some(1) {
        None
    } else {
        Some(git_stdout(symbolic_head)?)
    };
    let head_short = if head.status.success() {
        Some(git_stdout(head)?)
    } else if branch.is_some() && head.status.code() == Some(128) {
        // A symbolic HEAD can point at a branch before the first commit.
        None
    } else {
        return git_stdout(head).map(|_| GitWorkspaceStatus::default());
    };
    let detached_head = branch.is_none();
    let branch =
        branch.unwrap_or_else(|| format!("HEAD ({})", head_short.as_deref().unwrap_or_default()));
    let mut revisions = Vec::new();
    let branch_range;
    let comparison_basis = match scope {
        GitDiffScope::Unstaged => "unstaged vs index",
        GitDiffScope::Staged => {
            revisions.push("--cached");
            if head_short.is_some() {
                "staged vs HEAD"
            } else {
                "unborn HEAD"
            }
        }
        GitDiffScope::Uncommitted => {
            if head_short.is_some() {
                revisions.push("HEAD");
                "worktree vs HEAD"
            } else {
                // Keep showing the staged first commit against the empty tree.
                revisions.push("--cached");
                "unborn HEAD"
            }
        }
        GitDiffScope::Branch => {
            let mut base = None;
            for reference in [
                "origin/HEAD",
                "origin/main",
                "origin/master",
                "main",
                "master",
                "@{upstream}",
            ] {
                let output = git_output(dir_path, ssh, &["merge-base", "HEAD", reference]).await?;
                if output.status.success() {
                    base = Some(git_stdout(output)?);
                    break;
                }
            }
            branch_range = format!("{}..HEAD", base.ok_or("Git could not find a branch base.")?);
            revisions.push(&branch_range);
            "HEAD vs merge base"
        }
        GitDiffScope::LastTurn => {
            let hash = baseline
                .filter(|hash| {
                    matches!(hash.len(), 40 | 64)
                        && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .ok_or("No Git baseline was captured for the last Work request.")?;
            revisions.push(hash);
            "worktree vs last Work baseline"
        }
    };
    let diff_base = [
        "-c",
        "core.quotePath=true",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-relative",
        "--submodule=short",
    ];
    let diff_args = diff_base
        .into_iter()
        .chain(["--stat", "--patch", "--src-prefix=a/", "--dst-prefix=b/"])
        .chain(revisions.iter().copied())
        .chain(["--"])
        .collect::<Vec<_>>();
    let numstat_args = diff_base
        .into_iter()
        .chain(["--numstat", "-z"])
        .chain(revisions)
        .chain(["--"])
        .collect::<Vec<_>>();

    let (status, worktrees, diff, numstat) = tokio::try_join!(
        git_output(
            dir_path,
            ssh,
            &["--no-optional-locks", "status", "--porcelain=v1", "-z"]
        ),
        git_output(dir_path, ssh, &["worktree", "list"]),
        git_output(dir_path, ssh, &diff_args),
        git_output(dir_path, ssh, &numstat_args),
    )?;
    let mut changed_files = parse_git_changes(&git_check(status)?.stdout);
    let worktrees = git_check(worktrees)?;
    let diff = git_check(diff)?;
    let numstat = git_check(numstat)?;
    let diff = String::from_utf8_lossy(&diff.stdout);
    let stats = parse_git_numstat(&numstat.stdout);
    merge_git_change_details(&mut changed_files, &stats, &diff);
    let (total_added, total_removed) = stats.iter().fold((0u64, 0u64), |(added, removed), stat| {
        (
            added.saturating_add(stat.added.unwrap_or(0)),
            removed.saturating_add(stat.removed.unwrap_or(0)),
        )
    });
    Ok(GitWorkspaceStatus {
        is_repo: true,
        branch,
        detached_head,
        head_short,
        comparison_basis: comparison_basis.into(),
        total_added,
        total_removed,
        changed_files,
        worktrees: String::from_utf8_lossy(&worktrees.stdout)
            .lines()
            .map(str::to_string)
            .collect(),
        diff: cap_git_diff(&diff),
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
    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("hermes-work-git-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn git_fixture(dir: &Path, args: &[&str]) {
        let result = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn numstat_handles_tabs_newlines_renames_and_binary_files() {
        let stats = parse_git_numstat(
            b"2\t3\tnew\tline\n.txt\0-\t-\timage.bin\0\x30\t1\t\0old file\0new file\0",
        );
        assert_eq!(
            stats,
            [
                GitNumstat {
                    path: b"new\tline\n.txt",
                    previous_path: None,
                    added: Some(2),
                    removed: Some(3),
                    is_binary: false
                },
                GitNumstat {
                    path: b"image.bin",
                    previous_path: None,
                    added: None,
                    removed: None,
                    is_binary: true
                },
                GitNumstat {
                    path: b"new file",
                    previous_path: Some(b"old file"),
                    added: Some(0),
                    removed: Some(1),
                    is_binary: false
                },
            ]
        );
        assert!(parse_git_numstat(b"1\t2\t\0old name\0").is_empty());
    }

    #[test]
    fn per_file_diff_sections_exclude_the_summary_and_keep_file_whitespace() {
        let first = "diff --git a/one file b/one file\n@@ -1 +1 @@\n-old\n+new\n";
        let second = "diff --git a/other b/other\n@@ -0,0 +1 @@\n+\tdiff --git a/x b/x  \n";
        let patch = format!(" one file | 2 +-\n\n{first}{second}");
        let files = split_git_file_diffs(&patch);
        assert_eq!(files.get("diff --git a/one file b/one file"), Some(&first));
        assert_eq!(files.get("diff --git a/other b/other"), Some(&second));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn per_file_limits_keep_later_patches_and_do_not_split_utf8() {
        let large = "🦀".repeat(GIT_DIFF_LIMIT / 4);
        let patch =
            format!("diff --git a/large b/large\n+{large}\ndiff --git a/small b/small\n+small\n");
        let stats = parse_git_numstat(b"1\t0\tlarge\x001\t0\tsmall\x00");
        let mut changes = Vec::new();
        merge_git_change_details(&mut changes, &stats, &patch);
        assert!(changes[0]
            .file_diff
            .as_deref()
            .unwrap()
            .ends_with(GIT_DIFF_TRUNCATED));
        assert_eq!(
            changes[1].file_diff.as_deref(),
            Some("diff --git a/small b/small\n+small\n")
        );
    }

    #[tokio::test]
    async fn non_git_directory_does_not_initialize_a_repository() {
        let dir = TestDirectory::new();
        fs::write(dir.0.join("keep.txt"), "untouched\n").unwrap();
        let result = git_workspace_status(
            dir.0.to_str().unwrap(),
            None,
            GitDiffScope::Uncommitted,
            None,
        )
        .await
        .unwrap();
        assert!(!result.is_repo);
        assert!(!result.detached_head);
        assert!(result.changed_files.is_empty());
        assert_eq!((result.total_added, result.total_removed), (0, 0));
        assert!(!dir.0.join(".git").exists());
        assert_eq!(
            fs::read_to_string(dir.0.join("keep.txt")).unwrap(),
            "untouched\n"
        );
    }

    #[tokio::test]
    async fn detached_head_counts_the_selected_scope_and_preserves_existing_edits() {
        let dir = TestDirectory::new();
        let path = dir.0.to_str().unwrap();
        git_fixture(&dir.0, &["init", "--quiet"]);
        git_fixture(
            &dir.0,
            &["config", "user.email", "work-test@example.invalid"],
        );
        git_fixture(&dir.0, &["config", "user.name", "Work test"]);
        git_fixture(&dir.0, &["config", "commit.gpgsign", "false"]);
        let file = "file with spaces ü\tand newline\n.txt";
        fs::write(dir.0.join(file), "first\n").unwrap();
        fs::write(dir.0.join("binary.data"), b"\0first").unwrap();
        fs::write(dir.0.join("rename me.txt"), "rename content\n").unwrap();
        git_fixture(&dir.0, &["add", "--all"]);
        let unborn = git_workspace_status(path, None, GitDiffScope::Staged, None)
            .await
            .unwrap();
        assert_eq!(unborn.comparison_basis, "unborn HEAD");
        assert!(unborn.is_repo && !unborn.detached_head && unborn.head_short.is_none());
        git_fixture(&dir.0, &["commit", "--quiet", "-m", "First"]);
        git_fixture(&dir.0, &["switch", "--quiet", "--detach"]);

        fs::write(dir.0.join(file), "staged\nextra\n").unwrap();
        git_fixture(&dir.0, &["add", "--", file]);
        git_fixture(&dir.0, &["mv", "--", "rename me.txt", "renamed ü.txt"]);
        fs::write(dir.0.join(file), "staged\nextra\nunstaged\n").unwrap();
        fs::write(dir.0.join("binary.data"), b"\0second").unwrap();

        let result = git_workspace_status(path, None, GitDiffScope::Uncommitted, None)
            .await
            .unwrap();
        assert!(result.is_repo && result.detached_head);
        assert_eq!(
            result.branch,
            format!("HEAD ({})", result.head_short.as_deref().unwrap())
        );
        assert_eq!(result.comparison_basis, "worktree vs HEAD");
        assert_eq!((result.total_added, result.total_removed), (3, 1));
        let file_change = result
            .changed_files
            .iter()
            .find(|change| change.path == file)
            .unwrap();
        assert_eq!((file_change.added, file_change.removed), (Some(3), Some(1)));
        let file_diff = file_change.file_diff.as_deref().unwrap();
        assert!(file_diff.contains("+unstaged"));
        assert!(!file_diff.contains("binary.data"));

        let binary = result
            .changed_files
            .iter()
            .find(|change| change.path == "binary.data")
            .unwrap();
        assert!(binary.is_binary);
        assert_eq!((binary.added, binary.removed), (None, None));
        assert!(binary
            .file_diff
            .as_deref()
            .unwrap()
            .contains("Binary files"));
        let rename = result
            .changed_files
            .iter()
            .find(|change| change.status == "R")
            .unwrap();
        assert_eq!((rename.added, rename.removed), (Some(0), Some(0)));
        assert!(rename
            .file_diff
            .as_deref()
            .unwrap()
            .contains("rename from rename me.txt"));

        let staged = git_workspace_status(path, None, GitDiffScope::Staged, None)
            .await
            .unwrap();
        assert_eq!(staged.comparison_basis, "staged vs HEAD");
        assert_eq!((staged.total_added, staged.total_removed), (2, 1));
        assert!(!staged.diff.contains("+unstaged"));
        let unstaged = git_workspace_status(path, None, GitDiffScope::Unstaged, None)
            .await
            .unwrap();
        assert_eq!(unstaged.comparison_basis, "unstaged vs index");
        assert_eq!((unstaged.total_added, unstaged.total_removed), (1, 0));
        assert_eq!(
            fs::read_to_string(dir.0.join(file)).unwrap(),
            "staged\nextra\nunstaged\n"
        );
    }

    #[test]
    fn git_changes_keep_spaces_newlines_unicode_and_rename_pairs() {
        let changes = parse_git_changes(
            " M ordinary file\0R  renamed ü.txt\0old\nname.txt\0?? new file\0".as_bytes(),
        );
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].path, "ordinary file");
        assert_eq!(changes[1].path, "old\nname.txt → renamed ü.txt");
        assert_eq!(
            changes[1].paths.as_ref().unwrap(),
            &["renamed ü.txt", "old\nname.txt"]
        );
        assert_eq!(changes[2].path, "new file");
    }

    #[test]
    fn work_git_actions_use_repository_root_and_keep_unstaged_changes() {
        let scratch = TestDirectory::new();
        let repo = scratch.0.join("repo");
        let subdir = repo.join("nested");
        fs::create_dir_all(&subdir).unwrap();
        let repo_path = repo.to_str().unwrap();
        let subdir_path = subdir.to_str().unwrap();
        let run = |args: &[&str]| {
            let result = std::process::Command::new("git")
                .current_dir(&repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        };
        run(&["init", "--quiet"]);
        run(&["config", "user.email", "work-test@example.invalid"]);
        run(&["config", "user.name", "Work test"]);
        run(&["config", "commit.gpgsign", "false"]);
        let special = "literal [file].txt";
        fs::write(repo.join(special), "first\n").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let stage = || GitWorkspaceAction::Stage {
                paths: vec![special.into()],
            };
            git_workspace_action(subdir_path, None, stage())
                .await
                .unwrap();
            let staged = git_workspace_status(repo_path, None, GitDiffScope::Staged, None)
                .await
                .unwrap();
            assert!(staged.diff.contains("+first"));
            git_workspace_action(
                subdir_path,
                None,
                GitWorkspaceAction::Unstage {
                    paths: vec![special.into()],
                },
            )
            .await
            .unwrap();
            assert!(
                git_workspace_status(repo_path, None, GitDiffScope::Staged, None)
                    .await
                    .unwrap()
                    .diff
                    .is_empty()
            );
            git_workspace_action(subdir_path, None, stage())
                .await
                .unwrap();
            git_workspace_action(
                repo_path,
                None,
                GitWorkspaceAction::Commit {
                    message: "First commit".into(),
                },
            )
            .await
            .unwrap();
            fs::write(repo.join(special), "second\n").unwrap();
            git_workspace_action(subdir_path, None, stage())
                .await
                .unwrap();
            fs::write(repo.join(special), "third\n").unwrap();
            assert!(
                git_workspace_status(subdir_path, None, GitDiffScope::Staged, None)
                    .await
                    .unwrap()
                    .diff
                    .contains("+second")
            );
            assert!(
                git_workspace_status(subdir_path, None, GitDiffScope::Uncommitted, None)
                    .await
                    .unwrap()
                    .diff
                    .contains("+third")
            );
            let baseline = git_work_turn_baseline(subdir_path, None).await.unwrap();
            fs::write(repo.join(special), "fourth\n").unwrap();
            let last_turn =
                git_workspace_status(subdir_path, None, GitDiffScope::LastTurn, Some(&baseline))
                    .await
                    .unwrap();
            assert!(last_turn.diff.contains("-third"));
            assert!(last_turn.diff.contains("+fourth"));
            assert!(
                git_workspace_status(subdir_path, None, GitDiffScope::Staged, None)
                    .await
                    .unwrap()
                    .diff
                    .contains("+second")
            );
            let worktree = scratch.0.join("other checkout");
            git_workspace_action(
                repo_path,
                None,
                GitWorkspaceAction::Worktree {
                    path: worktree.to_str().unwrap().into(),
                    branch: "work-fixture".into(),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                fs::read_to_string(worktree.join(special)).unwrap(),
                "first\n"
            );
            assert_eq!(fs::read_to_string(repo.join(special)).unwrap(), "fourth\n");
        });
    }
}
