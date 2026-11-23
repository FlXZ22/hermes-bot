use std::path::PathBuf;
use std::process::Command;

fn git_output(repo: &PathBuf, args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).current_dir(repo).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let repo_dir = manifest_dir
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.clone());

    println!("cargo:rustc-env=HERMES_REPO_DIR={}", repo_dir.display());
    println!("cargo:rerun-if-changed={}", repo_dir.join(".git/HEAD").display());
    println!("cargo:rerun-if-changed={}", repo_dir.join(".git/packed-refs").display());

    let short_commit = git_output(&repo_dir, &["rev-parse", "--short=12", "HEAD"])
        .unwrap_or_else(|| "unknown".into());
    let full_commit = git_output(&repo_dir, &["rev-parse", "HEAD"])
        .unwrap_or_else(|| "unknown".into());
    let branch = git_output(&repo_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "main".into());
    let built_at = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=HERMES_BUILD_COMMIT={short_commit}");
    println!("cargo:rustc-env=HERMES_BUILD_FULL_COMMIT={full_commit}");
    println!("cargo:rustc-env=HERMES_BUILD_BRANCH={branch}");
    println!("cargo:rustc-env=HERMES_BUILD_TIME={built_at}");

    #[cfg(feature = "tauri-host")]
    tauri_build::build();
}

