use std::collections::{BTreeMap, HashSet};
use std::env;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use directories::UserDirs;
use toml::Value as TomlValue;

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub item_id: String,
    pub live_path: Option<PathBuf>,
    pub store_path: Option<PathBuf>,
    pub mode: FileMode,
    pub merge: Option<String>,
    pub when: Vec<String>,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub tags: Vec<String>,
    pub description: Option<String>,
}

impl Item {
    pub fn matches(&self, selectors: &[String]) -> bool {
        if selectors.is_empty() {
            return true;
        }

        selectors.iter().any(|selector| {
            self.item_id == *selector
                || self.name == *selector
                || self.tags.iter().any(|tag| tag == selector)
        })
    }
}

#[derive(Clone, Debug)]
pub struct Manifest {
    pub path: PathBuf,
    pub items: Vec<Item>,
    pub variables: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct StatusResult {
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct ActionResult {
    pub item: Item,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileMode {
    Copy,
    Symlink,
    Hardlink,
    Merge,
    Script,
}

impl std::fmt::Display for FileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            FileMode::Copy => "copy",
            FileMode::Symlink => "symlink",
            FileMode::Hardlink => "hardlink",
            FileMode::Merge => "merge",
            FileMode::Script => "script",
        };
        write!(f, "{name}")
    }
}

pub fn current_platform() -> &'static str {
    match env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    }
}

pub fn load_manifest(path: &Path) -> Result<Manifest> {
    let manifest_path = expand_home(path);
    if !manifest_path.exists() {
        bail!("Manifest not found: {}", manifest_path.display());
    }
    let manifest_dir = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let text = std::fs::read_to_string(&manifest_path)?;
    let data: TomlValue = text.parse()?;
    let items_data = data
        .get("items")
        .and_then(TomlValue::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| anyhow!("Manifest must contain a non-empty [[items]] list."))?;

    let mut variables = build_variables(manifest_dir)?;
    apply_custom_variables(data.get("vars"), &mut variables)?;

    let platform = current_platform();
    let mut seen_ids = HashSet::new();
    let mut items = Vec::with_capacity(items_data.len());

    for (index, row) in items_data.iter().enumerate() {
        let table = row
            .as_table()
            .ok_or_else(|| anyhow!("items[{}] must be a table.", index + 1))?;

        let name = require_string(table, "name", index + 1)?;
        let item_id = require_string(table, "id", index + 1)?;
        if !seen_ids.insert(item_id.clone()) {
            bail!("Duplicate item id: {item_id}");
        }

        let mode_raw = table
            .get("mode")
            .cloned()
            .unwrap_or_else(|| TomlValue::String("copy".to_string()));
        let resolved_mode = resolve_variant(&mode_raw, platform, "mode", &item_id)?
            .ok_or_else(|| anyhow!("mode is required for {item_id}"))?;
        let mode = parse_mode(&resolved_mode, &item_id)?;

        let merge = match table.get("merge") {
            Some(TomlValue::String(value)) => Some(value.clone()),
            Some(_) => bail!("merge must be a string for {item_id}"),
            None => None,
        };
        if let Some(mode_name) = merge.as_deref() {
            if !matches!(mode_name, "json" | "toml" | "yaml") {
                bail!("Unsupported merge mode for {item_id}: {mode_name}");
            }
        }

        let source_value = table.get("source");
        let live_raw = source_value
            .map(|value| resolve_variant(value, platform, "source", &item_id))
            .transpose()?
            .flatten();
        let store_raw = table
            .get("target")
            .map(|value| resolve_variant(value, platform, "target", &item_id))
            .transpose()?
            .flatten()
            .ok_or_else(|| anyhow!("target is required for {item_id}"))?;

        if live_raw.is_none()
            && source_required_for_platform(source_value, platform, table, &item_id)?
        {
            bail!("source and target are required for {item_id}");
        }

        items.push(Item {
            name,
            item_id: item_id.clone(),
            live_path: live_raw
                .as_deref()
                .map(|value| resolve_path(value, manifest_dir, &variables)),
            store_path: Some(resolve_path(&store_raw, manifest_dir, &variables)),
            mode,
            merge,
            when: parse_list_field(table.get("when"), "when", &item_id)?,
            before: parse_list_field(table.get("before"), "before", &item_id)?,
            after: parse_list_field(table.get("after"), "after", &item_id)?,
            tags: parse_list_field(table.get("tags"), "tags", &item_id)?,
            description: optional_string(table.get("description"), "description", &item_id)?,
        });
    }

    Ok(Manifest {
        path: manifest_path,
        items,
        variables,
    })
}

pub fn select_items<'a>(manifest: &'a Manifest, selectors: &[String]) -> Result<Vec<&'a Item>> {
    let selected: Vec<&Item> = manifest
        .items
        .iter()
        .filter(|item| item.matches(selectors))
        .collect();

    if !selectors.is_empty() && selected.is_empty() {
        bail!("No items matched selectors: {}", selectors.join(", "));
    }

    Ok(selected)
}

pub fn conditions_match(tokens: &[String], action: Option<&str>, platform: Option<&str>) -> bool {
    if tokens.is_empty() {
        return true;
    }

    let normalized: HashSet<String> = tokens
        .iter()
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect();

    if normalized.is_empty()
        || normalized
            .iter()
            .any(|token| matches!(token.as_str(), "all" | "always"))
    {
        return true;
    }

    let platform_tokens: HashSet<&str> = normalized
        .iter()
        .filter_map(|token| match token.as_str() {
            "macos" | "linux" | "windows" => Some(token.as_str()),
            _ => None,
        })
        .collect();
    if !platform_tokens.is_empty()
        && !platform.is_some_and(|candidate| platform_tokens.contains(candidate))
    {
        return false;
    }

    let action_tokens: HashSet<&str> = normalized
        .iter()
        .filter_map(|token| match token.as_str() {
            "pull" | "push" => Some(token.as_str()),
            _ => None,
        })
        .collect();
    if !action_tokens.is_empty()
        && !action.is_some_and(|candidate| action_tokens.contains(candidate))
    {
        return false;
    }

    normalized.iter().all(|token| {
        matches!(
            token.as_str(),
            "all" | "always" | "macos" | "linux" | "windows" | "pull" | "push"
        )
    })
}

fn parse_mode(value: &str, item_id: &str) -> Result<FileMode> {
    match value {
        "copy" => Ok(FileMode::Copy),
        "symlink" => Ok(FileMode::Symlink),
        "hardlink" => Ok(FileMode::Hardlink),
        "merge" => Ok(FileMode::Merge),
        "script" => Ok(FileMode::Script),
        other => bail!("Unsupported mode for {item_id}: {other}"),
    }
}

fn build_variables(backup_root: &Path) -> Result<BTreeMap<String, String>> {
    let home = home_dir()?;
    let platform = current_platform();

    let (config, localconfig, appdata, cache) = match platform {
        "macos" => (
            home.join(".config"),
            home.join(".local").join("share"),
            home.join("Library").join("Application Support"),
            home.join("Library").join("Caches"),
        ),
        "windows" => {
            let appdata =
                env_path("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming"));
            let localconfig =
                env_path("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local"));
            let cache = localconfig.join("Cache");
            (appdata.clone(), localconfig, appdata, cache)
        }
        _ => (
            env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            env_path("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share")),
            env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            env_path("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache")),
        ),
    };

    let documents = user_dirs_path(|dirs| dirs.document_dir(), home.join("Documents"));
    let downloads = user_dirs_path(|dirs| dirs.download_dir(), home.join("Downloads"));
    let desktop = user_dirs_path(|dirs| dirs.desktop_dir(), home.join("Desktop"));

    Ok(BTreeMap::from([
        ("backup".to_string(), backup_root.display().to_string()),
        ("home".to_string(), home.display().to_string()),
        ("cache".to_string(), cache.display().to_string()),
        ("config".to_string(), config.display().to_string()),
        ("localconfig".to_string(), localconfig.display().to_string()),
        ("appdata".to_string(), appdata.display().to_string()),
        ("documents".to_string(), documents.display().to_string()),
        ("downloads".to_string(), downloads.display().to_string()),
        ("desktop".to_string(), desktop.display().to_string()),
    ]))
}

fn apply_custom_variables(
    raw_vars: Option<&TomlValue>,
    builtin_variables: &mut BTreeMap<String, String>,
) -> Result<()> {
    let Some(raw_vars) = raw_vars else {
        return Ok(());
    };
    let table = raw_vars
        .as_table()
        .ok_or_else(|| anyhow!("Top-level [vars] must be a table."))?;

    for (key, value) in table {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("All [vars] entries must be string = string pairs."))?;
        let expanded = expand_variables(value, builtin_variables);
        builtin_variables.insert(key.clone(), expanded);
    }

    Ok(())
}

fn resolve_variant(
    value: &TomlValue,
    platform: &str,
    field_name: &str,
    item_id: &str,
) -> Result<Option<String>> {
    if let Some(value) = value.as_str() {
        return Ok(Some(value.to_string()));
    }

    if let Some(table) = value.as_table() {
        for key in [platform, "default", "common", "all"] {
            if let Some(value) = table.get(key).and_then(TomlValue::as_str) {
                return Ok(Some(value.to_string()));
            }
        }
        return Ok(None);
    }

    bail!("{field_name} must be a string or table for {item_id}")
}

fn source_required_for_platform(
    source_value: Option<&TomlValue>,
    platform: &str,
    row: &toml::value::Table,
    item_id: &str,
) -> Result<bool> {
    if !platform_matches_item(row, platform, item_id)? {
        return Ok(false);
    }

    let Some(source_value) = source_value else {
        return Ok(true);
    };

    if source_value.is_str() {
        return Ok(true);
    }

    if let Some(table) = source_value.as_table() {
        for key in [platform, "default", "common", "all"] {
            if table.get(key).and_then(TomlValue::as_str).is_some() {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    Ok(true)
}

fn platform_matches_item(row: &toml::value::Table, platform: &str, item_id: &str) -> Result<bool> {
    let Some(when) = row.get("when") else {
        return Ok(true);
    };
    let tokens = parse_list_field(Some(when), "when", item_id)?;
    Ok(conditions_match(&tokens, None, Some(platform)))
}

fn require_string(table: &toml::value::Table, key: &str, index: usize) -> Result<String> {
    let value = table
        .get(key)
        .and_then(TomlValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("items[{index}].{key} must be a non-empty string."))?;
    Ok(value.to_string())
}

fn optional_string(
    value: Option<&TomlValue>,
    field_name: &str,
    item_id: &str,
) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(TomlValue::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{field_name} must be a string for {item_id}"),
    }
}

fn parse_list_field(
    value: Option<&TomlValue>,
    field_name: &str,
    item_id: &str,
) -> Result<Vec<String>> {
    match value {
        None => Ok(Vec::new()),
        Some(TomlValue::String(value)) => Ok(vec![value.clone()]),
        Some(TomlValue::Array(values)) => values
            .iter()
            .map(|value| {
                value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    anyhow!("{field_name} must be a string or list of strings for {item_id}")
                })
            })
            .collect(),
        Some(_) => bail!("{field_name} must be a string or list of strings for {item_id}"),
    }
}

fn resolve_path(value: &str, base_dir: &Path, variables: &BTreeMap<String, String>) -> PathBuf {
    let expanded = expand_variables(value, variables);
    let path = PathBuf::from(expand_tilde(&expanded));
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(base_dir.join(path))
    }
}

fn expand_variables(value: &str, variables: &BTreeMap<String, String>) -> String {
    let mut result = value.to_string();
    for (key, replacement) in variables {
        result = result.replace(&format!("{{{key}}}"), replacement);
    }
    result
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn expand_home(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    PathBuf::from(expand_tilde(&raw))
}

fn expand_tilde(value: &str) -> String {
    if value == "~" {
        return home_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| value.to_string());
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    value.to_string()
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .ok_or_else(|| anyhow!("Could not determine the home directory"))
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key).map(PathBuf::from)
}

fn user_dirs_path<F>(selector: F, fallback: PathBuf) -> PathBuf
where
    F: Fn(&UserDirs) -> Option<&Path>,
{
    UserDirs::new()
        .and_then(|dirs| selector(&dirs).map(Path::to_path_buf))
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::conditions_match;

    #[test]
    fn requires_platform_and_action_when_both_present() {
        assert!(conditions_match(
            &["macos".to_string(), "push".to_string()],
            Some("push"),
            Some("macos")
        ));
        assert!(!conditions_match(
            &["macos".to_string(), "push".to_string()],
            Some("pull"),
            Some("macos")
        ));
        assert!(!conditions_match(
            &["macos".to_string(), "push".to_string()],
            Some("push"),
            Some("windows")
        ));
    }
}
