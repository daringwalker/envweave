#![forbid(unsafe_code)]

//! Safe collection, preview, and application of configuration files.

use envweave_manifest::{ApplyStrategy, ConfigItem, ItemKind};
use envweave_security::{SecurityError, expand_target, repository_path};
use std::{
    collections::BTreeMap,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyPreview {
    pub creates: Vec<PathBuf>,
    pub updates: Vec<PathBuf>,
    pub deletes: Vec<PathBuf>,
    pub preserves: Vec<PathBuf>,
}

impl ApplyPreview {
    pub fn has_changes(&self) -> bool {
        !self.creates.is_empty() || !self.updates.is_empty() || !self.deletes.is_empty()
    }
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
    if fs::symlink_metadata(&stored).is_err() {
        return Ok(FileStatus::MissingRepositoryCopy);
    }
    if fs::symlink_metadata(&target).is_err() {
        return Ok(FileStatus::MissingTarget);
    }
    if !kind_matches(&stored, &item.kind)? || !kind_matches(&target, &item.kind)? {
        return Ok(FileStatus::TypeMismatch);
    }
    Ok(if preview_apply(repo, home, item)?.has_changes() {
        FileStatus::Modified
    } else {
        FileStatus::InSync
    })
}

pub fn preview_apply(
    repo: &Path,
    home: &Path,
    item: &ConfigItem,
) -> Result<ApplyPreview, FileError> {
    let source = repository_path(repo, &item.source)?;
    ensure_kind(&source, item)?;
    let target = expand_target(home, &item.target)?;
    if fs::symlink_metadata(&target).is_err() {
        let mut preview = ApplyPreview::default();
        if item.kind == ItemKind::File {
            preview.creates.push(PathBuf::from("."));
        } else {
            preview.creates = collect_entries(&source)?
                .into_keys()
                .filter(|path| !is_excluded(path, &item.exclude))
                .collect();
        }
        return Ok(preview);
    }
    if !kind_matches(&target, &item.kind)? {
        return Err(FileError::TypeMismatch(item.id.clone()));
    }
    if item.kind == ItemKind::File {
        let mut preview = ApplyPreview::default();
        if equal_node(&source, &target)? {
            preview.preserves.push(PathBuf::from("."));
        } else {
            preview.updates.push(PathBuf::from("."));
        }
        return Ok(preview);
    }

    let source_entries = collect_entries(&source)?;
    let target_entries = collect_entries(&target)?;
    let mut preview = ApplyPreview::default();
    for (relative, source_path) in &source_entries {
        if is_excluded(relative, &item.exclude) {
            continue;
        }
        if item.apply_strategy == ApplyStrategy::KeepExisting
            && has_existing_blocking_ancestor(relative, &target_entries)?
        {
            preview.preserves.push(relative.clone());
            continue;
        }
        match target_entries.get(relative) {
            Some(_) if item.apply_strategy == ApplyStrategy::KeepExisting => {
                preview.preserves.push(relative.clone());
            }
            Some(target_path) if equal_node(source_path, target_path)? => {
                preview.preserves.push(relative.clone());
            }
            Some(_) => preview.updates.push(relative.clone()),
            None => preview.creates.push(relative.clone()),
        }
    }
    for relative in target_entries.keys() {
        if is_excluded(relative, &item.exclude) || !source_entries.contains_key(relative) {
            if item.apply_strategy == ApplyStrategy::Replace
                && !is_excluded(relative, &item.exclude)
            {
                preview.deletes.push(relative.clone());
            } else {
                preview.preserves.push(relative.clone());
            }
        }
    }
    Ok(preview)
}

pub fn capture(repo: &Path, home: &Path, item: &ConfigItem) -> Result<(), FileError> {
    let source = expand_target(home, &item.target)?;
    ensure_kind(&source, item)?;
    let destination = repository_path(repo, &item.source)?;
    if item.kind == ItemKind::File {
        return replace_tree(&source, &destination);
    }
    let staging = staging_path(&destination, "capture");
    fs::create_dir_all(&staging)?;
    overlay_directory(&source, &staging, Path::new(""), true, &item.exclude)?;
    commit_staging(&staging, &destination)
}

pub fn apply(repo: &Path, home: &Path, item: &ConfigItem) -> Result<(), FileError> {
    let source = repository_path(repo, &item.source)?;
    ensure_kind(&source, item)?;
    let destination = expand_target(home, &item.target)?;
    if item.kind == ItemKind::File {
        return replace_tree(&source, &destination);
    }

    let staging = staging_path(&destination, "apply");
    match item.apply_strategy {
        ApplyStrategy::Replace => {
            fs::create_dir_all(&staging)?;
            overlay_directory(&source, &staging, Path::new(""), true, &item.exclude)?;
            preserve_excluded(&destination, &staging, &item.exclude)?;
        }
        ApplyStrategy::Merge | ApplyStrategy::KeepExisting => {
            if fs::symlink_metadata(&destination).is_ok() {
                copy_tree(&destination, &staging)?;
            } else {
                fs::create_dir_all(&staging)?;
            }
            overlay_directory(
                &source,
                &staging,
                Path::new(""),
                item.apply_strategy == ApplyStrategy::Merge,
                &item.exclude,
            )?;
        }
    }
    commit_staging(&staging, &destination)
}

fn kind_matches(path: &Path, kind: &ItemKind) -> Result<bool, std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(match kind {
        ItemKind::File => metadata.is_file(),
        ItemKind::Directory => metadata.is_dir() && !metadata.file_type().is_symlink(),
    })
}

fn ensure_kind(path: &Path, item: &ConfigItem) -> Result<(), FileError> {
    if kind_matches(path, &item.kind).unwrap_or(false) {
        Ok(())
    } else {
        Err(FileError::TypeMismatch(item.id.clone()))
    }
}

fn replace_tree(source: &Path, destination: &Path) -> Result<(), FileError> {
    let staging = staging_path(destination, "replace");
    if let Some(parent) = staging.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_tree(source, &staging)?;
    commit_staging(&staging, destination)
}

fn staging_path(destination: &Path, operation: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    destination.with_extension(format!("envweave-{operation}-{stamp:x}.tmp"))
}

fn commit_staging(staging: &Path, destination: &Path) -> Result<(), FileError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let previous = staging_path(destination, "previous");
    let had_destination = fs::symlink_metadata(destination).is_ok();
    if had_destination {
        fs::rename(destination, &previous)?;
    }
    if let Err(error) = fs::rename(staging, destination) {
        if had_destination {
            let _ = fs::rename(&previous, destination);
        }
        let _ = remove_existing(staging);
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

fn overlay_directory(
    source: &Path,
    destination: &Path,
    relative: &Path,
    overwrite: bool,
    excludes: &[String],
) -> Result<(), FileError> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        if is_excluded(&child_relative, excludes) {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let source_metadata = fs::symlink_metadata(&source_path)?;
        let destination_metadata = fs::symlink_metadata(&destination_path).ok();
        if source_metadata.is_dir() && !source_metadata.file_type().is_symlink() {
            if let Some(metadata) = destination_metadata {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    if !overwrite {
                        continue;
                    }
                    remove_existing(&destination_path)?;
                    fs::create_dir_all(&destination_path)?;
                }
            } else {
                fs::create_dir_all(&destination_path)?;
            }
            overlay_directory(
                &source_path,
                &destination_path,
                &child_relative,
                overwrite,
                excludes,
            )?;
        } else if destination_metadata.is_none() || overwrite {
            remove_existing(&destination_path)?;
            copy_tree(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn preserve_excluded(source: &Path, staging: &Path, excludes: &[String]) -> Result<(), FileError> {
    for excluded in excludes {
        let relative = Path::new(excluded.trim_end_matches('/'));
        let source_path = source.join(relative);
        if fs::symlink_metadata(&source_path).is_err() {
            continue;
        }
        let destination = staging.join(relative);
        if fs::symlink_metadata(&destination).is_ok() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_tree(&source_path, &destination)?;
    }
    Ok(())
}

fn is_excluded(relative: &Path, excludes: &[String]) -> bool {
    excludes.iter().any(|excluded| {
        let excluded = Path::new(excluded.trim_end_matches('/'));
        relative == excluded || relative.starts_with(excluded)
    })
}

fn collect_entries(root: &Path) -> Result<BTreeMap<PathBuf, PathBuf>, FileError> {
    fn visit(
        root: &Path,
        directory: &Path,
        entries: &mut BTreeMap<PathBuf, PathBuf>,
    ) -> Result<(), FileError> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            let metadata = fs::symlink_metadata(&path)?;
            entries.insert(relative, path.clone());
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                visit(root, &path, entries)?;
            }
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries)?;
    Ok(entries)
}

fn has_existing_blocking_ancestor(
    relative: &Path,
    targets: &BTreeMap<PathBuf, PathBuf>,
) -> Result<bool, FileError> {
    for ancestor in relative
        .ancestors()
        .skip(1)
        .filter(|path| !path.as_os_str().is_empty())
    {
        if let Some(path) = targets.get(ancestor) {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn equal_node(left: &Path, right: &Path) -> Result<bool, FileError> {
    let left_metadata = fs::symlink_metadata(left)?;
    let right_metadata = fs::symlink_metadata(right)?;
    if left_metadata.file_type().is_symlink() {
        return Ok(right_metadata.file_type().is_symlink()
            && fs::read_link(left)? == fs::read_link(right)?);
    }
    if left_metadata.is_file() {
        return Ok(right_metadata.is_file() && fs::read(left)? == fs::read(right)?);
    }
    Ok(left_metadata.is_dir()
        && right_metadata.is_dir()
        && !right_metadata.file_type().is_symlink())
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
            target: if kind == ItemKind::File {
                "~/.zshrc"
            } else {
                "~/.config/zsh"
            }
            .into(),
            kind,
            adapter: envweave_manifest::AdapterKind::Filesystem,
            apply_strategy: ApplyStrategy::Replace,
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

    fn directory_fixture() -> (tempfile::TempDir, tempfile::TempDir, ConfigItem) {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("files/zsh/nested")).unwrap();
        fs::create_dir_all(home.path().join(".config/zsh/nested")).unwrap();
        fs::write(repo.path().join("files/zsh/shared"), "repository").unwrap();
        fs::write(repo.path().join("files/zsh/nested/new"), "new").unwrap();
        fs::write(home.path().join(".config/zsh/shared"), "local").unwrap();
        fs::write(home.path().join(".config/zsh/local-only"), "keep me").unwrap();
        fs::write(home.path().join(".config/zsh/remove-me"), "remove me").unwrap();
        (repo, home, item(ItemKind::Directory))
    }

    #[test]
    fn replace_deletes_untracked_paths_but_preserves_exclusions() {
        let (repo, home, mut value) = directory_fixture();
        value.exclude = vec!["local-only".into()];
        let preview = preview_apply(repo.path(), home.path(), &value).unwrap();
        assert!(preview.updates.contains(&PathBuf::from("shared")));
        assert!(!preview.deletes.contains(&PathBuf::from("local-only")));
        assert!(preview.deletes.contains(&PathBuf::from("remove-me")));
        apply(repo.path(), home.path(), &value).unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join(".config/zsh/shared")).unwrap(),
            "repository"
        );
        assert!(home.path().join(".config/zsh/local-only").exists());
        assert!(!home.path().join(".config/zsh/remove-me").exists());
    }

    #[test]
    fn merge_overwrites_repository_paths_and_keeps_local_only_paths() {
        let (repo, home, mut value) = directory_fixture();
        value.apply_strategy = ApplyStrategy::Merge;
        let preview = preview_apply(repo.path(), home.path(), &value).unwrap();
        assert!(preview.deletes.is_empty());
        apply(repo.path(), home.path(), &value).unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join(".config/zsh/shared")).unwrap(),
            "repository"
        );
        assert!(home.path().join(".config/zsh/local-only").exists());
        assert_eq!(
            scan(repo.path(), home.path(), &value).unwrap(),
            FileStatus::InSync
        );
    }

    #[test]
    fn keep_existing_only_adds_missing_paths() {
        let (repo, home, mut value) = directory_fixture();
        value.apply_strategy = ApplyStrategy::KeepExisting;
        apply(repo.path(), home.path(), &value).unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join(".config/zsh/shared")).unwrap(),
            "local"
        );
        assert!(home.path().join(".config/zsh/nested/new").exists());
        assert!(home.path().join(".config/zsh/local-only").exists());
        assert_eq!(
            scan(repo.path(), home.path(), &value).unwrap(),
            FileStatus::InSync
        );
    }
}
