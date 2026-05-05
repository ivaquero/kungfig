use std::fs;

use anyhow::{Result, bail};
use similar::{ChangeTag, TextDiff};

use crate::error::KungfigError;
use crate::path::{path_exists, same_content, symlink_points_to};
use crate::plan::ResolvedItem;
use crate::template::render_template_source;

pub fn diff_item(item: &ResolvedItem) -> Result<String> {
    if !path_exists(&item.source) {
        return Ok("source path is missing".to_string());
    }
    if !path_exists(&item.target) {
        return Ok("target path is missing".to_string());
    }

    if item.source.is_dir() || item.target.is_dir() {
        if same_content(&item.source, &item.target)? {
            return Ok("directories match".to_string());
        }
        return Err(KungfigError::UnsupportedDiff(item.name.clone()).into());
    }

    let source = if item.mode == crate::config::Mode::Template {
        render_template_source(&item.source)?
    } else {
        fs::read_to_string(&item.source).or_else(|_| {
            fs::read(&item.source).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })?
    };
    let target = fs::read_to_string(&item.target).or_else(|_| {
        fs::read(&item.target).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    })?;

    if symlink_points_to(&item.target, &item.source)? {
        return Ok("target already symlinks to source".to_string());
    }

    if source == target {
        return Ok("files match".to_string());
    }

    let diff = TextDiff::from_lines(&target, &source);
    let mut output = String::new();
    for change in diff.iter_all_changes() {
        let prefix = match change.tag() {
            ChangeTag::Delete => '-',
            ChangeTag::Insert => '+',
            ChangeTag::Equal => ' ',
        };
        output.push(prefix);
        output.push_str(change.as_str().unwrap_or(""));
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }

    if output.is_empty() {
        bail!("failed to render diff for {}", item.name);
    }

    Ok(output)
}
