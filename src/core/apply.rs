use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow, bail};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml::Value as TomlValue;
use walkdir::WalkDir;

use super::engine::{FileMode, Item, current_platform};

pub(crate) fn apply_item(
    manifest_path: &Path,
    item: &Item,
    action: &str,
    dry_run: bool,
) -> Result<()> {
    let live_path = item.live_path.as_ref().ok_or_else(|| {
        anyhow!(
            "{} is not configured for {}",
            item.item_id,
            current_platform()
        )
    })?;
    let store_path = item.store_path.as_ref().ok_or_else(|| {
        anyhow!(
            "{} is not configured for {}",
            item.item_id,
            current_platform()
        )
    })?;

    run_hooks(manifest_path, item, &item.before, action, dry_run)?;

    match action {
        "pull" => pull_item(manifest_path, item, live_path, store_path, dry_run)?,
        "push" => push_item(manifest_path, item, live_path, store_path, dry_run)?,
        other => bail!("Unsupported action: {other}"),
    }

    run_hooks(manifest_path, item, &item.after, action, dry_run)
}

fn pull_item(
    manifest_path: &Path,
    item: &Item,
    live_path: &Path,
    store_path: &Path,
    dry_run: bool,
) -> Result<()> {
    match item.mode {
        FileMode::Copy | FileMode::Symlink | FileMode::Hardlink => {
            copy_path(live_path, store_path, dry_run)
        }
        FileMode::Merge => merge_path(live_path, store_path, item.merge.as_deref(), dry_run),
        FileMode::Script => run_script(manifest_path, item, "pull", dry_run),
    }
}

fn push_item(
    manifest_path: &Path,
    item: &Item,
    live_path: &Path,
    store_path: &Path,
    dry_run: bool,
) -> Result<()> {
    match item.mode {
        FileMode::Copy => copy_path(store_path, live_path, dry_run),
        FileMode::Symlink => create_symlink(live_path, store_path, dry_run),
        FileMode::Hardlink => create_hardlink(live_path, store_path, dry_run),
        FileMode::Merge => merge_path(store_path, live_path, item.merge.as_deref(), dry_run),
        FileMode::Script => run_script(manifest_path, item, "push", dry_run),
    }
}

fn run_hooks(
    manifest_path: &Path,
    item: &Item,
    hooks: &[String],
    action: &str,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        return Ok(());
    }

    let env_vars = build_env(manifest_path, item, action);
    for hook in hooks {
        run_shell(hook, &env_vars)?;
    }

    Ok(())
}

fn run_script(manifest_path: &Path, item: &Item, action: &str, dry_run: bool) -> Result<()> {
    let script_path = item
        .store_path
        .as_ref()
        .ok_or_else(|| anyhow!("{} is missing a store path", item.item_id))?;
    if !script_path.exists() {
        bail!("Script not found: {}", script_path.display());
    }
    if dry_run {
        return Ok(());
    }

    let mut command = if script_path.extension() == Some(OsStr::new("py")) {
        let interpreter = if cfg!(windows) { "python" } else { "python3" };
        let mut command = Command::new(interpreter);
        command.arg(script_path);
        command
    } else if matches!(
        script_path.extension().and_then(OsStr::to_str),
        Some("sh" | "bash")
    ) {
        let mut command = Command::new("/bin/sh");
        command.arg(script_path);
        command
    } else {
        Command::new(script_path)
    };

    command
        .arg(action)
        .arg(
            item.live_path
                .as_ref()
                .map(PathBuf::as_path)
                .unwrap_or_else(|| Path::new("")),
        )
        .arg(script_path);
    command.envs(build_env(manifest_path, item, action));

    let status = command.status()?;
    if !status.success() {
        bail!("Script failed: {}", script_path.display());
    }

    Ok(())
}

fn build_env(manifest_path: &Path, item: &Item, action: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("KUNGFIG_ACTION".to_string(), action.to_string()),
        ("KUNGFIG_ITEM_ID".to_string(), item.item_id.clone()),
        ("KUNGFIG_ITEM_NAME".to_string(), item.name.clone()),
        (
            "KUNGFIG_LIVE_PATH".to_string(),
            item.live_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ),
        (
            "KUNGFIG_STORE_PATH".to_string(),
            item.store_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        ),
        (
            "KUNGFIG_MANIFEST_PATH".to_string(),
            manifest_path.display().to_string(),
        ),
    ])
}

fn copy_path(source: &Path, target: &Path, dry_run: bool) -> Result<()> {
    if !path_exists(source) {
        bail!("Source path not found: {}", source.display());
    }
    if dry_run {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_existing(target)?;

    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        copy_dir(source, target)?;
    } else {
        fs::copy(source, target)?;
    }

    Ok(())
}

fn create_symlink(link_path: &Path, target_path: &Path, dry_run: bool) -> Result<()> {
    if !path_exists(target_path) {
        bail!("Store path not found: {}", target_path.display());
    }
    if dry_run {
        return Ok(());
    }

    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_existing(link_path)?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(target_path, link_path)?;

    #[cfg(windows)]
    {
        if target_path.is_dir() {
            std::os::windows::fs::symlink_dir(target_path, link_path)?;
        } else {
            std::os::windows::fs::symlink_file(target_path, link_path)?;
        }
    }

    Ok(())
}

fn create_hardlink(link_path: &Path, target_path: &Path, dry_run: bool) -> Result<()> {
    if !path_exists(target_path) {
        bail!("Store path not found: {}", target_path.display());
    }
    if target_path.is_dir() {
        bail!("Hardlinks are only supported for files.");
    }
    if dry_run {
        return Ok(());
    }

    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_existing(link_path)?;
    fs::hard_link(target_path, link_path)?;
    Ok(())
}

fn merge_path(source: &Path, target: &Path, merge_mode: Option<&str>, dry_run: bool) -> Result<()> {
    let merge_mode = merge_mode.ok_or_else(|| anyhow!("merge mode requires a merge format."))?;
    if !path_exists(source) {
        bail!("Source path not found: {}", source.display());
    }
    if source.is_dir() {
        bail!("merge mode only supports files.");
    }
    if dry_run {
        return Ok(());
    }

    let source_data = read_structured(source, merge_mode)?;
    let target_data = if path_exists(target) {
        read_structured(target, merge_mode)?
    } else {
        JsonValue::Object(JsonMap::new())
    };
    let merged = deep_merge(target_data, source_data);

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    remove_existing(target)?;
    write_structured(target, merge_mode, &merged)
}

fn read_structured(path: &Path, merge_mode: &str) -> Result<JsonValue> {
    let text = fs::read_to_string(path)?;
    let value = match merge_mode {
        "json" => serde_json::from_str(&text)?,
        "toml" => {
            let value: TomlValue = text.parse()?;
            serde_json::to_value(value)?
        }
        "yaml" => serde_yaml::from_str(&text)?,
        other => bail!("Unsupported merge mode: {other}"),
    };
    Ok(normalize_structured_value(value))
}

fn write_structured(path: &Path, merge_mode: &str, value: &JsonValue) -> Result<()> {
    let text = match merge_mode {
        "json" => format!("{}\n", serde_json::to_string_pretty(value)?),
        "toml" => {
            let toml_value: TomlValue = serde_json::from_value(value.clone())?;
            format!("{}\n", toml::to_string_pretty(&toml_value)?.trim_end())
        }
        "yaml" => serde_yaml::to_string(value)?,
        other => bail!("Unsupported merge mode: {other}"),
    };
    fs::write(path, text)?;
    Ok(())
}

fn deep_merge(base: JsonValue, override_value: JsonValue) -> JsonValue {
    match (base, override_value) {
        (JsonValue::Object(mut base), JsonValue::Object(override_map)) => {
            for (key, value) in override_map {
                let existing = base.remove(&key).unwrap_or(JsonValue::Null);
                base.insert(key, deep_merge(existing, value));
            }
            JsonValue::Object(base)
        }
        (_, override_value) => override_value,
    }
}

fn normalize_structured_value(value: JsonValue) -> JsonValue {
    if value.is_null() {
        JsonValue::Object(JsonMap::new())
    } else {
        value
    }
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_existing(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };

    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }

    Ok(())
}

fn copy_dir(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let source_path = entry.path();
        if source_path == source {
            continue;
        }

        let relative = source_path.strip_prefix(source)?;
        let target_path = target.join(relative);
        let metadata = fs::symlink_metadata(source_path)?;

        if metadata.is_dir() {
            fs::create_dir_all(&target_path)?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if metadata.file_type().is_symlink() {
            let linked = fs::read_link(source_path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(linked, &target_path)?;
            #[cfg(windows)]
            {
                if source_path.is_dir() {
                    std::os::windows::fs::symlink_dir(linked, &target_path)?;
                } else {
                    std::os::windows::fs::symlink_file(linked, &target_path)?;
                }
            }
        } else {
            fs::copy(source_path, &target_path)?;
        }
    }
    Ok(())
}

fn run_shell(command: &str, env_vars: &BTreeMap<String, String>) -> Result<()> {
    #[cfg(windows)]
    let mut child = {
        let mut child = Command::new("cmd");
        child.arg("/C").arg(command);
        child
    };

    #[cfg(not(windows))]
    let mut child = {
        let mut child = Command::new("/bin/sh");
        child.arg("-c").arg(command);
        child
    };

    child.envs(env_vars);
    let status = child.status()?;
    if !status.success() {
        bail!("Hook failed: {command}");
    }
    Ok(())
}
