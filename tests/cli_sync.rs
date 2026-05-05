mod common;

use std::fs;
use std::path::Path;

use common::{run_git, run_kungfig, workspace, write_manifest_text};

fn git_ok(workspace: &Path, args: &[&str]) {
    let output = run_git(workspace, args);
    assert!(output.status.success(), "{:?}", output);
}

#[test]
fn sync_pulls_sources_applies_changes_and_reports_git_status() {
    let workspace = workspace();
    let origin = workspace.join("origin.git");
    let seed = workspace.join("seed");
    let project = workspace.join("project");
    let live = workspace.join("live");

    fs::create_dir_all(&seed).expect("create seed dir");
    fs::create_dir_all(&live).expect("create live dir");

    git_ok(&workspace, &["init", "--bare", "origin.git"]);
    git_ok(&seed, &["init"]);
    git_ok(&seed, &["config", "user.name", "Ada"]);
    git_ok(&seed, &["config", "user.email", "ada@example.com"]);

    fs::create_dir_all(seed.join("repo")).expect("create source dir");
    fs::write(seed.join("repo/gitconfig"), "v1\n").expect("write v1 source");
    fs::write(seed.join(".gitignore"), ".state/\n").expect("write gitignore");
    write_manifest_text(
        &seed,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "../live/.gitconfig"
mode = "copy"
"#,
    );

    git_ok(&seed, &["add", "."]);
    git_ok(&seed, &["commit", "-m", "initial"]);
    let branch_output = run_git(&seed, &["branch", "--show-current"]);
    assert!(branch_output.status.success(), "{:?}", branch_output);
    let branch = String::from_utf8(branch_output.stdout)
        .expect("branch name")
        .trim()
        .to_string();
    let origin_path = origin.to_str().expect("origin path");
    git_ok(&seed, &["remote", "add", "origin", origin_path]);
    git_ok(&seed, &["push", "-u", "origin", &branch]);

    git_ok(&workspace, &["clone", "origin.git", "project"]);

    fs::write(seed.join("repo/gitconfig"), "v2\n").expect("write v2 source");
    git_ok(&seed, &["add", "repo/gitconfig"]);
    git_ok(&seed, &["commit", "-m", "update source"]);
    git_ok(&seed, &["push"]);

    let sync = run_kungfig(&project, &["sync"]);
    assert!(sync.status.success(), "{:?}", sync);

    assert_eq!(
        fs::read_to_string(project.join("repo/gitconfig")).expect("read pulled source"),
        "v2\n"
    );
    assert_eq!(
        fs::read_to_string(live.join(".gitconfig")).expect("read applied target"),
        "v2\n"
    );

    let stdout = String::from_utf8_lossy(&sync.stdout);
    assert!(stdout.contains("updated") || stdout.contains("created"));
    assert!(stdout.contains("git status clean"));
}

#[test]
fn sync_stops_after_pull_failure_without_applying_changes() {
    let workspace = workspace();
    let origin = workspace.join("origin.git");
    let seed = workspace.join("seed");
    let project = workspace.join("project");
    let live = workspace.join("live");

    fs::create_dir_all(&seed).expect("create seed dir");
    fs::create_dir_all(&live).expect("create live dir");

    git_ok(&workspace, &["init", "--bare", "origin.git"]);
    git_ok(&seed, &["init"]);
    git_ok(&seed, &["config", "user.name", "Ada"]);
    git_ok(&seed, &["config", "user.email", "ada@example.com"]);

    fs::create_dir_all(seed.join("repo")).expect("create source dir");
    fs::write(seed.join("repo/gitconfig"), "v1\n").expect("write v1 source");
    fs::write(seed.join(".gitignore"), ".state/\n").expect("write gitignore");
    write_manifest_text(
        &seed,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig"
target = "../live/.gitconfig"
mode = "copy"
"#,
    );

    git_ok(&seed, &["add", "."]);
    git_ok(&seed, &["commit", "-m", "initial"]);
    let branch_output = run_git(&seed, &["branch", "--show-current"]);
    assert!(branch_output.status.success(), "{:?}", branch_output);
    let branch = String::from_utf8(branch_output.stdout)
        .expect("branch name")
        .trim()
        .to_string();
    let origin_path = origin.to_str().expect("origin path");
    git_ok(&seed, &["remote", "add", "origin", origin_path]);
    git_ok(&seed, &["push", "-u", "origin", &branch]);

    git_ok(&workspace, &["clone", "origin.git", "project"]);

    fs::write(live.join(".gitconfig"), "existing\n").expect("write existing target");
    fs::remove_dir_all(&origin).expect("remove origin");

    let sync = run_kungfig(&project, &["sync"]);
    assert!(!sync.status.success(), "{:?}", sync);

    assert_eq!(
        fs::read_to_string(project.join("repo/gitconfig")).expect("read local source"),
        "v1\n"
    );
    assert_eq!(
        fs::read_to_string(live.join(".gitconfig")).expect("read unchanged target"),
        "existing\n"
    );

    let stdout = String::from_utf8_lossy(&sync.stdout);
    assert!(!stdout.contains("updated"));
    assert!(!stdout.contains("created"));
}
