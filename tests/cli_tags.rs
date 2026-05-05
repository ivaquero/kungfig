mod common;

use std::fs;

use common::{run_kungfig, workspace, write_manifest_text};

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
