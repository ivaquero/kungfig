use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::error::KungfigError;
use crate::path::current_platform;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub name: String,
    pub source: String,
    pub target: Target,
    #[serde(default)]
    pub mode: Mode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Single(String),
    Platform {
        macos: Option<String>,
        linux: Option<String>,
        windows: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Copy,
    Symlink,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Copy => write!(f, "copy"),
            Mode::Symlink => write!(f, "symlink"),
        }
    }
}

impl Target {
    pub fn for_current_platform(&self, item_name: &str) -> Result<&str> {
        match self {
            Target::Single(path) => Ok(path),
            Target::Platform {
                macos,
                linux,
                windows,
            } => {
                let selected = match current_platform() {
                    "macos" => macos.as_deref(),
                    "linux" => linux.as_deref(),
                    "windows" => windows.as_deref(),
                    _ => None,
                };

                selected.ok_or_else(|| {
                    KungfigError::MissingPlatformTarget(item_name.to_string()).into()
                })
            }
        }
    }
}

pub fn load_config(path: &Path) -> Result<(PathBuf, Config)> {
    let manifest_path = normalize_manifest_path(path)?;
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    validate_config(&config)?;
    Ok((manifest_path, config))
}

pub fn load_config_or_empty(path: &Path) -> Result<(PathBuf, Config)> {
    let manifest_path = normalize_manifest_path(path)?;
    if !manifest_path.exists() {
        return Ok((manifest_path, Config::default()));
    }

    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    if text.trim().is_empty() {
        return Ok((manifest_path, Config::default()));
    }

    let config: Config = toml::from_str(&text)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    validate_config(&config)?;
    Ok((manifest_path, config))
}

fn normalize_manifest_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    Ok(env::current_dir()?.join(path))
}

fn validate_config(config: &Config) -> Result<()> {
    if config.items.is_empty() {
        bail!("kungfig.toml must contain at least one [[items]] entry");
    }

    let mut names = HashSet::new();
    for item in &config.items {
        let name = item.name.trim();
        if name.is_empty() {
            bail!("item name cannot be empty");
        }
        if item.source.trim().is_empty() {
            bail!("item `{name}` has an empty source path");
        }
        if !names.insert(name.to_string()) {
            bail!("duplicate item name: {name}");
        }
    }

    Ok(())
}
