mod common;

use std::fs;

use common::{
    current_platform_name, other_platform_name, run_kungfig, workspace, write_manifest_text,
};

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
