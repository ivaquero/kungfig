mod common;

use common::{run_kungfig_with_string_env, workspace};

#[test]
fn tui_command_launches_in_test_mode() {
    let workspace = workspace();

    let output = run_kungfig_with_string_env(
        &workspace,
        &["tui", "plan"],
        &[("KUNGFIG_TUI_TEST_MODE", "1")],
    );

    assert!(output.status.success(), "{:?}", output);
}
