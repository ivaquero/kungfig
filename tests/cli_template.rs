mod common;

use std::fs;

use common::{run_kungfig_with_string_env, workspace, write_manifest_text};

#[test]
fn template_mode_renders_builtin_context_and_custom_vars() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(
        workspace.join("repo/gitconfig.tmpl"),
        r#"
[user]
name = "{{ username }}"
email = "{{ vars.email }}"

[system]
os = "{{ os }}"
arch = "{{ arch }}"
host = "{{ hostname }}"
profile = "{{ profile }}"
greeting = "{{ vars.greeting }}"

[core]
autocrlf = {% if os == "windows" %}true{% else %}input{% endif %}
"#,
    )
    .expect("write template source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig.tmpl"
target = "live/.gitconfig"
mode = "template"
"#,
    );

    let apply = run_kungfig_with_string_env(
        &workspace,
        &["apply"],
        &[
            ("USER", "ada"),
            ("HOSTNAME", "workstation"),
            ("KUNGFIG_PROFILE", "dev"),
            ("KUNGFIG_VAR_EMAIL", "ada@example.com"),
            ("KUNGFIG_VAR_GREETING", "hello"),
        ],
    );
    assert!(apply.status.success(), "{:?}", apply);

    let output = fs::read_to_string(workspace.join("live/.gitconfig")).expect("read rendered file");
    assert!(output.contains("name = \"ada\""));
    assert!(output.contains("email = \"ada@example.com\""));
    assert!(output.contains("host = \"workstation\""));
    assert!(output.contains("profile = \"dev\""));
    assert!(output.contains("greeting = \"hello\""));

    let expected_autocrlf = if std::env::consts::OS == "windows" {
        "true"
    } else {
        "input"
    };
    assert!(output.contains(&format!("autocrlf = {expected_autocrlf}")));

    let status = run_kungfig_with_string_env(
        &workspace,
        &["status"],
        &[
            ("USER", "ada"),
            ("HOSTNAME", "workstation"),
            ("KUNGFIG_PROFILE", "dev"),
            ("KUNGFIG_VAR_EMAIL", "ada@example.com"),
            ("KUNGFIG_VAR_GREETING", "hello"),
        ],
    );
    assert!(status.status.success(), "{:?}", status);
    assert!(String::from_utf8_lossy(&status.stdout).contains("synced"));
}

#[test]
fn template_mode_supports_diff_preview() {
    let workspace = workspace();
    fs::create_dir_all(workspace.join("repo")).expect("create repo dir");
    fs::write(
        workspace.join("repo/gitconfig.tmpl"),
        r#"
[user]
name = "{{ username }}"
"#,
    )
    .expect("write template source");
    write_manifest_text(
        &workspace,
        r#"
[[items]]
name = "gitconfig"
source = "repo/gitconfig.tmpl"
target = "live/.gitconfig"
mode = "template"
"#,
    );

    fs::create_dir_all(workspace.join("live")).expect("create live dir");
    fs::write(
        workspace.join("live/.gitconfig"),
        "[user]\nname = \"old\"\n",
    )
    .expect("write target");

    let args = ["diff", "gitconfig"];
    let envs = [("USER", "new-user")];
    let diff = run_kungfig_with_string_env(&workspace, &args, &envs);
    assert!(diff.status.success(), "{:?}", diff);

    let stdout = String::from_utf8_lossy(&diff.stdout);
    assert!(stdout.contains("== gitconfig =="));
    assert!(stdout.contains("-name = \"old\""));
    assert!(stdout.contains("+name = \"new-user\""));
}
