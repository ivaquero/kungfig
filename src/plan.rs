use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::{Config, Mode};
use crate::path::{expand_path_from, path_exists, same_content, symlink_points_to};
use crate::state::{ChangeState, StateStore, detect_change_state};

#[derive(Debug, Clone)]
pub struct ResolvedItem {
    pub name: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub mode: Mode,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone)]
pub enum Action {
    Create {
        name: String,
        source: PathBuf,
        target: PathBuf,
        mode: Mode,
    },
    Update {
        name: String,
        source: PathBuf,
        target: PathBuf,
        mode: Mode,
        change_state: ChangeState,
    },
    Skip {
        name: String,
        reason: String,
    },
    Conflict {
        name: String,
        target: PathBuf,
        reason: String,
    },
}

pub fn resolve_items(config: &Config, manifest_path: &Path) -> Result<Vec<ResolvedItem>> {
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut items = Vec::with_capacity(config.items.len());

    for item in &config.items {
        let source = expand_path_from(manifest_dir, &item.source)?;
        let target = expand_path_from(manifest_dir, item.target.for_current_platform(&item.name)?)?;
        items.push(ResolvedItem {
            name: item.name.clone(),
            source,
            target,
            mode: item.mode,
        });
    }

    Ok(items)
}

pub fn build_plan(
    config: &Config,
    manifest_path: &Path,
    store: Option<&StateStore>,
) -> Result<Plan> {
    let resolved_items = resolve_items(config, manifest_path)?;
    let mut actions = Vec::with_capacity(resolved_items.len());

    for item in resolved_items {
        if !path_exists(&item.source) {
            actions.push(Action::Skip {
                name: item.name,
                reason: "source path is missing".to_string(),
            });
            continue;
        }

        if !path_exists(&item.target) {
            actions.push(Action::Create {
                name: item.name,
                source: item.source,
                target: item.target,
                mode: item.mode,
            });
            continue;
        }

        if types_conflict(&item.source, &item.target)? {
            actions.push(Action::Conflict {
                name: item.name,
                target: item.target,
                reason: "source and target types differ".to_string(),
            });
            continue;
        }

        if matches_desired_state(&item)? {
            actions.push(Action::Skip {
                name: item.name,
                reason: "already up to date".to_string(),
            });
            continue;
        }

        if let Some(store) = store {
            let change_state = detect_change_state(&item, store)?;
            if change_state == ChangeState::Conflict {
                actions.push(Action::Conflict {
                    name: item.name,
                    target: item.target,
                    reason: "source and target both changed since last apply".to_string(),
                });
                continue;
            }

            actions.push(Action::Update {
                name: item.name,
                source: item.source,
                target: item.target,
                mode: item.mode,
                change_state,
            });
            continue;
        }

        actions.push(Action::Update {
            name: item.name,
            source: item.source,
            target: item.target,
            mode: item.mode,
            change_state: ChangeState::Untracked,
        });
    }

    Ok(Plan { actions })
}

fn matches_desired_state(item: &ResolvedItem) -> Result<bool> {
    match item.mode {
        Mode::Copy => same_content(&item.source, &item.target),
        Mode::Symlink => symlink_points_to(&item.target, &item.source),
    }
}

fn types_conflict(source: &Path, target: &Path) -> Result<bool> {
    let source_meta = fs::symlink_metadata(source)?;
    let target_meta = fs::symlink_metadata(target)?;

    if target_meta.file_type().is_symlink() {
        return Ok(false);
    }

    Ok(source_meta.is_dir() != target_meta.is_dir())
}
