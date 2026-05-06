mod common;

use std::fs;

use common::{run_kungfig, workspace, write_manifest, write_manifest_text};

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
fn diff_can_focus_on_alias() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(workspace.join("repo/gitconfig"), "v2\n").expect("write git source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
alias = "git"
source = "repo/gitconfig"
target = "live/.gitconfig"
mode = "copy"
"#,
    );
    fs::create_dir_all(workspace.join("live")).expect("create live dir");
    fs::write(workspace.join("live/.gitconfig"), "v1\n").expect("write git target");

    let diff = run_kungfig(&workspace, &["diff", "git"]);
    assert!(diff.status.success(), "{:?}", diff);

    let stdout = String::from_utf8_lossy(&diff.stdout);
    assert!(stdout.contains("== gitconfig =="));
    assert!(stdout.contains("-v1"));
    assert!(stdout.contains("+v2"));
}
