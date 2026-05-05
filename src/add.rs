use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use toml::Value;

use crate::config::{Config, Mode};
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
    if config.items.iter().any(|item| item.name == name) {
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

    if let Err(err) = append_manifest_item(manifest_path, name, &source_rel, &target_value) {
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

fn append_manifest_item(
    manifest_path: &Path,
    name: &str,
    source_rel: &str,
    target: &str,
) -> Result<()> {
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = if manifest_path.exists() {
        fs::read_to_string(manifest_path)?
    } else {
        String::new()
    };
    let mut updated = existing.trim_end().to_string();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(&render_item_block(name, source_rel, target));
    updated.push('\n');

    toml::from_str::<Config>(&updated)?;
    fs::write(manifest_path, updated)?;
    Ok(())
}

fn rollback_manifest_item(manifest_path: &Path) -> Result<()> {
    let existing = fs::read_to_string(manifest_path)?;
    let trimmed = existing.trim_end();
    if let Some(index) = trimmed.rfind("\n\n[[items]]") {
        let mut restored = trimmed[..index].to_string();
        restored.push('\n');
        fs::write(manifest_path, restored)?;
    } else if trimmed.starts_with("[[items]]") {
        fs::write(manifest_path, "")?;
    }
    Ok(())
}

fn render_item_block(name: &str, source_rel: &str, target: &str) -> String {
    format!(
        "[[items]]\nname = {}\nsource = {}\ntarget = {}\nmode = \"copy\"",
        Value::String(name.to_string()),
        Value::String(source_rel.replace('\\', "/")),
        Value::String(target.to_string())
    )
}

fn manifest_target_value(target_input: &str, target: &Path) -> Result<String> {
    let input_path = Path::new(target_input);
    if input_path.is_relative() && !target_input.starts_with("~/") && target_input != "~" {
        return Ok(target_input.replace('\\', "/"));
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
