use anyhow::Result;

use super::apply::apply_item;
use super::engine::{
    ActionResult, Item, Manifest, conditions_match, current_platform, select_items,
};

pub fn run_action(
    manifest: &Manifest,
    action: &str,
    selectors: &[String],
    dry_run: bool,
) -> Result<Vec<ActionResult>> {
    let items = select_items(manifest, selectors)?;
    let mut results = Vec::with_capacity(items.len());

    for item in items {
        if !should_apply(item, action) {
            results.push(ActionResult {
                item: item.clone(),
                status: "skipped".to_string(),
                detail: "when filter skipped".to_string(),
            });
            continue;
        }

        match apply_item(&manifest.path, item, action, dry_run) {
            Ok(()) => results.push(ActionResult {
                item: item.clone(),
                status: action.to_string(),
                detail: if dry_run { "planned" } else { "done" }.to_string(),
            }),
            Err(err) => results.push(ActionResult {
                item: item.clone(),
                status: "error".to_string(),
                detail: err.to_string(),
            }),
        }
    }

    Ok(results)
}

fn should_apply(item: &Item, action: &str) -> bool {
    if item.when.is_empty() {
        return true;
    }

    conditions_match(&item.when, Some(action), Some(current_platform()))
}
