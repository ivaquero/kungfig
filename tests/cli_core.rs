mod common;

use std::fs;

use common::{run_kungfig, run_kungfig_with_env, workspace, write_manifest, write_manifest_text};

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
fn dry_run_reports_update_without_touching_target() {
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
    assert!(!stdout.contains("backup:"));
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
