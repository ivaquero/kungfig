mod common;

use std::fs;

use common::{run_kungfig, workspace, write_manifest, write_manifest_text};

#[test]
fn recipe_list_lists_available_recipes() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("recipes")).expect("create recipes dir");
    fs::write(
        workspace.join("recipes/vscode.toml"),
        r#"
name = "vscode"

[[items]]
name = "vscode-settings"
source = "apps/vscode/settings.json"
target.macos = "{home}/Library/Application Support/Code/User/settings.json"
target.windows = "{appdata}/Code/User/settings.json"
mode = "copy"
"#,
    )
    .expect("write vscode recipe");
    fs::write(
        workspace.join("recipes/wezterm.toml"),
        r#"
name = "wezterm"

[[items]]
name = "wezterm-config"
source = "apps/wezterm/wezterm.lua"
target = "{config}/wezterm/wezterm.lua"
mode = "copy"
"#,
    )
    .expect("write wezterm recipe");

    let output = run_kungfig(&workspace, &["recipe", "list"]);
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("vscode"));
    assert!(stdout.contains("wezterm"));
}

#[test]
fn recipe_show_prints_recipe_contents() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("recipes")).expect("create recipes dir");
    fs::write(
        workspace.join("recipes/vscode.toml"),
        r#"
name = "vscode"

[[items]]
name = "vscode-settings"
source = "apps/vscode/settings.json"
target.macos = "{home}/Library/Application Support/Code/User/settings.json"
target.windows = "{appdata}/Code/User/settings.json"
mode = "copy"
"#,
    )
    .expect("write recipe");

    let output = run_kungfig(&workspace, &["recipe", "show", "vscode"]);
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("name = \"vscode\""));
    assert!(stdout.contains("name = \"vscode-settings\""));
}

#[test]
fn add_app_appends_recipe_items_to_manifest() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("recipes")).expect("create recipes dir");
    fs::write(
        workspace.join("recipes/vscode.toml"),
        r#"
name = "vscode"

[[items]]
name = "vscode-settings"
source = "apps/vscode/settings.json"
target.macos = "{home}/Library/Application Support/Code/User/settings.json"
target.windows = "{appdata}/Code/User/settings.json"
mode = "copy"
tags = ["gui"]
when = "os != 'linux'"

[[items]]
name = "vscode-keybindings"
source = "apps/vscode/keybindings.json"
target.macos = "{home}/Library/Application Support/Code/User/keybindings.json"
target.windows = "{appdata}/Code/User/keybindings.json"
mode = "copy"
"#,
    )
    .expect("write recipe");
    write_manifest_text(&workspace, "");

    let output = run_kungfig(&workspace, &["add-app", "vscode"]);
    assert!(output.status.success(), "{:?}", output);

    let manifest = fs::read_to_string(workspace.join("kungfig.toml")).expect("read manifest");
    assert!(manifest.contains("name = \"vscode-settings\""));
    assert!(manifest.contains("name = \"vscode-keybindings\""));
    assert!(
        manifest.contains(
            "target.macos = \"{home}/Library/Application Support/Code/User/settings.json\""
        )
    );
    assert!(manifest.contains("tags = [\"gui\"]"));
    assert!(manifest.contains("when = \"os != 'linux'\""));
}

#[test]
fn add_app_rejects_duplicate_item_names() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("recipes")).expect("create recipes dir");
    fs::write(
        workspace.join("recipes/vscode.toml"),
        r#"
name = "vscode"

[[items]]
name = "gitconfig"
source = "apps/vscode/settings.json"
target = "live/.gitconfig"
mode = "copy"
"#,
    )
    .expect("write recipe");
    write_manifest(&workspace);

    let output = run_kungfig(&workspace, &["add-app", "vscode"]);
    assert!(!output.status.success(), "{:?}", output);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("item `gitconfig` already exists"));
}
