#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Macos,
    Unsupported,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unsupported
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryLocation(PathBuf);

impl RepositoryLocation {
    pub fn from_existing_directory(path: impl Into<PathBuf>) -> Result<Self, DomainError> {
        let path = path.into();
        if !path.is_dir() {
            return Err(DomainError::RepositoryIsNotDirectory(path));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("repository path is not a directory: {0}")]
    RepositoryIsNotDirectory(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_platform_has_stable_name() {
        assert!(matches!(
            Platform::current().as_str(),
            "linux" | "macos" | "unsupported"
        ));
    }

    #[test]
    fn repository_rejects_missing_path() {
        let path = PathBuf::from("this-path-must-not-exist-envweave");
        assert!(RepositoryLocation::from_existing_directory(path).is_err());
    }
}
