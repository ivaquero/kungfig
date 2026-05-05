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
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub when: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WhenOperator {
    Eq,
    Ne,
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

impl Config {
    pub fn filtered(&self, name: Option<&str>, tag: Option<&str>) -> Result<Self> {
        let mut items = Vec::new();

        for item in &self.items {
            if let Some(name) = name {
                if item.name != name {
                    continue;
                }
            }

            if let Some(tag) = tag {
                if !item.tags.iter().any(|item_tag| item_tag == tag) {
                    continue;
                }
            }

            if !item.matches_current_platform()? {
                continue;
            }

            items.push(item.clone());
        }

        Ok(Self { items })
    }
}

impl Item {
    fn matches_current_platform(&self) -> Result<bool> {
        let Some(expr) = self.when.as_deref() else {
            return Ok(true);
        };

        let (operator, expected_os) = parse_when_expression(expr)?;
        let actual_os = current_platform();

        Ok(match operator {
            WhenOperator::Eq => actual_os == expected_os,
            WhenOperator::Ne => actual_os != expected_os,
        })
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

pub(crate) fn validate_config(config: &Config) -> Result<()> {
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
        for tag in &item.tags {
            if tag.trim().is_empty() {
                bail!("item `{name}` has an empty tag");
            }
        }
        if let Some(expr) = item.when.as_deref() {
            parse_when_expression(expr)?;
        }
    }

    Ok(())
}

fn parse_when_expression(expr: &str) -> Result<(WhenOperator, &str)> {
    let parts: Vec<_> = expr.split_whitespace().collect();
    if parts.len() != 3 {
        bail!("unsupported when expression `{expr}`; expected os == windows");
    }

    if parts[0] != "os" {
        bail!(
            "unsupported when variable `{}`; only `os` is supported",
            parts[0]
        );
    }

    let operator = match parts[1] {
        "==" => WhenOperator::Eq,
        "!=" => WhenOperator::Ne,
        other => bail!("unsupported when operator `{other}`; use `==` or `!=`"),
    };

    let value = parse_string_literal(parts[2])?;
    if !matches!(value, "macos" | "linux" | "windows") {
        bail!("unsupported os value `{value}` in when expression");
    }

    Ok((operator, value))
}

fn parse_string_literal(input: &str) -> Result<&str> {
    if input.len() < 2 {
        bail!("unsupported when expression literal `{input}`");
    }

    let bytes = input.as_bytes();
    let quote = bytes[0];
    if (quote != 39 && quote != 34) || bytes[input.len() - 1] != quote {
        bail!("unsupported when expression literal `{input}`; use single or double quotes");
    }

    Ok(&input[1..input.len() - 1])
}
