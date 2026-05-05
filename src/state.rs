use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::config::Mode;
use crate::path::{AppDirs, app_dirs, hash_path, path_exists, same_content, symlink_points_to};
use crate::plan::ResolvedItem;

#[derive(Debug)]
pub struct StateStore {
    conn: Connection,
    dirs: AppDirs,
}

#[derive(Debug, Clone)]
pub struct ManagedRecord {
    pub source_hash: String,
    pub target_hash: String,
}

#[derive(Debug, Clone)]
pub struct BackupRecord {
    pub backup_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StatusEntry {
    pub name: String,
    pub status: String,
    pub detail: String,
}

impl StateStore {
    pub fn open() -> Result<Self> {
        let dirs = app_dirs()?;
        if let Some(parent) = dirs.state_db.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&dirs.backup_root)?;

        let conn = Connection::open(&dirs.state_db)?;
        let store = Self { conn, dirs };
        store.init_schema()?;
        Ok(store)
    }

    pub fn backup_root(&self) -> &Path {
        &self.dirs.backup_root
    }

    pub fn record_managed(&self, item: &ResolvedItem) -> Result<()> {
        let source_hash = hash_path(&item.source)?;
        let target_hash = hash_path(&item.target)?;
        let applied_at = now_timestamp();

        self.conn.execute(
            "INSERT INTO managed_files
             (name, source_path, target_path, mode, source_hash, target_hash, applied_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(name) DO UPDATE SET
                 source_path = excluded.source_path,
                 target_path = excluded.target_path,
                 mode = excluded.mode,
                 source_hash = excluded.source_hash,
                 target_hash = excluded.target_hash,
                 applied_at = excluded.applied_at",
            params![
                item.name,
                item.source.display().to_string(),
                item.target.display().to_string(),
                item.mode.to_string(),
                source_hash,
                target_hash,
                applied_at,
            ],
        )?;

        Ok(())
    }

    pub fn record_backup(
        &self,
        name: &str,
        target_path: &Path,
        backup_path: &Path,
        created_at: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO backups (name, target_path, backup_path, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                name,
                target_path.display().to_string(),
                backup_path.display().to_string(),
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn managed_record(&self, name: &str) -> Result<Option<ManagedRecord>> {
        let row = self
            .conn
            .query_row(
                "SELECT source_hash, target_hash FROM managed_files WHERE name = ?1",
                params![name],
                |row| {
                    Ok(ManagedRecord {
                        source_hash: row.get(0)?,
                        target_hash: row.get(1)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn latest_backup(&self, name: &str, target_path: &Path) -> Result<Option<BackupRecord>> {
        let row = self
            .conn
            .query_row(
                "SELECT backup_path
                 FROM backups
                 WHERE name = ?1 AND target_path = ?2
                 ORDER BY id DESC
                 LIMIT 1",
                params![name, target_path.display().to_string()],
                |row| {
                    Ok(BackupRecord {
                        backup_path: PathBuf::from(row.get::<_, String>(0)?),
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS managed_files (
                name TEXT PRIMARY KEY,
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                mode TEXT NOT NULL,
                source_hash TEXT NOT NULL,
                target_hash TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS backups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                target_path TEXT NOT NULL,
                backup_path TEXT NOT NULL,
                created_at TEXT NOT NULL
            );",
        )?;
        Ok(())
    }
}

pub fn collect_status(items: &[ResolvedItem], store: &StateStore) -> Result<Vec<StatusEntry>> {
    let mut statuses = Vec::with_capacity(items.len());

    for item in items {
        if !path_exists(&item.source) {
            statuses.push(StatusEntry {
                name: item.name.clone(),
                status: "missing-source".to_string(),
                detail: item.source.display().to_string(),
            });
            continue;
        }

        if !path_exists(&item.target) {
            statuses.push(StatusEntry {
                name: item.name.clone(),
                status: "missing-target".to_string(),
                detail: item.target.display().to_string(),
            });
            continue;
        }

        let desired = match item.mode {
            Mode::Copy => same_content(&item.source, &item.target)?,
            Mode::Symlink => symlink_points_to(&item.target, &item.source)?,
        };

        if desired {
            statuses.push(StatusEntry {
                name: item.name.clone(),
                status: "synced".to_string(),
                detail: "source and target match".to_string(),
            });
            continue;
        }

        let current_target_hash = hash_path(&item.target)?;
        match store.managed_record(&item.name)? {
            Some(record) if record.target_hash == current_target_hash => {
                statuses.push(StatusEntry {
                    name: item.name.clone(),
                    status: "pending".to_string(),
                    detail: "source changed but target still matches last apply".to_string(),
                })
            }
            Some(_) => statuses.push(StatusEntry {
                name: item.name.clone(),
                status: "drift".to_string(),
                detail: "target differs from last applied state".to_string(),
            }),
            None => statuses.push(StatusEntry {
                name: item.name.clone(),
                status: "unmanaged".to_string(),
                detail: "item differs and has no state record".to_string(),
            }),
        }
    }

    Ok(statuses)
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
