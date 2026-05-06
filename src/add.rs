use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use toml::Value;

use crate::config::{Config, Item, Mode, Target};
use crate::path::{compact_path_for_manifest, copy_path, expand_path_from, path_exists};
use crate::plan::ResolvedItem;
use crate::state::StateStore;

#[derive(Debug, Clone)]
pub struct AddResult {
    pub name: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub manifest: PathBuf,
}

pub fn add_item(
    manifest_path: &Path,
    config: &Config,
    target_input: &str,
    name: &str,
    store: &StateStore,
) -> Result<AddResult> {
    let name = name.trim();
    if name.is_empty() {
        bail!("item name cannot be empty");
    }
    if config.contains_identifier(name) {
        bail!("item `{name}` already exists");
    }

    let target = expand_path_from(&env::current_dir()?, target_input)?;
    if !path_exists(&target) {
        bail!("target path does not exist: {}", target.display());
    }

    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let source_rel = default_source_path(name, &target);
    let source = manifest_dir.join(&source_rel);

    if path_exists(&source) {
        bail!(
            "destination source path already exists: {}",
            source.display()
        );
    }

    copy_path(&target, &source)?;
    let target_value = manifest_target_value(target_input, &target)?;
    let item = Item {
        name: name.to_string(),
        alias: None,
        source: source_rel,
        target: Target::Single(target_value),
        mode: Mode::Copy,
        tags: Vec::new(),
        when: None,
    };

    if let Err(err) = append_manifest_items(manifest_path, &[item]) {
        let _ = remove_if_exists(&source);
        return Err(err);
    }

    let resolved = ResolvedItem {
        name: name.to_string(),
        source: source.clone(),
        target: target.clone(),
        mode: Mode::Copy,
    };

    if let Err(err) = store.record_managed(&resolved) {
        let _ = rollback_manifest_item(manifest_path);
        let _ = remove_if_exists(&source);
        return Err(err);
    }

    Ok(AddResult {
        name: name.to_string(),
        source,
        target,
        manifest: manifest_path.to_path_buf(),
    })
}

pub(crate) fn append_manifest_items(manifest_path: &Path, items: &[Item]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = if manifest_path.exists() {
        fs::read_to_string(manifest_path)?
    } else {
        String::new()
    };
    let mut updated = existing.trim_end().to_string();
    for item in items {
        if !updated.is_empty() {
            updated.push_str("\n\n");
        }
        updated.push_str(&render_item_block(item));
    }
    updated.push(10 as char);

    toml::from_str::<Config>(&updated)?;
    fs::write(manifest_path, updated)?;
    Ok(())
}

fn rollback_manifest_item(manifest_path: &Path) -> Result<()> {
    let existing = fs::read_to_string(manifest_path)?;
    let trimmed = existing.trim_end();
    if let Some(index) = trimmed.rfind("\n\n[[items]]") {
        let mut restored = trimmed[..index].to_string();
        restored.push(10 as char);
        fs::write(manifest_path, restored)?;
    } else if trimmed.starts_with("[[items]]") {
        fs::write(manifest_path, "")?;
    }
    Ok(())
}

fn render_item_block(item: &Item) -> String {
    let mut lines = vec![
        "[[items]]".to_string(),
        format!("name = {}", Value::String(item.name.clone())),
    ];

    if let Some(alias) = &item.alias {
        lines.push(format!("alias = {}", Value::String(alias.clone())));
    }

    lines.push(format!(
        "source = {}",
        Value::String(item.source.replace(std::path::MAIN_SEPARATOR, "/"))
    ));

    match &item.target {
        Target::Single(path) => {
            lines.push(format!("target = {}", Value::String(path.clone())));
        }
        Target::Platform {
            macos,
            linux,
            windows,
        } => {
            if let Some(path) = macos {
                lines.push(format!("target.macos = {}", Value::String(path.clone())));
            }
            if let Some(path) = linux {
                lines.push(format!("target.linux = {}", Value::String(path.clone())));
            }
            if let Some(path) = windows {
                lines.push(format!("target.windows = {}", Value::String(path.clone())));
            }
        }
    }

    lines.push(format!("mode = {}", Value::String(item.mode.to_string())));

    if !item.tags.is_empty() {
        lines.push(format!(
            "tags = {}",
            Value::Array(item.tags.iter().cloned().map(Value::String).collect())
        ));
    }

    if let Some(expr) = &item.when {
        lines.push(format!("when = {}", Value::String(expr.clone())));
    }

    lines.join("\n")
}

fn manifest_target_value(target_input: &str, target: &Path) -> Result<String> {
    let input_path = Path::new(target_input);
    if input_path.is_relative() && !target_input.starts_with("~/") && target_input != "~" {
        return Ok(target_input.replace(std::path::MAIN_SEPARATOR, "/"));
    }

    compact_path_for_manifest(target)
}

fn default_source_path(name: &str, target: &Path) -> String {
    if target.is_dir() {
        return format!("dotfiles/{name}");
    }

    let ext = target
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();

    if ext.is_empty() || name.ends_with(&ext) {
        format!("dotfiles/{name}")
    } else {
        format!("dotfiles/{name}{ext}")
    }
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if !path_exists(path) {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}
