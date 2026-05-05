use std::process::Command;

fn run_kungfig(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kungfig"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run kungfig binary")
}

#[test]
fn list_runs_against_repo_manifest() {
    let output = run_kungfig(&["list"]);
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("manifest: kungfig.toml"));
    assert!(stdout.contains("vscode"));
}

#[test]
fn show_reports_item_status() {
    let output = run_kungfig(&["show", "wezterm"]);
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id:          wezterm"));
    assert!(stdout.contains("status:"));
}

#[test]
fn status_returns_non_zero_for_missing_item() {
    let output = run_kungfig(&["status", "wezterm"]);
    assert_eq!(output.status.code(), Some(1), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("missing"));
    assert!(stdout.contains("wezterm"));
}
