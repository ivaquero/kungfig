use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::plan::resolve_items;

#[derive(Debug, Clone)]
pub struct EditResult {
    pub editor: String,
    pub target: PathBuf,
}

pub fn edit_target(
    config: &Config,
    manifest_path: &Path,
    item_name: Option<&str>,
) -> Result<EditResult> {
    let editor = resolve_editor()?;
    let target = select_edit_target(config, manifest_path, item_name, &editor)?;

    let mut command = Command::new(&editor[0]);
    if editor.len() > 1 {
        command.args(&editor[1..]);
    }
    command.arg(&target);

    let status = command
        .status()
        .with_context(|| format!("failed to launch editor {}", editor[0]))?;
    if !status.success() {
        bail!("editor exited with status {status}");
    }

    Ok(EditResult {
        editor: editor[0].clone(),
        target,
    })
}

fn select_edit_target(
    config: &Config,
    manifest_path: &Path,
    item_name: Option<&str>,
    editor: &[String],
) -> Result<PathBuf> {
    if let Some(name) = item_name {
        let items = resolve_items(config, manifest_path)?;
        let item = items
            .into_iter()
            .find(|item| item.name == name)
            .ok_or_else(|| anyhow::anyhow!("item `{name}` not found"))?;
        return Ok(item.source);
    }

    let repo_root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    if repo_root.is_dir() && editor_prefers_files(editor) {
        Ok(manifest_path.to_path_buf())
    } else {
        Ok(repo_root)
    }
}

fn resolve_editor() -> Result<Vec<String>> {
    for value in [env::var("VISUAL").ok(), env::var("EDITOR").ok()]
        .into_iter()
        .flatten()
    {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        let parsed = shell_words::split(value)
            .with_context(|| format!("failed to parse editor command `{value}`"))?;
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    }

    #[cfg(windows)]
    {
        return Ok(vec!["notepad".to_string()]);
    }

    #[cfg(not(windows))]
    {
        Ok(vec!["vi".to_string()])
    }
}

fn editor_prefers_files(editor: &[String]) -> bool {
    editor
        .first()
        .map(|cmd| {
            let lower = cmd.to_ascii_lowercase();
            lower.ends_with("/vi")
                || lower.ends_with("/vim")
                || lower.ends_with("/nvim")
                || lower == "vi"
                || lower == "vim"
                || lower == "nvim"
        })
        .unwrap_or(false)
}
