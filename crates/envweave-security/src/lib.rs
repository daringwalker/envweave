#![forbid(unsafe_code)]

//! Path-boundary and sensitive-content policies.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecurityError {
    #[error("path escapes its allowed root: {0}")]
    PathEscape(PathBuf),
    #[error("target path must be absolute or start with ~/: {0}")]
    InvalidTarget(String),
}

pub fn repository_path(root: &Path, relative: &Path) -> Result<PathBuf, SecurityError> {
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(SecurityError::PathEscape(relative.to_path_buf()));
    }
    Ok(root.join(relative))
}

pub fn expand_target(home: &Path, target: &str) -> Result<PathBuf, SecurityError> {
    if target == "~" {
        return Ok(home.to_path_buf());
    }
    if let Some(relative) = target.strip_prefix("~/") {
        return repository_path(home, Path::new(relative));
    }
    let path = PathBuf::from(target);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(SecurityError::InvalidTarget(target.into()))
    }
}

pub fn sensitive_hint(path: &Path) -> bool {
    let value = path.to_string_lossy().to_ascii_lowercase();
    [
        "id_rsa",
        "id_ed25519",
        ".pem",
        ".key",
        "credentials",
        "keychain",
        ".gnupg",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

pub fn sensitive_content_hint(path: &Path) -> bool {
    fn inspect(path: &Path, remaining: &mut usize) -> bool {
        if *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
        if metadata.is_dir() {
            return fs::read_dir(path).is_ok_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| inspect(&entry.path(), remaining))
            });
        }
        if metadata.len() > 1024 * 1024 {
            return false;
        }
        let Ok(bytes) = fs::read(path) else {
            return false;
        };
        let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
        [
            "-----begin private key-----",
            "-----begin rsa private key-----",
            "-----begin openssh private key-----",
            "authorization: bearer ",
            "api_key=",
            "api-key=",
            "access_token=",
            "client_secret=",
            "password=",
        ]
        .iter()
        .any(|needle| text.contains(needle))
    }
    inspect(path, &mut 200)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expands_portable_home_target() {
        assert_eq!(
            expand_target(Path::new("/home/u"), "~/.zshrc").unwrap(),
            Path::new("/home/u/.zshrc")
        );
    }
    #[test]
    fn rejects_escape() {
        assert!(repository_path(Path::new("/repo"), Path::new("../x")).is_err());
    }
    #[test]
    fn flags_common_secret_names() {
        assert!(sensitive_hint(Path::new(".ssh/id_ed25519")));
    }
    #[test]
    fn flags_common_secret_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config");
        fs::write(&path, "client_secret=do-not-commit").unwrap();
        assert!(sensitive_content_hint(&path));
    }
}
