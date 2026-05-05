use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::engine::{FileMode, Item, StatusResult, current_platform};

pub fn get_status(item: &Item) -> StatusResult {
    let Some(live_path) = item.live_path.as_ref() else {
        return StatusResult {
            status: "unavailable".to_string(),
            detail: format!("not configured for {}", current_platform()),
        };
    };
    let Some(store_path) = item.store_path.as_ref() else {
        return StatusResult {
            status: "unavailable".to_string(),
            detail: format!("not configured for {}", current_platform()),
        };
    };

    let live_exists = path_exists(live_path);
    let store_exists = path_exists(store_path);

    if item.mode == FileMode::Symlink && live_exists && store_exists && live_path.is_symlink() {
        if same_resolved_path(live_path, store_path) {
            return StatusResult {
                status: "linked".to_string(),
                detail: "live path points to store".to_string(),
            };
        }
    }

    if item.mode == FileMode::Hardlink
        && live_exists
        && store_exists
        && same_hardlink(live_path, store_path)
    {
        return StatusResult {
            status: "linked".to_string(),
            detail: "live path hardlinks store".to_string(),
        };
    }

    if !live_exists && !store_exists {
        return StatusResult {
            status: "missing".to_string(),
            detail: "live and store are missing".to_string(),
        };
    }
    if !live_exists {
        return StatusResult {
            status: "missing-live".to_string(),
            detail: "live path is missing".to_string(),
        };
    }
    if !store_exists {
        return StatusResult {
            status: "missing-store".to_string(),
            detail: "store path is missing".to_string(),
        };
    }
    if same_content(live_path, store_path) {
        return StatusResult {
            status: "synced".to_string(),
            detail: "live and store match".to_string(),
        };
    }

    StatusResult {
        status: "drift".to_string(),
        detail: "live and store differ".to_string(),
    }
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn same_resolved_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn same_hardlink(left: &Path, right: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        match (fs::metadata(left), fs::metadata(right)) {
            (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
            _ => false,
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (left, right);
        false
    }
}

fn same_content(left: &Path, right: &Path) -> bool {
    path_signature(left).ok() == path_signature(right).ok()
}

fn path_signature(path: &Path) -> Result<(String, String)> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "broken".to_string());
        return Ok(("symlink".to_string(), target));
    }
    if metadata.is_file() {
        return Ok(("file".to_string(), file_digest(path)?));
    }
    if metadata.is_dir() {
        return Ok(("dir".to_string(), tree_digest(path)?));
    }
    Ok(("missing".to_string(), String::new()))
}

fn file_digest(path: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }

    Ok(format!("{:x}", digest.finalize()))
}

fn tree_digest(path: &Path) -> Result<String> {
    let mut nodes: Vec<PathBuf> = WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path() != path)
        .map(|entry| entry.into_path())
        .collect();
    nodes.sort();

    let mut digest = Sha256::new();
    for node in nodes {
        let relative = node
            .strip_prefix(path)
            .context("Failed to compute relative path while hashing directory")?;
        digest.update(relative.to_string_lossy().as_bytes());

        let metadata = fs::symlink_metadata(&node)?;
        if metadata.file_type().is_symlink() {
            digest.update(b"symlink");
            digest.update(fs::read_link(&node)?.to_string_lossy().as_bytes());
        } else if metadata.is_file() {
            digest.update(file_digest(&node)?.as_bytes());
        } else {
            digest.update(b"dir");
        }
    }

    Ok(format!("{:x}", digest.finalize()))
}
