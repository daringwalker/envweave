#![forbid(unsafe_code)]

//! Transactional backups and restoration.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub id: String,
    pub stored_path: PathBuf,
    pub original_path: PathBuf,
    pub existed: bool,
}

pub fn create(root: &Path, original: &Path) -> Result<Backup, BackupError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = format!("{stamp:x}");
    let backup_directory = root.join(&id);
    fs::create_dir_all(&backup_directory)?;
    let stored_path = backup_directory.join("content");
    let existed = original.exists();
    if existed {
        copy_tree(original, &stored_path)?;
    }
    let backup = Backup {
        id,
        stored_path,
        original_path: original.to_path_buf(),
        existed,
    };
    fs::write(
        backup_directory.join("backup.toml"),
        toml::to_string(&backup).map_err(std::io::Error::other)?,
    )?;
    Ok(backup)
}

pub fn restore(backup: &Backup) -> Result<(), BackupError> {
    if backup.existed {
        replace_tree(&backup.stored_path, &backup.original_path)?;
    } else {
        remove_existing(&backup.original_path)?;
    }
    Ok(())
}

pub fn list(root: &Path) -> Result<Vec<Backup>, BackupError> {
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut backups: Vec<Backup> = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path().join("backup.toml");
        if path.is_file() {
            let text = fs::read_to_string(path)?;
            if let Ok(backup) = toml::from_str(&text) {
                backups.push(backup);
            }
        }
    }
    backups.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(backups)
}

pub fn restore_id(root: &Path, id: &str) -> Result<Backup, BackupError> {
    let backup = load_id(root, id)?;
    restore(&backup)?;
    Ok(backup)
}

pub fn load_id(root: &Path, id: &str) -> Result<Backup, BackupError> {
    if id.is_empty() || !id.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid backup id").into(),
        );
    }
    let text = fs::read_to_string(root.join(id).join("backup.toml"))?;
    let backup: Backup = toml::from_str(&text).map_err(std::io::Error::other)?;
    if backup.id != id || backup.stored_path != root.join(id).join("content") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "backup metadata does not match its storage location",
        )
        .into());
    }
    Ok(backup)
}

fn remove_existing(path: &Path) -> Result<(), std::io::Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
        }
        Ok(_) => fs::remove_file(path),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn replace_tree(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = destination.with_extension(format!("envweave-restore-{stamp:x}.tmp"));
    let previous = destination.with_extension(format!("envweave-previous-{stamp:x}.tmp"));
    copy_tree(source, &staging)?;
    let had_destination = fs::symlink_metadata(destination).is_ok();
    if had_destination {
        fs::rename(destination, &previous)?;
    }
    if let Err(error) = fs::rename(&staging, destination) {
        if had_destination {
            let _ = fs::rename(&previous, destination);
        }
        let _ = remove_existing(&staging);
        return Err(error);
    }
    if had_destination {
        let _ = remove_existing(&previous);
    }
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
    } else if metadata.is_file() {
        fs::copy(source, destination)?;
    } else {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let e = entry?;
            copy_tree(&e.path(), &destination.join(e.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_previous_content() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("home/file");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "before").unwrap();
        let root = d.path().join("backups");
        let b = create(&root, &p).unwrap();
        fs::write(&p, "after").unwrap();
        assert_eq!(list(&root).unwrap().len(), 1);
        restore_id(&root, &b.id).unwrap();
        assert_eq!(fs::read_to_string(p).unwrap(), "before");
    }

    #[test]
    fn records_and_restores_a_previously_missing_target() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("home/new-file");
        let root = directory.path().join("backups");
        let backup = create(&root, &target).unwrap();
        assert!(!backup.existed);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "created later").unwrap();
        restore(&backup).unwrap();
        assert!(!target.exists());
    }
}
