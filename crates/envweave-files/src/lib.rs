#![forbid(unsafe_code)]

//! Safe collection and application of configuration files.

use envweave_manifest::{ConfigItem, ItemKind};
use envweave_security::{SecurityError, expand_target, repository_path};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    InSync,
    Modified,
    MissingTarget,
    MissingRepositoryCopy,
    TypeMismatch,
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("source type does not match manifest for {0}")]
    TypeMismatch(String),
}

pub fn scan(repo: &Path, home: &Path, item: &ConfigItem) -> Result<FileStatus, FileError> {
    let stored = repository_path(repo, &item.source)?;
    let target = expand_target(home, &item.target)?;
    if !stored.exists() {
        return Ok(FileStatus::MissingRepositoryCopy);
    }
    if !target.exists() {
        return Ok(FileStatus::MissingTarget);
    }
    if matches!(item.kind, ItemKind::File) != stored.is_file()
        || stored.is_file() != target.is_file()
    {
        return Ok(FileStatus::TypeMismatch);
    }
    Ok(if equal_tree(&stored, &target)? {
        FileStatus::InSync
    } else {
        FileStatus::Modified
    })
}

pub fn capture(repo: &Path, home: &Path, item: &ConfigItem) -> Result<(), FileError> {
    let source = expand_target(home, &item.target)?;
    ensure_kind(&source, item)?;
    let destination = repository_path(repo, &item.source)?;
    replace_tree(&source, &destination)
}

pub fn apply(repo: &Path, home: &Path, item: &ConfigItem) -> Result<(), FileError> {
    let source = repository_path(repo, &item.source)?;
    ensure_kind(&source, item)?;
    let destination = expand_target(home, &item.target)?;
    replace_tree(&source, &destination)
}

fn ensure_kind(path: &Path, item: &ConfigItem) -> Result<(), FileError> {
    let valid = match item.kind {
        ItemKind::File => path.is_file(),
        ItemKind::Directory => path.is_dir(),
    };
    if valid {
        Ok(())
    } else {
        Err(FileError::TypeMismatch(item.id.clone()))
    }
}

fn replace_tree(source: &Path, destination: &Path) -> Result<(), FileError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = destination.with_extension(format!("envweave-{stamp:x}.tmp"));
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
        return Err(error.into());
    }
    if had_destination {
        let _ = remove_existing(&previous);
    }
    Ok(())
}

fn remove_existing(path: &Path) -> Result<(), std::io::Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)
        }
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), FileError> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
        return Ok(());
    }
    if metadata.is_file() {
        fs::copy(source, destination)?;
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
    }
    Ok(())
}

fn equal_tree(left: &Path, right: &Path) -> Result<bool, FileError> {
    let left_metadata = fs::symlink_metadata(left)?;
    let right_metadata = fs::symlink_metadata(right)?;
    if left_metadata.file_type().is_symlink() {
        return Ok(right_metadata.file_type().is_symlink()
            && fs::read_link(left)? == fs::read_link(right)?);
    }
    if left_metadata.is_file() {
        return Ok(right_metadata.is_file() && fs::read(left)? == fs::read(right)?);
    }
    if !right.is_dir() {
        return Ok(false);
    }
    let mut left_names: Vec<_> = fs::read_dir(left)?
        .map(|e| e.map(|v| v.file_name()))
        .collect::<Result<_, _>>()?;
    let mut right_names: Vec<_> = fs::read_dir(right)?
        .map(|e| e.map(|v| v.file_name()))
        .collect::<Result<_, _>>()?;
    left_names.sort();
    right_names.sort();
    if left_names != right_names {
        return Ok(false);
    }
    for name in left_names {
        if !equal_tree(&left.join(&name), &right.join(&name))? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn target_path(home: &Path, item: &ConfigItem) -> Result<PathBuf, FileError> {
    Ok(expand_target(home, &item.target)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(kind: ItemKind) -> ConfigItem {
        ConfigItem {
            id: "zsh".into(),
            application_id: "shell.zsh".into(),
            name: "Zsh".into(),
            source: "files/zsh".into(),
            target: "~/.zshrc".into(),
            kind,
            adapter: envweave_manifest::AdapterKind::Filesystem,
            apply_strategy: envweave_manifest::ApplyStrategy::Replace,
            portability: envweave_manifest::Portability::Portable,
            scope: envweave_manifest::ConfigScope::User,
            platforms: vec![],
            tags: vec![],
            conditions: envweave_manifest::ItemConditions::default(),
            dependencies: vec![],
            sensitive: false,
            exclude: vec![],
            validators: vec![],
            enabled: true,
        }
    }
    #[test]
    fn capture_scan_apply_round_trip() {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".zshrc"), "one").unwrap();
        capture(repo.path(), home.path(), &item(ItemKind::File)).unwrap();
        assert_eq!(
            scan(repo.path(), home.path(), &item(ItemKind::File)).unwrap(),
            FileStatus::InSync
        );
        fs::write(home.path().join(".zshrc"), "two").unwrap();
        assert_eq!(
            scan(repo.path(), home.path(), &item(ItemKind::File)).unwrap(),
            FileStatus::Modified
        );
        apply(repo.path(), home.path(), &item(ItemKind::File)).unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join(".zshrc")).unwrap(),
            "one"
        );
    }
}
