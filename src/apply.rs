use anyhow::Result;

use crate::backup::backup_target;
use crate::config::Mode;
use crate::path::{copy_path, create_symlink, remove_path};
use crate::plan::{Action, Plan, ResolvedItem};
use crate::state::ChangeState;
use crate::state::StateStore;
use crate::template::render_template_source;

#[derive(Debug, Clone)]
pub struct OperationResult {
    pub name: String,
    pub status: String,
    pub detail: String,
}

pub fn apply_plan(plan: &Plan, store: &StateStore) -> Result<Vec<OperationResult>> {
    let mut results = Vec::with_capacity(plan.actions.len());

    for action in &plan.actions {
        match action {
            Action::Create {
                name,
                source,
                target,
                mode,
            } => match materialize(name, source, target, *mode, false, store) {
                Ok(()) => results.push(OperationResult {
                    name: name.clone(),
                    status: "created".to_string(),
                    detail: target.display().to_string(),
                }),
                Err(err) => results.push(OperationResult {
                    name: name.clone(),
                    status: "error".to_string(),
                    detail: err.to_string(),
                }),
            },
            Action::Update {
                name,
                source,
                target,
                mode,
                ..
            } => match materialize(name, source, target, *mode, true, store) {
                Ok(()) => results.push(OperationResult {
                    name: name.clone(),
                    status: "updated".to_string(),
                    detail: target.display().to_string(),
                }),
                Err(err) => results.push(OperationResult {
                    name: name.clone(),
                    status: "error".to_string(),
                    detail: err.to_string(),
                }),
            },
            Action::Skip { name, reason } => results.push(OperationResult {
                name: name.clone(),
                status: "skipped".to_string(),
                detail: reason.clone(),
            }),
            Action::Conflict {
                name,
                target,
                reason,
            } => results.push(OperationResult {
                name: name.clone(),
                status: "conflict".to_string(),
                detail: format!("{} ({reason})", target.display()),
            }),
        }
    }

    Ok(results)
}

pub fn preview_plan(plan: &Plan) -> Vec<OperationResult> {
    let mut results = Vec::with_capacity(plan.actions.len());

    for action in &plan.actions {
        match action {
            Action::Create { name, target, .. } => results.push(OperationResult {
                name: name.clone(),
                status: "would-create".to_string(),
                detail: target.display().to_string(),
            }),
            Action::Update {
                name,
                target,
                change_state,
                ..
            } => results.push(OperationResult {
                name: name.clone(),
                status: "would-update".to_string(),
                detail: format!(
                    "{} (target: {}; backup: {})",
                    target.display(),
                    describe_change_state(*change_state),
                    target.display()
                ),
            }),
            Action::Skip { name, reason } => results.push(OperationResult {
                name: name.clone(),
                status: "skipped".to_string(),
                detail: reason.clone(),
            }),
            Action::Conflict {
                name,
                target,
                reason,
            } => results.push(OperationResult {
                name: name.clone(),
                status: "conflict".to_string(),
                detail: format!("{} ({reason})", target.display()),
            }),
        }
    }

    results
}

pub fn rollback_items(items: &[ResolvedItem], store: &StateStore) -> Result<Vec<OperationResult>> {
    let mut results = Vec::with_capacity(items.len());

    for item in items {
        match store.latest_backup(&item.name, &item.target)? {
            Some(backup) => match restore_item(item, &backup.backup_path, store) {
                Ok(()) => results.push(OperationResult {
                    name: item.name.clone(),
                    status: "restored".to_string(),
                    detail: backup.backup_path.display().to_string(),
                }),
                Err(err) => results.push(OperationResult {
                    name: item.name.clone(),
                    status: "error".to_string(),
                    detail: err.to_string(),
                }),
            },
            None => results.push(OperationResult {
                name: item.name.clone(),
                status: "skipped".to_string(),
                detail: "no backup found".to_string(),
            }),
        }
    }

    Ok(results)
}

fn describe_change_state(change_state: ChangeState) -> &'static str {
    match change_state {
        ChangeState::Clean => "clean",
        ChangeState::Modified => "modified",
        ChangeState::Conflict => "conflict",
        ChangeState::Untracked => "untracked",
    }
}

fn materialize(
    name: &str,
    source: &std::path::Path,
    target: &std::path::Path,
    mode: Mode,
    needs_backup: bool,
    store: &StateStore,
) -> Result<()> {
    if needs_backup {
        if let Some(backup) = backup_target(name, target, store.backup_root())? {
            store.record_backup(name, target, &backup.backup_path, &backup.created_at)?;
        }
    }

    remove_path(target)?;
    match mode {
        Mode::Copy => copy_path(source, target)?,
        Mode::Symlink => create_symlink(target, source)?,
        Mode::Template => {
            let rendered = render_template_source(source)?;
            crate::path::ensure_parent_dir(target)?;
            std::fs::write(target, rendered)?;
        }
    }

    let item = ResolvedItem {
        name: name.to_string(),
        source: source.to_path_buf(),
        target: target.to_path_buf(),
        mode,
    };
    store.record_managed(&item)?;
    Ok(())
}

fn restore_item(
    item: &ResolvedItem,
    backup_path: &std::path::Path,
    store: &StateStore,
) -> Result<()> {
    remove_path(&item.target)?;
    copy_path(backup_path, &item.target)?;
    store.record_managed(item)?;
    Ok(())
}
