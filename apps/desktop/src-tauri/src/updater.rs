//! In-app updater for Hermes Grok Bot (Chromium & WebKitGTK desktop hosts).
//!
//! Supports fast frontend-only updates (rebuilding only JS/CSS/HTML and atomically
//! swapping the Chromium release without recompiling Rust/Tauri) as well as full
//! native rebuilds when `apps/desktop/src-tauri` changes.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const DEFAULT_GITHUB_REPO: &str = "FlXZ22/hermes-bot";
const MAX_LOG_LINES: usize = 250;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfig {
    pub repo_path: Option<String>,
    pub target_branch: String,
    pub auto_check: bool,
    pub github_repo: String,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        let default_repo = Path::new(env!("HERMES_REPO_DIR"));
        Self {
            repo_path: if default_repo.join(".git").exists() {
                Some(default_repo.display().to_string())
            } else {
                None
            },
            target_branch: "main".to_string(),
            auto_check: true,
            github_repo: DEFAULT_GITHUB_REPO.to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseMeta {
    version: Option<String>,
    commit: Option<String>,
    short_commit: Option<String>,
    branch: Option<String>,
    built_at: Option<String>,
    native_commit: Option<String>,
    host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub current_commit: String,
    pub current_full_commit: String,
    pub native_commit: String,
    pub built_at: String,
    pub host: String,
    pub repo_path: Option<String>,
    pub repo_valid: bool,
    pub repo_dirty: bool,
    pub current_branch: String,
    pub target_branch: String,
    pub available_branches: Vec<String>,
    pub github_repo: String,
    pub auto_check: bool,
    pub phase: String,
    pub update_available: bool,
    pub commits_behind: usize,
    pub latest_commit: Option<String>,
    pub latest_commit_message: Option<String>,
    pub latest_commit_date: Option<String>,
    pub rust_rebuild_needed: bool,
    pub latest_release_tag: Option<String>,
    pub latest_release_url: Option<String>,
    pub step: Option<String>,
    pub logs: Vec<String>,
    pub last_checked_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsPatch {
    pub target_branch: Option<String>,
    pub repo_path: Option<String>,
    pub auto_check: Option<bool>,
}

#[derive(Clone)]
pub struct AppUpdater {
    inner: Arc<Mutex<UpdateRuntime>>,
}

struct UpdateRuntime {
    config: UpdateConfig,
    phase: String,
    update_available: bool,
    commits_behind: usize,
    latest_commit: Option<String>,
    latest_commit_message: Option<String>,
    latest_commit_date: Option<String>,
    rust_rebuild_needed: bool,
    latest_release_tag: Option<String>,
    latest_release_url: Option<String>,
    step: Option<String>,
    logs: Vec<String>,
    last_checked_at: Option<String>,
    error: Option<String>,
    in_flight: bool,
}

fn config_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .ok_or("Could not resolve config directory")?;
    Ok(base.join("hermes-grok-bot").join("updater.json"))
}

fn load_config() -> UpdateConfig {
    let Ok(path) = config_path() else {
        return UpdateConfig::default();
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return UpdateConfig::default();
    };
    let mut cfg: UpdateConfig = serde_json::from_str(&raw).unwrap_or_default();
    if cfg.github_repo.trim().is_empty() {
        cfg.github_repo = DEFAULT_GITHUB_REPO.to_string();
    }
    if cfg.target_branch.trim().is_empty() {
        cfg.target_branch = "main".to_string();
    }
    if cfg.repo_path.is_none() {
        cfg.repo_path = UpdateConfig::default().repo_path;
    }
    cfg
}

fn save_config(cfg: &UpdateConfig) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed creating config dir: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    let body = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, body).map_err(|e| format!("Failed saving updater config: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn validate_branch_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('-')
        || trimmed.contains("..")
        || trimmed.contains('@')
        || trimmed.contains('\\')
        || trimmed.ends_with('/')
        || trimmed.ends_with(".lock")
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
    {
        return Err(format!("Invalid git branch name: {name}"));
    }
    Ok(trimmed.to_string())
}

fn detect_host() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(name) = exe.file_name().and_then(|s| s.to_str()) {
            if name.contains("hermes-native") {
                return "chromium".to_string();
            }
        }
    }
    "webkit".to_string()
}

fn read_release_meta() -> Option<ReleaseMeta> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("release-meta.json"));
        }
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")));
    if let Some(base) = data_home {
        candidates.push(base.join("hermes-grok-bot/chromium/current/resources/release-meta.json"));
    }
    for path in candidates {
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(meta) = serde_json::from_str::<ReleaseMeta>(&raw) {
                return Some(meta);
            }
        }
    }
    None
}

fn git_sync(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("git {} failed with {}", args.join(" "), out.status)
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn resolve_repo_path(cfg: &UpdateConfig) -> Option<PathBuf> {
    cfg.repo_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.join(".git").exists() && p.join("package.json").exists())
}

fn list_branches(repo: &Path) -> Vec<String> {
    let mut branches = Vec::new();
    if let Ok(out) = git_sync(repo, &["for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes/origin"]) {
        for line in out.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "origin/HEAD" || trimmed == "origin" {
                continue;
            }
            let normalized = trimmed.strip_prefix("origin/").unwrap_or(trimmed).to_string();
            if !branches.contains(&normalized) {
                branches.push(normalized);
            }
        }
    }
    if branches.is_empty() {
        branches.push("main".to_string());
    }
    branches.sort();
    // Put "main" first if present.
    if let Some(pos) = branches.iter().position(|b| b == "main") {
        let main = branches.remove(pos);
        branches.insert(0, main);
    }
    branches
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}

fn rust_changed_between(repo: &Path, from_rev: &str, to_rev: &str) -> bool {
    if from_rev.is_empty() || from_rev == "unknown" {
        return true;
    }
    let range = format!("{from_rev}..{to_rev}");
    match git_sync(repo, &["diff", "--name-only", &range, "--", "apps/desktop/src-tauri"]) {
        Ok(out) => !out.trim().is_empty(),
        Err(_) => true,
    }
}

fn lockfile_changed_between(repo: &Path, from_rev: &str, to_rev: &str) -> bool {
    if from_rev.is_empty() || from_rev == "unknown" {
        return false;
    }
    let range = format!("{from_rev}..{to_rev}");
    match git_sync(repo, &["diff", "--name-only", &range, "--", "package-lock.json"]) {
        Ok(out) => !out.trim().is_empty(),
        Err(_) => false,
    }
}

impl AppUpdater {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(UpdateRuntime {
                config: load_config(),
                phase: "idle".to_string(),
                update_available: false,
                commits_behind: 0,
                latest_commit: None,
                latest_commit_message: None,
                latest_commit_date: None,
                rust_rebuild_needed: false,
                latest_release_tag: None,
                latest_release_url: None,
                step: None,
                logs: Vec::new(),
                last_checked_at: None,
                error: None,
                in_flight: false,
            })),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        let rt = self.inner.lock().unwrap();
        let meta = read_release_meta();
        let host = meta
            .as_ref()
            .and_then(|m| m.host.clone())
            .unwrap_or_else(detect_host);
        let current_version = meta
            .as_ref()
            .and_then(|m| m.version.clone())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        let current_full_commit = meta
            .as_ref()
            .and_then(|m| m.commit.clone())
            .unwrap_or_else(|| env!("HERMES_BUILD_FULL_COMMIT").to_string());
        let current_commit = meta
            .as_ref()
            .and_then(|m| m.short_commit.clone())
            .unwrap_or_else(|| env!("HERMES_BUILD_COMMIT").to_string());
        let native_commit = meta
            .as_ref()
            .and_then(|m| m.native_commit.clone())
            .map(|s| short_sha(&s))
            .unwrap_or_else(|| env!("HERMES_BUILD_COMMIT").to_string());
        let built_at = meta
            .as_ref()
            .and_then(|m| m.built_at.clone())
            .unwrap_or_else(|| env!("HERMES_BUILD_TIME").to_string());

        let repo = resolve_repo_path(&rt.config);
        let repo_valid = repo.is_some();
        let (repo_dirty, current_branch, available_branches) = if let Some(ref repo_dir) = repo {
            let dirty = git_sync(repo_dir, &["status", "--porcelain"])
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            let branch = git_sync(repo_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|_| {
                    meta.as_ref()
                        .and_then(|m| m.branch.clone())
                        .unwrap_or_else(|| env!("HERMES_BUILD_BRANCH").to_string())
                });
            let branches = list_branches(repo_dir);
            (dirty, branch, branches)
        } else {
            let branch = meta
                .as_ref()
                .and_then(|m| m.branch.clone())
                .unwrap_or_else(|| env!("HERMES_BUILD_BRANCH").to_string());
            (false, branch, vec![rt.config.target_branch.clone()])
        };

        UpdateStatus {
            current_version,
            current_commit,
            current_full_commit,
            native_commit,
            built_at,
            host,
            repo_path: rt.config.repo_path.clone(),
            repo_valid,
            repo_dirty,
            current_branch,
            target_branch: rt.config.target_branch.clone(),
            available_branches,
            github_repo: rt.config.github_repo.clone(),
            auto_check: rt.config.auto_check,
            phase: rt.phase.clone(),
            update_available: rt.update_available,
            commits_behind: rt.commits_behind,
            latest_commit: rt.latest_commit.clone(),
            latest_commit_message: rt.latest_commit_message.clone(),
            latest_commit_date: rt.latest_commit_date.clone(),
            rust_rebuild_needed: rt.rust_rebuild_needed,
            latest_release_tag: rt.latest_release_tag.clone(),
            latest_release_url: rt.latest_release_url.clone(),
            step: rt.step.clone(),
            logs: rt.logs.clone(),
            last_checked_at: rt.last_checked_at.clone(),
            error: rt.error.clone(),
        }
    }

    pub fn update_settings(&self, patch: UpdateSettingsPatch) -> Result<UpdateStatus, String> {
        {
            let mut rt = self.inner.lock().unwrap();
            if let Some(branch) = patch.target_branch {
                rt.config.target_branch = validate_branch_name(&branch)?;
            }
            if let Some(repo_path) = patch.repo_path {
                let trimmed = repo_path.trim().to_string();
                rt.config.repo_path = if trimmed.is_empty() { None } else { Some(trimmed) };
            }
            if let Some(auto_check) = patch.auto_check {
                rt.config.auto_check = auto_check;
            }
            save_config(&rt.config)?;
        }
        Ok(self.status())
    }

    pub async fn check_for_updates(&self) -> Result<UpdateStatus, String> {
        let (cfg, installed_full, native_full) = {
            let mut rt = self.inner.lock().unwrap();
            if rt.in_flight {
                return Ok(self.status());
            }
            rt.phase = "checking".to_string();
            rt.error = None;
            rt.step = Some("Checking GitHub for updates…".to_string());
            let meta = read_release_meta();
            let installed_full = meta
                .as_ref()
                .and_then(|m| m.commit.clone())
                .unwrap_or_else(|| env!("HERMES_BUILD_FULL_COMMIT").to_string());
            let native_full = meta
                .as_ref()
                .and_then(|m| m.native_commit.clone())
                .unwrap_or_else(|| env!("HERMES_BUILD_FULL_COMMIT").to_string());
            (rt.config.clone(), installed_full, native_full)
        };

        let github_release = fetch_latest_github_release(&cfg.github_repo).await;

        let check_res = tokio::task::spawn_blocking(move || -> Result<CheckResult, String> {
            let Some(repo) = resolve_repo_path(&cfg) else {
                return Ok(CheckResult {
                    update_available: false,
                    commits_behind: 0,
                    latest_commit: None,
                    latest_commit_message: None,
                    latest_commit_date: None,
                    rust_rebuild_needed: false,
                    note: Some("Local git repository not configured. Checking GitHub Releases only.".to_string()),
                });
            };

            let branch = validate_branch_name(&cfg.target_branch)?;
            git_sync(&repo, &["fetch", "origin", "--prune"])?;

            let remote_ref = format!("origin/{branch}");
            let target_ref = if git_sync(&repo, &["rev-parse", "--verify", &remote_ref]).is_ok() {
                remote_ref
            } else {
                branch.clone()
            };

            let latest_full = git_sync(&repo, &["rev-parse", &target_ref])?;
            let latest_msg = git_sync(&repo, &["log", "-1", "--pretty=%s", &target_ref]).ok();
            let latest_date = git_sync(&repo, &["log", "-1", "--pretty=%cI", &target_ref]).ok();

            let (commits_behind, update_available) = if installed_full == latest_full {
                (0, false)
            } else {
                let count = git_sync(
                    &repo,
                    &["rev-list", "--count", &format!("{installed_full}..{target_ref}")],
                )
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1);
                (count.max(1), true)
            };

            let rust_rebuild_needed =
                update_available && rust_changed_between(&repo, &native_full, &target_ref);

            Ok(CheckResult {
                update_available,
                commits_behind,
                latest_commit: Some(short_sha(&latest_full)),
                latest_commit_message: latest_msg,
                latest_commit_date: latest_date,
                rust_rebuild_needed,
                note: None,
            })
        })
        .await
        .map_err(|e| e.to_string())?;

        let now = Utc::now().to_rfc3339();
        {
            let mut rt = self.inner.lock().unwrap();
            rt.last_checked_at = Some(now);
            rt.step = None;
            if let Some((tag, url)) = github_release {
                rt.latest_release_tag = Some(tag);
                rt.latest_release_url = Some(url);
            }
            match check_res {
                Ok(res) => {
                    rt.update_available = res.update_available;
                    rt.commits_behind = res.commits_behind;
                    rt.latest_commit = res.latest_commit;
                    rt.latest_commit_message = res.latest_commit_message;
                    rt.latest_commit_date = res.latest_commit_date;
                    rt.rust_rebuild_needed = res.rust_rebuild_needed;
                    rt.phase = if res.update_available {
                        "available".to_string()
                    } else {
                        "up_to_date".to_string()
                    };
                    if let Some(note) = res.note {
                        push_log(&mut rt.logs, note);
                    }
                }
                Err(err) => {
                    rt.phase = "error".to_string();
                    rt.error = Some(err);
                }
            }
        }

        Ok(self.status())
    }

    pub fn apply_update(&self, mode: String, force_native_rebuild: bool) -> Result<UpdateStatus, String> {
        let mode = match mode.as_str() {
            "remote" | "local" => mode,
            _ => return Err(format!("Unsupported update mode: {mode}")),
        };

        let (cfg, host, native_full, installed_full) = {
            let mut rt = self.inner.lock().unwrap();
            if rt.in_flight {
                return Err("An update is already running".to_string());
            }
            let meta = read_release_meta();
            let host = meta
                .as_ref()
                .and_then(|m| m.host.clone())
                .unwrap_or_else(detect_host);
            let native_full = meta
                .as_ref()
                .and_then(|m| m.native_commit.clone())
                .unwrap_or_else(|| env!("HERMES_BUILD_FULL_COMMIT").to_string());
            let installed_full = meta
                .as_ref()
                .and_then(|m| m.commit.clone())
                .unwrap_or_else(|| env!("HERMES_BUILD_FULL_COMMIT").to_string());

            rt.in_flight = true;
            rt.phase = "updating".to_string();
            rt.error = None;
            rt.logs.clear();
            rt.step = Some("Preparing update…".to_string());
            (rt.config.clone(), host, native_full, installed_full)
        };

        let updater = self.clone();
        tokio::spawn(async move {
            let outcome = updater
                .run_update_pipeline(cfg, host, mode, force_native_rebuild, installed_full, native_full)
                .await;
            let mut rt = updater.inner.lock().unwrap();
            rt.in_flight = false;
            match outcome {
                Ok(new_short) => {
                    rt.phase = "restart_required".to_string();
                    rt.update_available = false;
                    rt.commits_behind = 0;
                    rt.latest_commit = Some(new_short);
                    rt.step = Some("Update installed. Restart the app to apply.".to_string());
                    rt.error = None;
                }
                Err(err) => {
                    rt.phase = "error".to_string();
                    rt.step = None;
                    rt.error = Some(err.clone());
                    push_log(&mut rt.logs, format!("ERROR: {err}"));
                }
            }
        });

        Ok(self.status())
    }

    async fn run_update_pipeline(
        &self,
        cfg: UpdateConfig,
        host: String,
        mode: String,
        force_native_rebuild: bool,
        installed_full: String,
        native_full: String,
    ) -> Result<String, String> {
        let repo = resolve_repo_path(&cfg)
            .ok_or_else(|| "Local repository path is not valid. Set a valid git checkout path first.".to_string())?;

        if mode == "remote" {
            let branch = validate_branch_name(&cfg.target_branch)?;
            self.set_step(format!("Checking working tree & pulling origin/{branch}…"));
            let dirty = git_sync(&repo, &["status", "--porcelain"])
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if dirty {
                return Err(
                    "Your local repository has uncommitted changes. Commit/stash them first, or use 'Install Local Workspace' to preview local changes safely."
                        .to_string(),
                );
            }

            self.run_cmd(&repo, "git", &["fetch", "origin", "--prune"], &[]).await?;
            let local_exists = git_sync(&repo, &["show-ref", "--verify", &format!("refs/heads/{branch}")]).is_ok();
            let remote_ref = format!("origin/{branch}");
            if local_exists {
                self.run_cmd(&repo, "git", &["checkout", &branch], &[]).await?;
                self.run_cmd(&repo, "git", &["merge", "--ff-only", &remote_ref], &[]).await?;
            } else {
                self.run_cmd(&repo, "git", &["checkout", "-B", &branch, &remote_ref], &[]).await?;
            }
        } else {
            self.set_step("Using current local workspace checkout…");
            self.append_log("Mode: Local workspace preview (git state untouched).");
        }

        let new_full = git_sync(&repo, &["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".into());
        let new_short = short_sha(&new_full);

        // Install JS dependencies only if node_modules is missing or package-lock.json changed.
        if !repo.join("node_modules").is_dir()
            || lockfile_changed_between(&repo, &installed_full, "HEAD")
        {
            self.set_step("Installing npm dependencies (npm ci)…");
            self.run_cmd(&repo, "npm", &["ci"], &[]).await?;
        }

        // Always build frontend packages (fast: ~2-3s).
        self.set_step("Building UI packages (npm run build)…");
        self.run_cmd(&repo, "npm", &["run", "build"], &[]).await?;

        let native_bin = repo.join("apps/desktop/src-tauri/target/release/hermes-native");
        let rust_changed = force_native_rebuild
            || !native_bin.exists()
            || rust_changed_between(&repo, &native_full, "HEAD")
            || (mode == "local"
                && git_sync(&repo, &["status", "--porcelain", "--", "apps/desktop/src-tauri"])
                    .map(|s| !s.is_empty())
                    .unwrap_or(false));

        let mut installed_native_commit = native_full.clone();

        if host == "webkit" {
            self.set_step("Building Tauri WebKitGTK binary…");
            self.run_cmd(&repo, "npm", &["run", "desktop:build"], &[]).await?;
            self.set_step("Installing WebKitGTK release…");
            self.run_cmd(&repo, "bash", &["scripts/install-linux.sh"], &[]).await?;
        } else {
            if rust_changed {
                self.set_step("Rust backend changed — rebuilding hermes-native…");
                self.run_cmd(
                    &repo,
                    "cargo",
                    &[
                        "build",
                        "--manifest-path",
                        "apps/desktop/src-tauri/Cargo.toml",
                        "--release",
                        "--no-default-features",
                        "--bin",
                        "hermes-native",
                    ],
                    &[],
                )
                .await?;
                installed_native_commit = new_full.clone();
            } else {
                self.append_log(
                    "No Rust changes detected — skipping Rust/Tauri rebuild (fast frontend update).",
                );
            }

            self.set_step("Installing Chromium desktop release atomically…");
            self.run_cmd(
                &repo,
                "bash",
                &["scripts/install-chromium.sh"],
                &[("HERMES_NATIVE_COMMIT", installed_native_commit.as_str())],
            )
            .await?;
        }

        self.append_log(format!("Update complete! Installed commit {new_short}."));
        Ok(new_short)
    }

    fn set_step(&self, step: impl Into<String>) {
        let text = step.into();
        let mut rt = self.inner.lock().unwrap();
        rt.step = Some(text.clone());
        push_log(&mut rt.logs, format!("==> {text}"));
    }

    fn append_log(&self, line: impl Into<String>) {
        let mut rt = self.inner.lock().unwrap();
        push_log(&mut rt.logs, line.into());
    }

    async fn run_cmd(
        &self,
        cwd: &Path,
        program: &str,
        args: &[&str],
        envs: &[(&str, &str)],
    ) -> Result<(), String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in envs {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start `{program} {}`: {e}", args.join(" ")))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let u1 = self.clone();
        let t1 = tokio::spawn(async move {
            if let Some(out) = stdout {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        u1.append_log(line);
                    }
                }
            }
        });

        let u2 = self.clone();
        let t2 = tokio::spawn(async move {
            if let Some(err) = stderr {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !line.trim().is_empty() {
                        u2.append_log(line);
                    }
                }
            }
        });

        let status = child.wait().await.map_err(|e| e.to_string())?;
        let _ = tokio::join!(t1, t2);
        if !status.success() {
            return Err(format!(
                "Command `{program} {}` exited with status {status}",
                args.join(" ")
            ));
        }
        Ok(())
    }
}

struct CheckResult {
    update_available: bool,
    commits_behind: usize,
    latest_commit: Option<String>,
    latest_commit_message: Option<String>,
    latest_commit_date: Option<String>,
    rust_rebuild_needed: bool,
    note: Option<String>,
}

fn push_log(logs: &mut Vec<String>, line: String) {
    logs.push(line);
    if logs.len() > MAX_LOG_LINES {
        let excess = logs.len() - MAX_LOG_LINES;
        logs.drain(0..excess);
    }
}

async fn fetch_latest_github_release(github_repo: &str) -> Option<(String, String)> {
    let url = format!("https://api.github.com/repos/{github_repo}/releases/latest");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .user_agent("hermes-grok-bot-updater")
        .build()
        .ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let val: serde_json::Value = resp.json().await.ok()?;
    let tag = val.get("tag_name")?.as_str()?.to_string();
    let html = val.get("html_url")?.as_str()?.to_string();
    Some((tag, html))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_validation_blocks_injection_and_allows_valid_refs() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("feat/settings-updates").is_ok());
        assert!(validate_branch_name("release/v1.2.3").is_ok());
        for bad in ["", "--upload-pack=sh", "../main", "a..b", "foo;rm -rf /", "bad branch", "foo.lock"] {
            assert!(validate_branch_name(bad).is_err(), "expected {bad:?} to fail");
        }
    }

    #[test]
    fn updater_initial_status_is_well_formed() {
        let updater = AppUpdater::new();
        let status = updater.status();
        assert!(!status.current_version.is_empty());
        assert!(!status.current_commit.is_empty());
        assert_eq!(status.phase, "idle");
        assert_eq!(status.github_repo, DEFAULT_GITHUB_REPO);
    }
}
