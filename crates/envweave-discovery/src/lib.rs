#![forbid(unsafe_code)]

//! Data-driven discovery of configuration files for installed applications.

use envweave_domain::Platform;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

const BUILTIN_KNOWLEDGE: &str = include_str!("../knowledge/configurations.toml");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KnowledgeBase {
    pub version: u32,
    #[serde(default)]
    pub applications: Vec<ApplicationKnowledge>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApplicationKnowledge {
    pub id: String,
    pub name: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub executables: Vec<String>,
    #[serde(default)]
    pub configs: Vec<ConfigKnowledge>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConfigKnowledge {
    pub id: String,
    pub path: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default = "default_true")]
    pub recommended: bool,
    #[serde(default)]
    pub description: String,
}

const fn default_true() -> bool {
    true
}

fn default_category() -> String {
    "other".into()
}

fn default_role() -> String {
    "config".into()
}

fn default_scope() -> String {
    "user".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeSource {
    Builtin,
    User,
}

#[derive(Debug, Clone)]
pub struct CatalogApplication {
    pub application: ApplicationKnowledge,
    pub source: KnowledgeSource,
}

#[derive(Debug, Clone)]
pub struct KnowledgeCatalog {
    pub applications: Vec<CatalogApplication>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryCandidate {
    pub id: String,
    pub application_id: String,
    pub application_name: String,
    pub path: PathBuf,
    pub target: String,
    pub role: String,
    pub scope: String,
    pub kind: CandidateKind,
    pub sensitive: bool,
    pub recommended: bool,
    pub description: String,
    pub managed: bool,
    pub detected_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewFile {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    File,
    Directory,
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("配置知识库无效：{0}")]
    InvalidKnowledge(String),
    #[error("找不到用户知识条目：{0}")]
    UserKnowledgeNotFound(String),
    #[error("configuration preview path is outside the knowledge base: {0}")]
    PreviewForbidden(PathBuf),
    #[error("cannot inspect configuration preview: {0}")]
    Io(#[from] std::io::Error),
}

impl KnowledgeBase {
    pub fn builtin() -> Result<Self, DiscoveryError> {
        let knowledge: Self = toml::from_str(BUILTIN_KNOWLEDGE)
            .map_err(|error| DiscoveryError::InvalidKnowledge(error.to_string()))?;
        if knowledge.version != 1 {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "unsupported version {}",
                knowledge.version
            )));
        }
        for application in &knowledge.applications {
            validate_application(application)?;
        }
        Ok(knowledge)
    }
}

pub fn load_catalog(user_directory: Option<&Path>) -> Result<KnowledgeCatalog, DiscoveryError> {
    let builtin = KnowledgeBase::builtin()?;
    let mut applications: Vec<CatalogApplication> = builtin
        .applications
        .into_iter()
        .map(|application| CatalogApplication {
            application,
            source: KnowledgeSource::Builtin,
        })
        .collect();
    let mut positions: HashMap<String, usize> = applications
        .iter()
        .enumerate()
        .map(|(index, item)| (item.application.id.clone(), index))
        .collect();
    let mut warnings = Vec::new();

    if let Some(directory) = user_directory.filter(|path| path.is_dir()) {
        let mut files = fs::read_dir(directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "toml")
            })
            .collect::<Vec<_>>();
        files.sort();
        for path in files {
            let loaded = fs::read_to_string(&path)
                .map_err(DiscoveryError::Io)
                .and_then(|text| parse_knowledge(&text));
            let knowledge = match loaded {
                Ok(value) => value,
                Err(error) => {
                    warnings.push(format!("{}：{error}", path.display()));
                    continue;
                }
            };
            for application in knowledge.applications {
                if let Err(error) = validate_application(&application) {
                    warnings.push(format!("{}：{error}", path.display()));
                    continue;
                }
                let item = CatalogApplication {
                    application: application.clone(),
                    source: KnowledgeSource::User,
                };
                if let Some(index) = positions.get(&application.id).copied() {
                    applications[index] = item;
                } else {
                    positions.insert(application.id.clone(), applications.len());
                    applications.push(item);
                }
            }
        }
    }
    applications.sort_by(|a, b| a.application.name.cmp(&b.application.name));
    Ok(KnowledgeCatalog {
        applications,
        warnings,
    })
}

pub fn save_user_application(
    directory: &Path,
    application: &ApplicationKnowledge,
) -> Result<(), DiscoveryError> {
    validate_application(application)?;
    fs::create_dir_all(directory)?;
    let knowledge = KnowledgeBase {
        version: 1,
        applications: vec![application.clone()],
    };
    let text = toml::to_string_pretty(&knowledge)
        .map_err(|error| DiscoveryError::InvalidKnowledge(error.to_string()))?;
    let destination = directory.join(format!("{}.toml", application.id));
    let temporary = directory.join(format!(".{}.toml.tmp", application.id));
    fs::write(&temporary, text)?;
    fs::rename(temporary, destination)?;
    Ok(())
}

pub fn delete_user_application(directory: &Path, id: &str) -> Result<(), DiscoveryError> {
    validate_id("应用 ID", id)?;
    let path = directory.join(format!("{id}.toml"));
    if !path.is_file() {
        return Err(DiscoveryError::UserKnowledgeNotFound(id.to_owned()));
    }
    fs::remove_file(path)?;
    Ok(())
}

fn parse_knowledge(text: &str) -> Result<KnowledgeBase, DiscoveryError> {
    let knowledge: KnowledgeBase = toml::from_str(text)
        .map_err(|error| DiscoveryError::InvalidKnowledge(error.to_string()))?;
    if knowledge.version != 1 {
        return Err(DiscoveryError::InvalidKnowledge(format!(
            "不支持版本 {}",
            knowledge.version
        )));
    }
    Ok(knowledge)
}

fn validate_application(application: &ApplicationKnowledge) -> Result<(), DiscoveryError> {
    validate_id("应用 ID", &application.id)?;
    if application.name.trim().is_empty() {
        return Err(DiscoveryError::InvalidKnowledge("应用名称不能为空".into()));
    }
    if application.category.trim().is_empty() {
        return Err(DiscoveryError::InvalidKnowledge("应用类别不能为空".into()));
    }
    if application.configs.is_empty() {
        return Err(DiscoveryError::InvalidKnowledge(
            "至少需要一个配置位置".into(),
        ));
    }
    let mut config_ids = HashSet::new();
    for config in &application.configs {
        validate_id("配置 ID", &config.id)?;
        if !config_ids.insert(&config.id) {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "配置 ID 重复：{}",
                config.id
            )));
        }
        if !(config.path.starts_with("~/") || Path::new(&config.path).is_absolute()) {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "配置路径必须以 ~/ 开头或使用绝对路径：{}",
                config.path
            )));
        }
        if !matches!(
            config.role.as_str(),
            "config" | "history" | "flags" | "state" | "extensions" | "credentials"
        ) {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "{} 包含不支持的用途：{}",
                config.id, config.role
            )));
        }
        if !matches!(config.scope.as_str(), "user" | "system") {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "{} 包含不支持的作用域：{}",
                config.id, config.scope
            )));
        }
        if config.scope == "system" && !Path::new(&config.path).is_absolute() {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "系统配置必须使用绝对路径：{}",
                config.path
            )));
        }
        if config
            .platforms
            .iter()
            .any(|platform| !matches!(platform.as_str(), "linux" | "macos"))
        {
            return Err(DiscoveryError::InvalidKnowledge(format!(
                "{} 包含不支持的平台",
                config.id
            )));
        }
    }
    Ok(())
}

fn validate_id(label: &str, id: &str) -> Result<(), DiscoveryError> {
    if id.is_empty()
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        return Err(DiscoveryError::InvalidKnowledge(format!(
            "{label} 只能包含小写字母、数字、连字符和下划线"
        )));
    }
    Ok(())
}

pub fn scan_system(
    home: &Path,
    platform: Platform,
    installed_packages: &HashSet<String>,
    managed_targets: &HashSet<String>,
    catalog: &KnowledgeCatalog,
) -> Result<Vec<DiscoveryCandidate>, DiscoveryError> {
    let mut path_entries: Vec<PathBuf> = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    path_entries.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/usr/bin",
            "/bin",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    path_entries.push(home.join(".local/bin"));
    path_entries.push(home.join(".cargo/bin"));
    let mut candidates = Vec::new();
    for catalog_application in &catalog.applications {
        let application = &catalog_application.application;
        let package_match = application
            .packages
            .iter()
            .any(|name| installed_packages.contains(&name.to_ascii_lowercase()));
        let executable_match = application
            .executables
            .iter()
            .any(|name| executable_exists(name, &path_entries));
        for config in application
            .configs
            .iter()
            .filter(|config| applies_to(config, platform))
        {
            let path = expand_home(home, &config.path);
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let mut detected_by = Vec::new();
            if package_match {
                detected_by.push("已安装软件包".into());
            }
            if executable_match {
                detected_by.push("可执行命令".into());
            }
            if detected_by.is_empty() {
                detected_by.push("发现遗留配置".into());
            }
            candidates.push(DiscoveryCandidate {
                id: format!("{}:{}", application.id, config.id),
                application_id: application.id.clone(),
                application_name: application.name.clone(),
                path,
                target: config.path.clone(),
                role: config.role.clone(),
                scope: config.scope.clone(),
                kind: if metadata.is_dir() {
                    CandidateKind::Directory
                } else {
                    CandidateKind::File
                },
                sensitive: config.sensitive,
                recommended: config.recommended,
                description: config.description.clone(),
                managed: managed_targets.contains(&config.path),
                detected_by,
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.application_name
            .cmp(&b.application_name)
            .then(a.target.cmp(&b.target))
    });
    Ok(candidates)
}

pub fn preview_files(
    home: &Path,
    platform: Platform,
    root: &Path,
    catalog: &KnowledgeCatalog,
) -> Result<Vec<PreviewFile>, DiscoveryError> {
    validate_known_root(home, platform, root, catalog)?;
    let canonical_root = fs::canonicalize(root)?;
    if canonical_root.is_file() {
        let metadata = fs::metadata(&canonical_root)?;
        return Ok(if probably_text(&canonical_root, metadata.len())? {
            vec![PreviewFile {
                path: root.to_path_buf(),
                relative_path: root.file_name().unwrap_or_default().into(),
                size: metadata.len() as usize,
            }]
        } else {
            vec![]
        });
    }
    let mut files = Vec::new();
    collect_preview_files(&canonical_root, &canonical_root, 0, &mut files)?;
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

pub fn validate_preview_file(
    home: &Path,
    platform: Platform,
    root: &Path,
    file: &Path,
    catalog: &KnowledgeCatalog,
) -> Result<PathBuf, DiscoveryError> {
    validate_known_root(home, platform, root, catalog)?;
    let canonical_root = fs::canonicalize(root)?;
    let canonical_file = fs::canonicalize(file)?;
    let allowed = if canonical_root.is_file() {
        canonical_file == canonical_root
    } else {
        canonical_file.starts_with(&canonical_root)
    };
    if !allowed || !canonical_file.is_file() {
        return Err(DiscoveryError::PreviewForbidden(file.to_path_buf()));
    }
    Ok(canonical_file)
}

fn validate_known_root(
    home: &Path,
    platform: Platform,
    root: &Path,
    catalog: &KnowledgeCatalog,
) -> Result<(), DiscoveryError> {
    let known = catalog
        .applications
        .iter()
        .flat_map(|application| &application.application.configs)
        .filter(|config| applies_to(config, platform))
        .any(|config| expand_home(home, &config.path) == root);
    if known {
        Ok(())
    } else {
        Err(DiscoveryError::PreviewForbidden(root.to_path_buf()))
    }
}

fn collect_preview_files(
    root: &Path,
    directory: &Path,
    depth: usize,
    files: &mut Vec<PreviewFile>,
) -> Result<(), DiscoveryError> {
    if depth > 6 || files.len() >= 200 {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        if files.len() >= 200 {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if [
                ".git",
                "node_modules",
                "cache",
                ".cache",
                "undo",
                "swap",
                "backup",
            ]
            .contains(&name.as_ref())
            {
                continue;
            }
            collect_preview_files(root, &path, depth + 1, files)?;
        } else {
            let Ok(canonical) = fs::canonicalize(&path) else {
                continue;
            };
            if !canonical.starts_with(root) || !canonical.is_file() {
                continue;
            }
            let size = fs::metadata(&canonical)?.len();
            if probably_text(&canonical, size)? {
                files.push(PreviewFile {
                    path,
                    relative_path: canonical
                        .strip_prefix(root)
                        .unwrap_or(&canonical)
                        .to_path_buf(),
                    size: size as usize,
                });
            }
        }
    }
    Ok(())
}

fn probably_text(path: &Path, size: u64) -> Result<bool, std::io::Error> {
    use std::io::Read;
    if size > 1024 * 1024 {
        return Ok(false);
    }
    let mut file = fs::File::open(path)?;
    let mut sample = [0_u8; 8192];
    let read = file.read(&mut sample)?;
    Ok(!sample[..read].contains(&0))
}

fn applies_to(config: &ConfigKnowledge, platform: Platform) -> bool {
    config.platforms.is_empty()
        || config
            .platforms
            .iter()
            .any(|value| value == platform.as_str())
}

fn expand_home(home: &Path, value: &str) -> PathBuf {
    value
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(value), |relative| home.join(relative))
}

fn executable_exists(name: &str, entries: &[PathBuf]) -> bool {
    entries.iter().any(|entry| entry.join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_knowledge_is_valid_and_unique() {
        let knowledge = KnowledgeBase::builtin().unwrap();
        assert!(knowledge.applications.len() >= 84);
        let ids: HashSet<_> = knowledge.applications.iter().map(|item| &item.id).collect();
        assert_eq!(ids.len(), knowledge.applications.len());
        let mut paths = HashSet::new();
        assert!(
            knowledge
                .applications
                .iter()
                .all(|item| item.configs.iter().all(|config| paths.insert(&config.path)))
        );
        assert!(
            knowledge
                .applications
                .iter()
                .all(|item| item.category != "other")
        );
        assert!(knowledge.applications.iter().any(|item| {
            item.id == "zsh"
                && item
                    .configs
                    .iter()
                    .any(|config| config.role == "history" && config.sensitive)
        }));
        assert!(knowledge.applications.iter().any(|item| {
            item.id == "chromium" && item.configs.iter().any(|config| config.role == "flags")
        }));
        for required in [
            "kde-plasma",
            "plasma-panel",
            "krunner",
            "konsole",
            "spectacle",
            "dolphin",
            "xfce",
            "lxqt",
            "hyprland",
            "sway",
            "xonsh",
        ] {
            assert!(
                knowledge
                    .applications
                    .iter()
                    .any(|item| item.id == required)
            );
        }
    }

    #[test]
    fn finds_existing_configs_and_marks_managed_and_sensitive() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".zshrc"), "export A=1").unwrap();
        fs::create_dir_all(home.path().join(".ssh")).unwrap();
        fs::write(home.path().join(".ssh/config"), "Host example").unwrap();
        let packages = HashSet::from(["zsh".to_owned()]);
        let managed = HashSet::from(["~/.zshrc".to_owned()]);
        let catalog = load_catalog(None).unwrap();
        let result =
            scan_system(home.path(), Platform::Macos, &packages, &managed, &catalog).unwrap();
        let zsh = result
            .iter()
            .find(|item| item.target == "~/.zshrc")
            .unwrap();
        assert!(zsh.managed);
        assert!(zsh.detected_by.contains(&"已安装软件包".to_owned()));
        assert!(
            result
                .iter()
                .find(|item| item.target == "~/.ssh/config")
                .unwrap()
                .sensitive
        );
    }

    #[test]
    fn filters_platform_specific_locations() {
        let home = tempfile::tempdir().unwrap();
        let mac = home.path().join("Library/Application Support/Code/User");
        fs::create_dir_all(&mac).unwrap();
        fs::write(mac.join("settings.json"), "{}").unwrap();
        let catalog = load_catalog(None).unwrap();
        assert!(
            scan_system(
                home.path(),
                Platform::Macos,
                &HashSet::new(),
                &HashSet::new(),
                &catalog
            )
            .unwrap()
            .iter()
            .any(|item| item.target.contains("Library/Application Support/Code"))
        );
        assert!(
            !scan_system(
                home.path(),
                Platform::Linux,
                &HashSet::new(),
                &HashSet::new(),
                &catalog
            )
            .unwrap()
            .iter()
            .any(|item| item.target.contains("Library/Application Support/Code"))
        );
    }

    #[test]
    fn previews_known_directory_but_rejects_escape() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".config/nvim");
        fs::create_dir_all(root.join("lua")).unwrap();
        fs::write(root.join("init.lua"), "print('ok')").unwrap();
        fs::write(root.join("lua/plugin.lua"), "return {}").unwrap();
        let catalog = load_catalog(None).unwrap();
        let files = preview_files(home.path(), Platform::Linux, &root, &catalog).unwrap();
        assert_eq!(files.len(), 2);
        assert!(
            validate_preview_file(
                home.path(),
                Platform::Linux,
                &root,
                &root.join("init.lua"),
                &catalog
            )
            .is_ok()
        );
        assert!(
            validate_preview_file(
                home.path(),
                Platform::Linux,
                &root,
                &home.path().join("outside"),
                &catalog
            )
            .is_err()
        );
    }

    #[test]
    fn saves_loads_overrides_and_deletes_user_knowledge() {
        let directory = tempfile::tempdir().unwrap();
        let application = ApplicationKnowledge {
            id: "zsh".into(),
            name: "My Zsh".into(),
            category: "shell".into(),
            packages: vec!["zsh".into()],
            executables: vec!["zsh".into()],
            configs: vec![ConfigKnowledge {
                id: "custom".into(),
                path: "~/.config/my-zsh".into(),
                role: "config".into(),
                scope: "user".into(),
                platforms: vec![],
                sensitive: false,
                recommended: true,
                description: "自定义 Zsh 配置".into(),
            }],
        };
        save_user_application(directory.path(), &application).unwrap();
        let catalog = load_catalog(Some(directory.path())).unwrap();
        let zsh = catalog
            .applications
            .iter()
            .find(|item| item.application.id == "zsh")
            .unwrap();
        assert_eq!(zsh.source, KnowledgeSource::User);
        assert_eq!(zsh.application.name, "My Zsh");

        delete_user_application(directory.path(), "zsh").unwrap();
        let restored = load_catalog(Some(directory.path())).unwrap();
        let zsh = restored
            .applications
            .iter()
            .find(|item| item.application.id == "zsh")
            .unwrap();
        assert_eq!(zsh.source, KnowledgeSource::Builtin);
        assert_eq!(zsh.application.name, "Zsh");
    }
}
