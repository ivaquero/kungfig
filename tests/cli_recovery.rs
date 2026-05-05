mod common;

use std::fs;

use common::{run_kungfig, workspace, write_manifest};

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
