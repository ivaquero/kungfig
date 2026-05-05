use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn workspace() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("kungfig-test-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).expect("create temp workspace");
    path
}

fn run_kungfig(workspace: &Path, args: &[&str]) -> std::process::Output {
    let state_root = workspace.join(".state");
    Command::new(env!("CARGO_BIN_EXE_kungfig"))
        .args(args)
        .current_dir(workspace)
        .env("KUNGFIG_STATE_DB", state_root.join("state.db"))
        .env("KUNGFIG_BACKUP_DIR", state_root.join("backups"))
        .output()
        .expect("failed to run kungfig binary")
}

fn write_manifest(workspace: &Path) {
    let manifest = r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
"#;
    fs::write(workspace.join("kungfig.toml"), manifest).expect("write manifest");
}

#[test]
fn plan_reports_create_for_missing_target() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(
        workspace.join("repo/gitconfig"),
        "[user]\nname = \"kungfig\"\n",
    )
    .expect("write source");
    write_manifest(&workspace);

    let output = run_kungfig(&workspace, &["plan"]);
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("create"));
    assert!(stdout.contains("gitconfig"));
}

#[test]
fn apply_copies_file_and_status_becomes_synced() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "[core]\neditor = vim\n").expect("write source");
    write_manifest(&workspace);

    let apply = run_kungfig(&workspace, &["apply"]);
    assert!(apply.status.success(), "{:?}", apply);
    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("read target"),
        "[core]\neditor = vim\n"
    );

    let status = run_kungfig(&workspace, &["status"]);
    assert!(status.status.success(), "{:?}", status);
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("synced"));
    assert!(stdout.contains("gitconfig"));
}

#[test]
fn doctor_reports_healthy_workspace() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "[core]\neditor = vim\n").expect("write source");
    write_manifest(&workspace);

    let output = run_kungfig(&workspace, &["doctor"]);
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("manifest"));
    assert!(stdout.contains("config-repo"));
    assert!(stdout.contains("target-writable"));
    assert!(stdout.contains("state-db"));
    assert!(stdout.contains("backup-dir"));
}

#[test]
fn dry_run_reports_backup_without_touching_target() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v1\n").expect("write v1");
    write_manifest(&workspace);

    let first_apply = run_kungfig(&workspace, &["apply"]);
    assert!(first_apply.status.success(), "{:?}", first_apply);

    fs::write(workspace.join("repo/gitconfig"), "v2\n").expect("write v2");
    let dry_run = run_kungfig(&workspace, &["apply", "--dry-run"]);
    assert!(dry_run.status.success(), "{:?}", dry_run);

    let stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(stdout.contains("would-update"));
    assert!(stdout.contains("backup:"));
    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("read unchanged target"),
        "v1\n"
    );
}

#[test]
fn status_reports_modified_when_target_changes_only() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v1\n").expect("write v1");
    write_manifest(&workspace);

    let first_apply = run_kungfig(&workspace, &["apply"]);
    assert!(first_apply.status.success(), "{:?}", first_apply);

    fs::write(workspace.join("live/.gitconfig"), "user edit\n").expect("edit target");
    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("modified"));
}

#[test]
fn apply_refuses_conflict_when_source_and_target_both_changed() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v1\n").expect("write v1");
    write_manifest(&workspace);

    let first_apply = run_kungfig(&workspace, &["apply"]);
    assert!(first_apply.status.success(), "{:?}", first_apply);

    fs::write(workspace.join("repo/gitconfig"), "v2\n").expect("write v2");
    fs::write(workspace.join("live/.gitconfig"), "user edit\n").expect("edit target");

    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);
    assert!(String::from_utf8_lossy(&status.stdout).contains("conflict"));

    let apply = run_kungfig(&workspace, &["apply"]);
    assert!(!apply.status.success(), "{:?}", apply);
    assert!(String::from_utf8_lossy(&apply.stdout).contains("conflict"));
    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("target preserved"),
        "user edit\n"
    );
}

#[test]
fn rollback_restores_latest_backup() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v1\n").expect("write v1");
    write_manifest(&workspace);

    let first_apply = run_kungfig(&workspace, &["apply"]);
    assert!(first_apply.status.success(), "{:?}", first_apply);

    fs::write(workspace.join("repo/gitconfig"), "v2\n").expect("write v2");
    let second_apply = run_kungfig(&workspace, &["apply"]);
    assert!(second_apply.status.success(), "{:?}", second_apply);
    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("read v2 target"),
        "v2\n"
    );

    let rollback = run_kungfig(&workspace, &["rollback"]);
    assert!(rollback.status.success(), "{:?}", rollback);
    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("read restored target"),
        "v1\n"
    );
}
