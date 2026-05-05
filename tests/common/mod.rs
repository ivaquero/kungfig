#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn workspace() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    let counter = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "kungfig-test-{}-{unique}-{counter}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp workspace");
    path
}

pub fn run_kungfig(workspace: &Path, args: &[&str]) -> std::process::Output {
    run_kungfig_with_env(workspace, args, &[])
}

pub fn run_kungfig_with_env(
    workspace: &Path,
    args: &[&str],
    envs: &[(&str, &std::path::Path)],
) -> std::process::Output {
    let state_root = workspace.join(".state");
    let mut command = Command::new(env!("CARGO_BIN_EXE_kungfig"));
    command
        .args(args)
        .current_dir(workspace)
        .env("KUNGFIG_STATE_DB", state_root.join("state.db"))
        .env("KUNGFIG_BACKUP_DIR", state_root.join("backups"));

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("failed to run kungfig binary")
}

pub fn run_kungfig_with_string_env(
    workspace: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> std::process::Output {
    let state_root = workspace.join(".state");
    let mut command = Command::new(env!("CARGO_BIN_EXE_kungfig"));
    command
        .args(args)
        .current_dir(workspace)
        .env("KUNGFIG_STATE_DB", state_root.join("state.db"))
        .env("KUNGFIG_BACKUP_DIR", state_root.join("backups"));

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("failed to run kungfig binary")
}

pub fn run_git(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("failed to run git")
}

pub fn write_manifest(workspace: &Path) {
    write_manifest_text(
        workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
"#,
    );
}

pub fn write_manifest_text(workspace: &Path, manifest: &str) {
    fs::write(workspace.join("kungfig.toml"), manifest).expect("write manifest");
}
#[allow(dead_code)]
pub fn current_platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    }
}

pub fn other_platform_name() -> &'static str {
    match current_platform_name() {
        "macos" => "windows",
        "linux" => "windows",
        "windows" => "macos",
        _ => "windows",
    }
}
