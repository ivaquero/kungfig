mod common;

use std::fs;

use common::{run_kungfig, workspace, write_manifest_text};

#[test]
fn empty_single_target_is_rejected() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = ""
mode = "copy"
"#,
    );

    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(stderr.contains("empty target path"));
}

#[test]
fn empty_platform_target_table_is_rejected() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = {}
mode = "copy"
"#,
    );

    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(stderr.contains("at least one platform target"));
}

#[test]
fn empty_platform_branch_is_rejected() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target.macos = ""
target.linux = "live/.gitconfig"
mode = "copy"
"#,
    );

    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(stderr.contains("empty `macos` target path"));
}

#[test]
fn duplicate_alias_is_rejected() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "git\n").expect("write source");
    fs::write(workspace.join("repo/zshrc"), "zsh\n").expect("write source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
alias = "git"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"

[[items]]
name = "zshrc"
alias = "git"
source = "repo/zshrc"
target = "live/.zshrc"
mode = "copy"
"#,
    );

    let status = run_kungfig(&workspace, &["status"]);
    assert!(!status.status.success(), "{:?}", status);
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(stderr.contains("duplicate item alias"));
}
