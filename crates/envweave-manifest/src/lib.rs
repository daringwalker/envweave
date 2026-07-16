#![forbid(unsafe_code)]

//! Versioned, deterministic EnvWeave manifest storage.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

pub const CURRENT_FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub format_version: u32,
    #[serde(default)]
    pub items: Vec<ConfigItem>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigScope {
    #[default]
    User,
    System,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterKind {
    #[default]
    Filesystem,
    GnomeDconf,
    MacosDefaults,
    Systemd,
    Launchd,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyStrategy {
    #[default]
    Replace,
    Merge,
    KeepExisting,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Portability {
    #[default]
    Portable,
    Review,
    MachineBound,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemConditions {
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub distributions: Vec<String>,
    #[serde(default)]
    pub desktops: Vec<String>,
    #[serde(default)]
    pub required_packages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigItem {
    pub id: String,
    pub application_id: String,
    pub name: String,
    pub source: PathBuf,
    pub target: String,
    pub kind: ItemKind,
    #[serde(default)]
    pub adapter: AdapterKind,
    #[serde(default)]
    pub apply_strategy: ApplyStrategy,
    #[serde(default)]
    pub portability: Portability,
    #[serde(default)]
    pub scope: ConfigScope,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub conditions: ItemConditions,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub validators: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("cannot read manifest: {0}")]
    Read(#[from] std::io::Error),
    #[error("invalid manifest TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("cannot encode manifest: {0}")]
    Encode(#[from] toml::ser::Error),
    #[error("仓库清单版本 {0} 不受支持；当前开发版本使用 Manifest v2，请重新初始化测试仓库")]
    UnsupportedVersion(u32),
    #[error("duplicate item id: {0}")]
    DuplicateId(String),
    #[error("item {0} has an unsafe repository source")]
    UnsafeSource(String),
    #[error("item {0} has an invalid target")]
    InvalidTarget(String),
    #[error("item {0} has an unsafe excluded path: {1}")]
    UnsafeExclude(String, String),
    #[error("file item {0} can only use replace without excluded child paths")]
    InvalidFilePolicy(String),
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let manifest: Self = toml::from_str(&fs::read_to_string(path)?)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<(), ManifestError> {
        self.validate()?;
        let mut stable = self.clone();
        stable.items.sort_by(|a, b| a.id.cmp(&b.id));
        let text = toml::to_string_pretty(&stable)?;
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, text)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.format_version != CURRENT_FORMAT_VERSION {
            return Err(ManifestError::UnsupportedVersion(self.format_version));
        }
        let mut ids = HashSet::new();
        for item in &self.items {
            if item.id.trim().is_empty() || !ids.insert(item.id.clone()) {
                return Err(ManifestError::DuplicateId(item.id.clone()));
            }
            if item.source.is_absolute()
                || item.source.components().any(|c| {
                    matches!(
                        c,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(ManifestError::UnsafeSource(item.id.clone()));
            }
            if item.target.trim().is_empty() || item.target.contains('\0') {
                return Err(ManifestError::InvalidTarget(item.id.clone()));
            }
            if item.scope == ConfigScope::User
                && (!item.target.starts_with("~/") || matches!(item.target.as_str(), "~/" | "~"))
            {
                return Err(ManifestError::InvalidTarget(item.id.clone()));
            }
            if item.application_id.trim().is_empty() {
                return Err(ManifestError::InvalidTarget(item.id.clone()));
            }
            if item.scope == ConfigScope::System && !Path::new(&item.target).is_absolute() {
                return Err(ManifestError::InvalidTarget(item.id.clone()));
            }
            if item.kind == ItemKind::File
                && (item.apply_strategy != ApplyStrategy::Replace || !item.exclude.is_empty())
            {
                return Err(ManifestError::InvalidFilePolicy(item.id.clone()));
            }
            for excluded in &item.exclude {
                let path = Path::new(excluded.trim_end_matches('/'));
                if excluded.trim().is_empty()
                    || path.is_absolute()
                    || path
                        .components()
                        .any(|component| !matches!(component, Component::Normal(_)))
                {
                    return Err(ManifestError::UnsafeExclude(
                        item.id.clone(),
                        excluded.clone(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, source: &str) -> ConfigItem {
        ConfigItem {
            id: id.into(),
            application_id: "shell.zsh".into(),
            name: id.into(),
            source: source.into(),
            target: "~/.zshrc".into(),
            kind: ItemKind::File,
            adapter: AdapterKind::Filesystem,
            apply_strategy: ApplyStrategy::Replace,
            portability: Portability::Portable,
            scope: ConfigScope::User,
            platforms: vec!["macos".into()],
            tags: vec![],
            conditions: ItemConditions::default(),
            dependencies: vec![],
            sensitive: false,
            exclude: vec![],
            validators: vec![],
            enabled: true,
        }
    }

    #[test]
    fn round_trip_is_stably_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("envweave.toml");
        let manifest = Manifest {
            format_version: CURRENT_FORMAT_VERSION,
            items: vec![item("z", "files/z"), item("a", "files/a")],
        };
        manifest.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.find("id = \"a\"").unwrap() < text.find("id = \"z\"").unwrap());
        assert_eq!(Manifest::load(&path).unwrap().items[0].id, "a");
    }

    #[test]
    fn rejects_parent_traversal_and_duplicates() {
        let mut manifest = Manifest {
            format_version: CURRENT_FORMAT_VERSION,
            items: vec![item("x", "../secret")],
        };
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::UnsafeSource(_))
        ));
        manifest.items = vec![item("x", "files/a"), item("x", "files/b")];
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::DuplicateId(_))
        ));

        let mut absolute_user = item("absolute", "files/absolute");
        absolute_user.target = "/tmp/absolute".into();
        manifest.items = vec![absolute_user];
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::InvalidTarget(_))
        ));

        let mut whole_home = item("home", "files/home");
        whole_home.target = "~/".into();
        manifest.items = vec![whole_home];
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::InvalidTarget(_))
        ));
    }

    #[test]
    fn rejects_unsafe_excluded_paths() {
        let mut value = item("unsafe-exclude", "files/value");
        value.kind = ItemKind::Directory;
        value.exclude = vec!["../outside".into()];
        let manifest = Manifest {
            format_version: CURRENT_FORMAT_VERSION,
            items: vec![value],
        };
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::UnsafeExclude(_, _))
        ));
    }

    #[test]
    fn rejects_directory_only_policy_on_a_file() {
        let mut value = item("file-policy", "files/value");
        value.apply_strategy = ApplyStrategy::Merge;
        let manifest = Manifest {
            format_version: CURRENT_FORMAT_VERSION,
            items: vec![value],
        };
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::InvalidFilePolicy(_))
        ));
    }
}
