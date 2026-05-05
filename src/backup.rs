use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::path::{copy_path, path_exists};

#[derive(Debug, Clone)]
pub struct BackupArtifact {
    pub backup_path: PathBuf,
    pub created_at: String,
}

pub fn backup_target(
    name: &str,
    target: &Path,
    backup_root: &Path,
) -> Result<Option<BackupArtifact>> {
    if !path_exists(target) {
        return Ok(None);
    }

    let created_at = now_timestamp();
    let backup_path = backup_root.join(sanitize(name)).join(&created_at);
    copy_path(target, &backup_path)?;

    Ok(Some(BackupArtifact {
        backup_path,
        created_at,
    }))
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
