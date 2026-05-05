use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::config::{Config, load_config};
use crate::path::app_dirs;
use crate::plan::resolve_items;
use crate::state::StateStore;

#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
}

pub fn run_doctor(manifest: &Path) -> Result<Vec<DoctorCheck>> {
    let mut checks = Vec::new();
    let manifest_exists = manifest.exists();

    checks.push(DoctorCheck {
        name: "manifest".to_string(),
        status: if manifest_exists { "ok" } else { "error" }.to_string(),
        detail: if manifest_exists {
            manifest.display().to_string()
        } else {
            format!("missing {}", manifest.display())
        },
    });

    let mut loaded = None;
    if manifest_exists {
        match load_config(manifest) {
            Ok((manifest_path, config)) => {
                checks.push(DoctorCheck {
                    name: "config".to_string(),
                    status: "ok".to_string(),
                    detail: "manifest parsed successfully".to_string(),
                });
                loaded = Some((manifest_path, config));
            }
            Err(err) => checks.push(DoctorCheck {
                name: "config".to_string(),
                status: "error".to_string(),
                detail: err.to_string(),
            }),
        }
    } else {
        checks.push(DoctorCheck {
            name: "config".to_string(),
            status: "error".to_string(),
            detail: "manifest is required before config checks can run".to_string(),
        });
    }

    match loaded.as_ref() {
        Some((manifest_path, config)) => {
            checks.push(check_config_repo(manifest_path, config));
            checks.push(check_target_writable(manifest_path, config));
        }
        None => {
            checks.push(DoctorCheck {
                name: "config-repo".to_string(),
                status: "error".to_string(),
                detail: "could not resolve managed source paths".to_string(),
            });
            checks.push(DoctorCheck {
                name: "target-writable".to_string(),
                status: "error".to_string(),
                detail: "could not resolve target paths".to_string(),
            });
        }
    }

    checks.push(check_git_available());
    checks.push(check_windows_symlink_support());

    let dirs = app_dirs()?;
    checks.push(check_state_store(&dirs.state_db));
    checks.push(check_backup_root(&dirs.backup_root));

    Ok(checks)
}

fn check_config_repo(manifest_path: &Path, config: &Config) -> DoctorCheck {
    match resolve_items(config, manifest_path) {
        Ok(items) => {
            let missing: Vec<_> = items
                .into_iter()
                .filter(|item| !item.source.exists())
                .map(|item| format!("{} -> {}", item.name, item.source.display()))
                .collect();

            if missing.is_empty() {
                let repo_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
                DoctorCheck {
                    name: "config-repo".to_string(),
                    status: "ok".to_string(),
                    detail: format!(
                        "{} source paths resolved under {}",
                        config.items.len(),
                        repo_root.display()
                    ),
                }
            } else {
                DoctorCheck {
                    name: "config-repo".to_string(),
                    status: "error".to_string(),
                    detail: format!("missing sources: {}", missing.join(", ")),
                }
            }
        }
        Err(err) => DoctorCheck {
            name: "config-repo".to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}

fn check_target_writable(manifest_path: &Path, config: &Config) -> DoctorCheck {
    match resolve_items(config, manifest_path) {
        Ok(items) => {
            let mut failures = Vec::new();

            for item in items {
                if let Err(err) = probe_target_writable(&item.target) {
                    failures.push(format!(
                        "{} -> {} ({err})",
                        item.name,
                        item.target.display()
                    ));
                }
            }

            if failures.is_empty() {
                DoctorCheck {
                    name: "target-writable".to_string(),
                    status: "ok".to_string(),
                    detail: format!("{} target paths are writable", config.items.len()),
                }
            } else {
                DoctorCheck {
                    name: "target-writable".to_string(),
                    status: "error".to_string(),
                    detail: failures.join(", "),
                }
            }
        }
        Err(err) => DoctorCheck {
            name: "target-writable".to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}

fn check_git_available() -> DoctorCheck {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => DoctorCheck {
            name: "git".to_string(),
            status: "ok".to_string(),
            detail: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        },
        Ok(output) => DoctorCheck {
            name: "git".to_string(),
            status: "error".to_string(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => DoctorCheck {
            name: "git".to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}

fn check_windows_symlink_support() -> DoctorCheck {
    #[cfg(not(windows))]
    {
        return DoctorCheck {
            name: "windows-symlink".to_string(),
            status: "ok".to_string(),
            detail: "not running on Windows".to_string(),
        };
    }

    #[cfg(windows)]
    {
        let temp_root = std::env::temp_dir().join(format!("kungfig-doctor-{}", unique_id()));
        let source = temp_root.join("source.txt");
        let link = temp_root.join("link.txt");

        let result = (|| -> Result<()> {
            fs::create_dir_all(&temp_root)?;
            fs::write(&source, "kungfig")?;
            std::os::windows::fs::symlink_file(&source, &link)?;
            Ok(())
        })();

        let _ = fs::remove_file(&link);
        let _ = fs::remove_file(&source);
        let _ = fs::remove_dir_all(&temp_root);

        match result {
            Ok(()) => DoctorCheck {
                name: "windows-symlink".to_string(),
                status: "ok".to_string(),
                detail: "symlink creation succeeded".to_string(),
            },
            Err(err) => DoctorCheck {
                name: "windows-symlink".to_string(),
                status: "error".to_string(),
                detail: err.to_string(),
            },
        }
    }
}

fn check_state_store(state_db: &Path) -> DoctorCheck {
    match StateStore::open() {
        Ok(_) => DoctorCheck {
            name: "state-db".to_string(),
            status: "ok".to_string(),
            detail: state_db.display().to_string(),
        },
        Err(err) => DoctorCheck {
            name: "state-db".to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}

fn check_backup_root(backup_root: &Path) -> DoctorCheck {
    if let Err(err) = fs::create_dir_all(backup_root) {
        return DoctorCheck {
            name: "backup-dir".to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        };
    }

    match probe_directory_writable(backup_root) {
        Ok(()) => DoctorCheck {
            name: "backup-dir".to_string(),
            status: "ok".to_string(),
            detail: backup_root.display().to_string(),
        },
        Err(err) => DoctorCheck {
            name: "backup-dir".to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}

fn probe_target_writable(target: &Path) -> Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    if parent.exists() {
        return probe_directory_writable(parent);
    }

    let nearest_existing = nearest_existing_ancestor(parent).unwrap_or_else(|| PathBuf::from("."));
    probe_directory_writable(&nearest_existing)
}

fn probe_directory_writable(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    let probe = path.join(format!(".kungfig-write-check-{}", unique_id()));
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)?;
    fs::remove_file(&probe)?;
    Ok(())
}

fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        current = current.parent()?.to_path_buf();
    }
}

fn unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}
