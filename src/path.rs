use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use directories::{BaseDirs, ProjectDirs, UserDirs};
use sha2::{Digest, Sha256};

use crate::error::KungfigError;

#[derive(Debug, Clone)]
pub struct AppDirs {
    pub state_db: PathBuf,
    pub backup_root: PathBuf,
}

pub fn current_platform() -> &'static str {
    match env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    }
}

pub fn app_dirs() -> Result<AppDirs> {
    if let Some(db_path) = env::var_os("KUNGFIG_STATE_DB") {
        let state_db = PathBuf::from(db_path);
        let backup_root = env::var_os("KUNGFIG_BACKUP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                state_db
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("backups")
            });
        return Ok(AppDirs {
            state_db,
            backup_root,
        });
    }

    let project_dirs = ProjectDirs::from("", "", "kungfig")
        .ok_or_else(|| anyhow!("could not determine app directories"))?;
    Ok(AppDirs {
        state_db: project_dirs.data_local_dir().join("state.db"),
        backup_root: project_dirs.data_local_dir().join("backups"),
    })
}

pub fn expand_path(input: &str) -> Result<PathBuf> {
    expand_path_from(Path::new("."), input)
}

pub fn expand_path_from(base_dir: &Path, input: &str) -> Result<PathBuf> {
    let expanded = substitute_variables(&expand_tilde(input)?)?;
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(base_dir.join(path))
    }
}

pub fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn remove_path(path: &Path) -> Result<()> {
    if !path_exists(path) {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)?;
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn copy_path(source: &Path, target: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect {}", source.display()))?;

    if metadata.file_type().is_symlink() {
        copy_symlink(source, target)?;
    } else if metadata.is_dir() {
        copy_dir(source, target)?;
    } else {
        ensure_parent_dir(target)?;
        fs::copy(source, target).with_context(|| {
            format!(
                "failed to copy {} -> {}",
                source.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

pub fn create_symlink(link_path: &Path, target_path: &Path) -> Result<()> {
    ensure_parent_dir(link_path)?;
    remove_path(link_path)?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(target_path, link_path)?;

    #[cfg(windows)]
    {
        if target_path.is_dir() {
            std::os::windows::fs::symlink_dir(target_path, link_path)?;
        } else {
            std::os::windows::fs::symlink_file(target_path, link_path)?;
        }
    }

    Ok(())
}

pub fn same_content(left: &Path, right: &Path) -> Result<bool> {
    Ok(hash_path(left)? == hash_path(right)?)
}

pub fn hash_path(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    let mut digest = Sha256::new();

    if metadata.file_type().is_symlink() {
        digest.update(b"symlink");
        digest.update(fs::read_link(path)?.to_string_lossy().as_bytes());
        return Ok(hex::encode(digest.finalize()));
    }

    if metadata.is_file() {
        digest.update(b"file");
        hash_file_into(path, &mut digest)?;
        return Ok(hex::encode(digest.finalize()));
    }

    if metadata.is_dir() {
        digest.update(b"dir");
        hash_dir_into(path, path, &mut digest)?;
        return Ok(hex::encode(digest.finalize()));
    }

    Ok(hex::encode(digest.finalize()))
}

pub fn symlink_points_to(link_path: &Path, target_path: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(link_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(false),
    };
    if !metadata.file_type().is_symlink() {
        return Ok(false);
    }

    let actual = fs::read_link(link_path)?;
    let actual = if actual.is_absolute() {
        actual
    } else {
        link_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(actual)
    };

    Ok(normalize_path(&actual) == normalize_path(target_path))
}

pub fn compact_path_for_manifest(path: &Path) -> Result<String> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| anyhow!("could not determine base directories"))?;
    let user_dirs = UserDirs::new();
    let absolute = normalize_path(path);

    let mappings = [
        ("home", base_dirs.home_dir().to_path_buf()),
        ("config", base_dirs.config_dir().to_path_buf()),
        ("data", base_dirs.data_dir().to_path_buf()),
        ("cache", base_dirs.cache_dir().to_path_buf()),
        (
            "appdata",
            if current_platform() == "windows" {
                env::var_os("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| base_dirs.config_dir().to_path_buf())
            } else {
                base_dirs.config_dir().to_path_buf()
            },
        ),
        (
            "localappdata",
            if current_platform() == "windows" {
                env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| base_dirs.data_local_dir().to_path_buf())
            } else {
                base_dirs.data_local_dir().to_path_buf()
            },
        ),
        (
            "documents",
            user_dirs
                .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
                .unwrap_or_else(|| base_dirs.home_dir().join("Documents")),
        ),
    ];

    for (name, root) in mappings {
        if absolute == root {
            return Ok(format!("{{{name}}}"));
        }

        if let Ok(relative) = absolute.strip_prefix(&root) {
            if relative.as_os_str().is_empty() {
                return Ok(format!("{{{name}}}"));
            }

            let relative = relative.to_string_lossy().replace('\\', "/");
            return Ok(format!("{{{name}}}/{relative}"));
        }
    }

    Ok(absolute.to_string_lossy().replace('\\', "/"))
}

fn substitute_variables(input: &str) -> Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            output.push_str(&rest[start..]);
            return Ok(output);
        };

        let key = &after_start[..end];
        output.push_str(&variable_value(key)?);
        rest = &after_start[end + 1..];
    }

    output.push_str(rest);
    Ok(output)
}

fn expand_tilde(input: &str) -> Result<String> {
    if input == "~" {
        let base_dirs =
            BaseDirs::new().ok_or_else(|| anyhow!("could not determine base directories"))?;
        return Ok(base_dirs.home_dir().display().to_string());
    }

    if let Some(rest) = input.strip_prefix("~/") {
        let base_dirs =
            BaseDirs::new().ok_or_else(|| anyhow!("could not determine base directories"))?;
        return Ok(base_dirs.home_dir().join(rest).display().to_string());
    }

    Ok(input.to_string())
}

fn variable_value(key: &str) -> Result<String> {
    let base_dirs =
        BaseDirs::new().ok_or_else(|| anyhow!("could not determine base directories"))?;
    let user_dirs = UserDirs::new();

    let value = match key {
        "home" => base_dirs.home_dir().to_path_buf(),
        "config" => base_dirs.config_dir().to_path_buf(),
        "data" => base_dirs.data_dir().to_path_buf(),
        "cache" => base_dirs.cache_dir().to_path_buf(),
        "appdata" => {
            if current_platform() == "windows" {
                env::var_os("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| base_dirs.config_dir().to_path_buf())
            } else {
                base_dirs.config_dir().to_path_buf()
            }
        }
        "localappdata" => {
            if current_platform() == "windows" {
                env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| base_dirs.data_local_dir().to_path_buf())
            } else {
                base_dirs.data_local_dir().to_path_buf()
            }
        }
        "documents" => user_dirs
            .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
            .unwrap_or_else(|| base_dirs.home_dir().join("Documents")),
        other => return Err(KungfigError::UnknownPathVariable(other.to_string()).into()),
    };

    Ok(value.display().to_string())
}

fn copy_dir(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;

    let mut entries: Vec<_> = fs::read_dir(source)?.collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        copy_path(&source_path, &target_path)?;
    }

    Ok(())
}

fn copy_symlink(source: &Path, target: &Path) -> Result<()> {
    let link_target = fs::read_link(source)?;
    ensure_parent_dir(target)?;

    #[cfg(unix)]
    std::os::unix::fs::symlink(&link_target, target)?;

    #[cfg(windows)]
    {
        let resolved = if link_target.is_absolute() {
            link_target.clone()
        } else {
            source
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&link_target)
        };

        if resolved.is_dir() {
            std::os::windows::fs::symlink_dir(&link_target, target)?;
        } else {
            std::os::windows::fs::symlink_file(&link_target, target)?;
        }
    }

    Ok(())
}

fn hash_file_into(path: &Path, digest: &mut Sha256) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut buffer = [0_u8; 8 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }

    Ok(())
}

fn hash_dir_into(root: &Path, path: &Path, digest: &mut Sha256) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(path)?.collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .context("failed to compute relative path while hashing directory")?;
        digest.update(relative.to_string_lossy().as_bytes());

        let metadata = fs::symlink_metadata(&entry_path)?;
        if metadata.file_type().is_symlink() {
            digest.update(b"symlink");
            digest.update(fs::read_link(&entry_path)?.to_string_lossy().as_bytes());
        } else if metadata.is_dir() {
            digest.update(b"dir");
            hash_dir_into(root, &entry_path, digest)?;
        } else {
            digest.update(b"file");
            hash_file_into(&entry_path, digest)?;
        }
    }

    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(".").join(path)
    }
}
