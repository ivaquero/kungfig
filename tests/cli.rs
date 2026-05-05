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
    run_kungfig_with_env(workspace, args, &[])
}

fn run_kungfig_with_env(
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

fn write_manifest(workspace: &Path) {
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

fn write_manifest_text(workspace: &Path, manifest: &str) {
    fs::write(workspace.join("kungfig.toml"), manifest).expect("write manifest");
}

fn current_platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    }
}

fn other_platform_name() -> &'static str {
    match current_platform_name() {
        "macos" => "windows",
        "linux" => "windows",
        "windows" => "macos",
        _ => "windows",
    }
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
fn add_copies_existing_config_updates_manifest_and_records_state() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("live")).expect("create live dir");
    fs::write(
        workspace.join("live/.gitconfig"),
        "[user]\nname = \"ada\"\n",
    )
    .expect("write target");
    write_manifest_text(workspace.as_path(), "");

    let add = run_kungfig(
        &workspace,
        &["add", "live/.gitconfig", "--name", "gitconfig"],
    );
    assert!(add.status.success(), "{:?}", add);

    let source_path = workspace.join("dotfiles/gitconfig");
    assert_eq!(
        fs::read_to_string(&source_path).expect("read adopted source"),
        "[user]\nname = \"ada\"\n"
    );

    let manifest = fs::read_to_string(workspace.join("kungfig.toml")).expect("read manifest");
    assert!(manifest.contains("name = \"gitconfig\""));
    assert!(manifest.contains("source = \"dotfiles/gitconfig\""));
    assert!(manifest.contains("target = \"live/.gitconfig\""));

    let status = run_kungfig(&workspace, &["status"]);
    assert!(status.status.success(), "{:?}", status);
    assert!(String::from_utf8_lossy(&status.stdout).contains("synced"));
}

#[test]
fn apply_with_tag_only_updates_matching_items() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write git source");
    fs::write(workspace.join("repo/zshrc"), "shell\n").expect("write shell source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
tags = ["git", "core"]

[[items]]
name = "zshrc"
source = "repo/zshrc"
target = "live/.zshrc"
mode = "copy"
tags = ["shell"]
"#,
    );

    let apply = run_kungfig(&workspace, &["apply", "--tag", "git"]);
    assert!(apply.status.success(), "{:?}", apply);

    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("read tagged target"),
        "git\n"
    );
    assert!(!workspace.join("live/.zshrc").exists());
}

#[test]
fn status_with_tag_only_reports_matching_items() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write git source");
    fs::write(workspace.join("repo/zshrc"), "shell\n").expect("write shell source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
tags = ["git", "core"]

[[items]]
name = "zshrc"
source = "repo/zshrc"
target = "live/.zshrc"
mode = "copy"
tags = ["shell"]
"#,
    );

    let apply = run_kungfig(&workspace, &["apply"]);
    assert!(apply.status.success(), "{:?}", apply);

    fs::write(workspace.join("live/.gitconfig"), "git user edit\n").expect("edit git target");
    fs::write(workspace.join("live/.zshrc"), "shell user edit\n").expect("edit shell target");

    let status = run_kungfig(&workspace, &["status", "--tag", "git"]);
    assert!(!status.status.success(), "{:?}", status);

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("gitconfig"));
    assert!(stdout.contains("modified"));
    assert!(!stdout.contains("zshrc"));
}

#[test]
fn when_applies_item_on_matching_platform() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write source");
    write_manifest_text(
        &workspace,
        &format!(
            r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
when = "os == \"{}\""
"#,
            current_platform_name()
        ),
    );

    let apply = run_kungfig(&workspace, &["apply"]);
    assert!(apply.status.success(), "{:?}", apply);
    assert_eq!(
        fs::read_to_string(workspace.join("live/.gitconfig")).expect("read matching target"),
        "git\n"
    );
}

#[test]
fn when_skips_item_on_non_matching_platform() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write git source");
    fs::write(workspace.join("repo/zshrc"), "shell\n").expect("write shell source");
    write_manifest_text(
        &workspace,
        &format!(
            r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"

[[items]]
name = "zshrc"
source = "repo/zshrc"
target = "live/.zshrc"
mode = "copy"
when = 'os == "{}"'
"#,
            other_platform_name()
        ),
    );

    let apply = run_kungfig(&workspace, &["apply"]);
    assert!(apply.status.success(), "{:?}", apply);
    assert!(workspace.join("live/.gitconfig").exists());
    assert!(!workspace.join("live/.zshrc").exists());
}

#[test]
fn invalid_when_expression_is_rejected() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
when = 'arch == "arm64"'
"#,
    );

    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(stderr.contains("unsupported when variable"));
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
fn edit_opens_item_source_with_editor_env() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "[core]\neditor = vim\n").expect("write source");
    write_manifest(&workspace);

    let editor_script = workspace.join("fake-editor.sh");
    fs::write(
        &editor_script,
        "#!/bin/sh\nprintf %s \"$1\" > \"$EDIT_LOG\"\n",
    )
    .expect("write editor script");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&editor_script)
            .expect("editor metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&editor_script, perms).expect("chmod editor script");
    }

    let edit_log = workspace.join("edit.log");
    let edit = run_kungfig_with_env(
        &workspace,
        &["edit", "gitconfig"],
        &[("EDITOR", &editor_script), ("EDIT_LOG", &edit_log)],
    );
    assert!(edit.status.success(), "{:?}", edit);

    let opened = fs::read_to_string(&edit_log).expect("read edit log");
    let expected = fs::canonicalize(workspace.join("repo/gitconfig")).expect("canonical source");
    let actual = fs::canonicalize(opened).expect("canonical opened path");
    assert_eq!(actual, expected);
}

#[test]
fn diff_summary_lists_changed_items_only() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v1\n").expect("write source");
    write_manifest(&workspace);

    let first_apply = run_kungfig(&workspace, &["apply"]);
    assert!(first_apply.status.success(), "{:?}", first_apply);

    fs::write(workspace.join("repo/gitconfig"), "v2\n").expect("write source update");
    let diff = run_kungfig(&workspace, &["diff", "--summary"]);
    assert!(diff.status.success(), "{:?}", diff);

    let stdout = String::from_utf8_lossy(&diff.stdout);
    assert!(stdout.contains("update"));
    assert!(stdout.contains("gitconfig"));
    assert!(!stdout.contains("== gitconfig =="));
}

#[test]
fn diff_can_focus_on_single_item() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v2\n").expect("write git source");
    fs::write(workspace.join("repo/zshrc"), "alias ll=eza\n").expect("write zsh source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"

[[items]]
name = "zshrc"
source = "repo/zshrc"
target = "live/.zshrc"
mode = "copy"
"#,
    );
    fs::create_dir_all(workspace.join("live")).expect("create live dir");
    fs::write(workspace.join("live/.gitconfig"), "v1\n").expect("write git target");
    fs::write(workspace.join("live/.zshrc"), "alias ll=ls\n").expect("write zsh target");

    let diff = run_kungfig(&workspace, &["diff", "gitconfig"]);
    assert!(diff.status.success(), "{:?}", diff);

    let stdout = String::from_utf8_lossy(&diff.stdout);
    assert!(stdout.contains("== gitconfig =="));
    assert!(stdout.contains("-v1"));
    assert!(stdout.contains("+v2"));
    assert!(!stdout.contains("== zshrc =="));
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
