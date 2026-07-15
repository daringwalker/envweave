#![forbid(unsafe_code)]

use envweave_backup::Backup;
use envweave_diff::TextDocument;
use envweave_discovery::{ApplicationKnowledge, DiscoveryCandidate, KnowledgeCatalog, PreviewFile};
use envweave_domain::{DomainError, Platform, RepositoryLocation};
use envweave_files::FileStatus;
use envweave_git::{FileRevision, GitCli, GitStatus};
use envweave_manifest::{
    AdapterKind, ApplyStrategy, ConfigItem, ConfigScope, ItemConditions, Manifest, Portability,
};
use envweave_packages::{InstallAction, InstalledPackage, PackageIdentity};
pub use envweave_restore::{
    MachineFacts, RestoreDisposition, RestoreError, RestoreItemStatus, RestorePlan, RestoreRun,
    RestoreRunItem, RestoreRunStatus, RestoreStep,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSnapshot {
    pub app_name: &'static str,
    pub version: &'static str,
    pub platform: Platform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySummary {
    pub path: PathBuf,
    pub has_manifest: bool,
    pub has_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigItemStatus {
    pub item: ConfigItem,
    pub status: FileStatus,
}

#[derive(Debug, Clone)]
pub struct DiffSession {
    pub local: TextDocument,
    pub repository: TextDocument,
}

#[derive(Debug, Clone, Default)]
pub struct PackageScan {
    pub packages: Vec<InstalledPackage>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPackageDisposition {
    Ready,
    Review,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct MigrationPackageStep {
    pub package: PackageIdentity,
    pub action: Option<InstallAction>,
    pub disposition: MigrationPackageDisposition,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MigrationPlan {
    pub configuration: RestorePlan,
    pub packages: Vec<MigrationPackageStep>,
    pub package_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConfigurationDiscovery {
    pub candidates: Vec<DiscoveryCandidate>,
    pub package_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BulkAddResult {
    pub added: Vec<ConfigItemStatus>,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Manifest(#[from] envweave_manifest::ManifestError),
    #[error(transparent)]
    Files(#[from] envweave_files::FileError),
    #[error(transparent)]
    Backup(#[from] envweave_backup::BackupError),
    #[error(transparent)]
    Diff(#[from] envweave_diff::DiffError),
    #[error(transparent)]
    Git(#[from] envweave_git::GitError),
    #[error(transparent)]
    Packages(#[from] envweave_packages::PackageError),
    #[error(transparent)]
    Discovery(#[from] envweave_discovery::DiscoveryError),
    #[error(transparent)]
    Restore(#[from] envweave_restore::RestoreError),
    #[error("configuration item not found: {0}")]
    ItemNotFound(String),
    #[error("cannot determine the user home directory")]
    MissingHome,
    #[error("a discovery batch may contain at most 200 paths")]
    TooManyDiscoveryTargets,
    #[error("“{0}”是系统级配置；当前版本仅支持采集，恢复需要受控的管理员权限支持")]
    SystemPrivilegeRequired(String),
    #[error("不能管理该路径：{0}；请选择用户目录中的具体配置文件或子目录，并避开 EnvWeave 仓库")]
    UnsafeConfigTarget(String),
    #[error("“{0}”可能包含密码、令牌或私钥；确认风险后才能收集")]
    SensitiveConfirmationRequired(String),
}

#[derive(Debug, Default)]
pub struct AppService;

impl AppService {
    pub fn system_snapshot(&self) -> SystemSnapshot {
        SystemSnapshot {
            app_name: "EnvWeave",
            version: env!("CARGO_PKG_VERSION"),
            platform: Platform::current(),
        }
    }

    pub fn inspect_repository(&self, path: PathBuf) -> Result<RepositorySummary, DomainError> {
        let location = RepositoryLocation::from_existing_directory(path)?;
        Ok(RepositorySummary {
            path: location.as_path().to_path_buf(),
            has_manifest: location.as_path().join("envweave.toml").is_file(),
            has_git: location.as_path().join(".git").exists(),
        })
    }

    pub fn create_repository(
        &self,
        path: &Path,
        initialize_git: bool,
    ) -> Result<RepositorySummary, ApplicationError> {
        fs::create_dir_all(path.join("files")).map_err(envweave_manifest::ManifestError::Read)?;
        let manifest_path = path.join("envweave.toml");
        if manifest_path.exists() {
            Manifest::load(&manifest_path)?;
        } else {
            Manifest::default().save(&manifest_path)?;
        }
        if !path.join("packages.toml").exists() {
            fs::write(path.join("packages.toml"), "format_version = 2\n")
                .map_err(envweave_manifest::ManifestError::Read)?;
        }
        ensure_line(&path.join(".gitignore"), ".envweave-backups/")?;
        if initialize_git && !path.join(".git").exists() {
            GitCli::default().init(path)?;
        }
        ensure_backup_excluded(path)?;
        Ok(self.inspect_repository(path.to_path_buf())?)
    }

    pub fn list_config_items(
        &self,
        repository: &Path,
    ) -> Result<Vec<ConfigItemStatus>, ApplicationError> {
        let manifest = Manifest::load(&repository.join("envweave.toml"))?;
        let home = user_home()?;
        manifest
            .items
            .into_iter()
            .map(|item| {
                let status = envweave_files::scan(repository, &home, &item)?;
                Ok(ConfigItemStatus { item, status })
            })
            .collect()
    }

    pub fn add_config_item(
        &self,
        repository: &Path,
        target: &Path,
        allow_sensitive: bool,
    ) -> Result<ConfigItemStatus, ApplicationError> {
        let mut manifest = Manifest::load(&repository.join("envweave.toml"))?;
        let home = user_home()?;
        validate_config_target(repository, &home, target)?;
        let portable = target
            .strip_prefix(&home)
            .map(|path| format!("~/{}", path.display()))
            .unwrap_or_else(|_| target.display().to_string());
        if let Some(item) = manifest
            .items
            .iter()
            .find(|item| item.target == portable)
            .cloned()
        {
            let status = envweave_files::scan(repository, &home, &item)?;
            return Ok(ConfigItemStatus { item, status });
        }
        let display = target
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let base = slug(&display);
        let mut id = base.clone();
        let mut suffix = 2;
        while manifest.items.iter().any(|item| item.id == id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        let (portability, sensitive, tags) = classify_target(target, &home);
        if sensitive && !allow_sensitive {
            return Err(ApplicationError::SensitiveConfirmationRequired(
                target.display().to_string(),
            ));
        }
        let item = ConfigItem {
            id: id.clone(),
            application_id: base,
            name: display,
            source: PathBuf::from("files").join(&id),
            target: portable,
            kind: if target.is_dir() {
                envweave_manifest::ItemKind::Directory
            } else {
                envweave_manifest::ItemKind::File
            },
            adapter: AdapterKind::Filesystem,
            apply_strategy: ApplyStrategy::Replace,
            portability,
            scope: if target.starts_with(&home) {
                ConfigScope::User
            } else {
                ConfigScope::System
            },
            platforms: vec![Platform::current().as_str().into()],
            tags,
            conditions: ItemConditions::default(),
            dependencies: vec![],
            sensitive,
            exclude: vec![],
            validators: vec![],
            enabled: true,
        };
        manifest.items.push(item.clone());
        manifest.save(&repository.join("envweave.toml"))?;
        if let Err(error) = envweave_files::capture(repository, &home, &item) {
            manifest.items.retain(|entry| entry.id != id);
            let _ = manifest.save(&repository.join("envweave.toml"));
            return Err(error.into());
        }
        Ok(ConfigItemStatus {
            item,
            status: FileStatus::InSync,
        })
    }

    pub fn capture_item(&self, repository: &Path, item_id: &str) -> Result<(), ApplicationError> {
        let item = find_item(repository, item_id)?;
        envweave_files::capture(repository, &user_home()?, &item)?;
        Ok(())
    }
    pub fn remove_config_item(
        &self,
        repository: &Path,
        item_id: &str,
    ) -> Result<(), ApplicationError> {
        let mut manifest = Manifest::load(&repository.join("envweave.toml"))?;
        let item = manifest
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| ApplicationError::ItemNotFound(item_id.into()))?;
        manifest.items.retain(|item| item.id != item_id);
        manifest.save(&repository.join("envweave.toml"))?;
        let stored = envweave_security_path(repository, &item.source)?;
        if stored.is_dir() {
            fs::remove_dir_all(stored).map_err(envweave_manifest::ManifestError::Read)?;
        } else if stored.exists() {
            fs::remove_file(stored).map_err(envweave_manifest::ManifestError::Read)?;
        }
        Ok(())
    }
    pub fn apply_item(&self, repository: &Path, item_id: &str) -> Result<Backup, ApplicationError> {
        ensure_backup_excluded(repository)?;
        let item = find_item(repository, item_id)?;
        if item.scope == ConfigScope::System || Path::new(&item.target).is_absolute() {
            return Err(ApplicationError::SystemPrivilegeRequired(item.name));
        }
        let target = envweave_files::target_path(&user_home()?, &item)?;
        let backup = envweave_backup::create(&repository.join(".envweave-backups"), &target)?;
        if let Err(error) = envweave_files::apply(repository, &user_home()?, &item) {
            let _ = envweave_backup::restore(&backup);
            return Err(error.into());
        }
        Ok(backup)
    }
    pub fn open_diff(
        &self,
        repository: &Path,
        item_id: &str,
    ) -> Result<DiffSession, ApplicationError> {
        let item = find_item(repository, item_id)?;
        let local = envweave_diff::open_text(&envweave_files::target_path(&user_home()?, &item)?)?;
        let stored = envweave_security_path(repository, &item.source)?;
        let repository = envweave_diff::open_text(&stored)?;
        Ok(DiffSession { local, repository })
    }
    pub fn save_repository_text(
        &self,
        repository: &Path,
        item_id: &str,
        expected: &str,
        content: &str,
    ) -> Result<TextDocument, ApplicationError> {
        let item = find_item(repository, item_id)?;
        let path = envweave_security_path(repository, &item.source)?;
        let document = envweave_diff::open_text(&path)?;
        Ok(envweave_diff::save_text(&document, expected, content)?)
    }
    pub fn diff_history(
        &self,
        repository: &Path,
        item_id: &str,
    ) -> Result<Vec<FileRevision>, ApplicationError> {
        let item = find_item(repository, item_id)?;
        Ok(GitCli::default().file_history(repository, &item.source, 50)?)
    }
    pub fn open_repository_revision(
        &self,
        repository: &Path,
        item_id: &str,
        revision: &str,
    ) -> Result<TextDocument, ApplicationError> {
        let item = find_item(repository, item_id)?;
        let bytes = GitCli::default().file_at_revision(repository, revision, &item.source)?;
        Ok(envweave_diff::document_from_bytes(
            item.source,
            bytes,
            true,
        )?)
    }
    pub fn git_status(&self, repository: &Path) -> Result<GitStatus, ApplicationError> {
        Ok(GitCli::default().status(repository)?)
    }
    pub fn git_commit(
        &self,
        repository: &Path,
        message: &str,
    ) -> Result<GitStatus, ApplicationError> {
        let git = GitCli::default();
        git.commit_all(repository, message)?;
        Ok(git.status(repository)?)
    }
    pub fn git_fetch(&self, repository: &Path) -> Result<GitStatus, ApplicationError> {
        let git = GitCli::default();
        git.fetch(repository)?;
        Ok(git.status(repository)?)
    }
    pub fn git_pull(&self, repository: &Path) -> Result<GitStatus, ApplicationError> {
        let git = GitCli::default();
        git.pull_rebase(repository)?;
        Ok(git.status(repository)?)
    }
    pub fn git_push(&self, repository: &Path) -> Result<GitStatus, ApplicationError> {
        let git = GitCli::default();
        git.push(repository)?;
        Ok(git.status(repository)?)
    }
    pub fn git_set_origin(
        &self,
        repository: &Path,
        remote: &str,
    ) -> Result<GitStatus, ApplicationError> {
        let git = GitCli::default();
        git.set_origin(repository, remote)?;
        Ok(git.status(repository)?)
    }
    pub fn git_set_identity(
        &self,
        repository: &Path,
        name: &str,
        email: &str,
    ) -> Result<(), ApplicationError> {
        GitCli::default().set_identity(repository, name, email)?;
        Ok(())
    }
    pub fn clone_repository(
        &self,
        remote: &str,
        destination: &Path,
    ) -> Result<RepositorySummary, ApplicationError> {
        GitCli::default().clone(remote, destination)?;
        Ok(self.inspect_repository(destination.to_path_buf())?)
    }
    pub fn scan_packages(&self) -> PackageScan {
        let mut result = PackageScan::default();
        let scans: Vec<(
            &str,
            Result<Vec<InstalledPackage>, envweave_packages::PackageError>,
        )> = if Platform::current() == Platform::Macos {
            vec![
                ("Homebrew", envweave_packages::scan_homebrew()),
                ("Mac App Store", envweave_packages::scan_mas()),
            ]
        } else {
            let mut scans = vec![
                ("pacman", envweave_packages::scan_pacman()),
                ("Flatpak", envweave_packages::scan_flatpak()),
            ];
            if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
                scans.push((
                    "Desktop Entry",
                    envweave_packages::scan_desktop_applications(&home),
                ));
            }
            scans
        };
        for (name, scan) in scans {
            match scan {
                Ok(mut packages) => result.packages.append(&mut packages),
                Err(error) => result.warnings.push(format!("{name}: {error}")),
            }
        }
        result
    }
    pub fn scan_repository_packages(&self, repository: &Path) -> PackageScan {
        let mut current = self.scan_packages();
        let Ok(saved) = envweave_packages::load_manifest(&repository.join("packages.toml")) else {
            return current;
        };
        merge_recorded_package_sources(&mut current.packages, &saved.packages);
        current
    }
    pub fn discover_configurations(
        &self,
        repository: &Path,
        knowledge_directory: &Path,
    ) -> Result<ConfigurationDiscovery, ApplicationError> {
        let manifest = Manifest::load(&repository.join("envweave.toml"))?;
        let managed_targets: HashSet<_> =
            manifest.items.into_iter().map(|item| item.target).collect();
        let packages = self.scan_packages();
        let installed_names: HashSet<_> = packages
            .packages
            .iter()
            .map(|package| package.identity.name.to_ascii_lowercase())
            .collect();
        let catalog = envweave_discovery::load_catalog(Some(knowledge_directory))?;
        let candidates = envweave_discovery::scan_system(
            &user_home()?,
            Platform::current(),
            &installed_names,
            &managed_targets,
            &catalog,
        )?;
        let mut warnings = catalog.warnings;
        warnings.extend(packages.warnings.into_iter().filter(|warning| {
            !warning.starts_with("Mac App Store: package manager is not available: mas")
                && !warning.starts_with("Flatpak: package manager is not available: flatpak")
        }));
        Ok(ConfigurationDiscovery {
            candidates,
            package_count: packages.packages.len(),
            warnings,
        })
    }
    pub fn add_discovered_configurations(
        &self,
        repository: &Path,
        candidates: &[DiscoveryCandidate],
    ) -> Result<BulkAddResult, ApplicationError> {
        if candidates.len() > 200 {
            return Err(ApplicationError::TooManyDiscoveryTargets);
        }
        let existing: HashSet<_> = Manifest::load(&repository.join("envweave.toml"))?
            .items
            .into_iter()
            .map(|item| item.target)
            .collect();
        let home = user_home()?;
        let mut result = BulkAddResult::default();
        for candidate in candidates {
            let target = &candidate.path;
            let portable = target
                .strip_prefix(&home)
                .map(|path| format!("~/{}", path.display()))
                .unwrap_or_else(|_| target.display().to_string());
            if existing.contains(&portable) {
                result.skipped.push(portable);
                continue;
            }
            match self.add_config_item(repository, target, true) {
                Ok(mut status) => {
                    let mut manifest = Manifest::load(&repository.join("envweave.toml"))?;
                    if let Some(item) = manifest
                        .items
                        .iter_mut()
                        .find(|item| item.id == status.item.id)
                    {
                        item.application_id = candidate.application_id.clone();
                        item.sensitive |= candidate.sensitive;
                        if !item.tags.contains(&candidate.role) {
                            item.tags.push(candidate.role.clone());
                        }
                        status.item = item.clone();
                    }
                    manifest.save(&repository.join("envweave.toml"))?;
                    result.added.push(status);
                }
                Err(error) => result
                    .failed
                    .push((target.display().to_string(), error.to_string())),
            }
        }
        Ok(result)
    }
    pub fn configuration_preview_index(
        &self,
        root: &Path,
        knowledge_directory: &Path,
    ) -> Result<Vec<PreviewFile>, ApplicationError> {
        let catalog = envweave_discovery::load_catalog(Some(knowledge_directory))?;
        Ok(envweave_discovery::preview_files(
            &user_home()?,
            Platform::current(),
            root,
            &catalog,
        )?)
    }
    pub fn read_configuration_preview(
        &self,
        root: &Path,
        file: &Path,
        knowledge_directory: &Path,
    ) -> Result<TextDocument, ApplicationError> {
        let catalog = envweave_discovery::load_catalog(Some(knowledge_directory))?;
        let path = envweave_discovery::validate_preview_file(
            &user_home()?,
            Platform::current(),
            root,
            file,
            &catalog,
        )?;
        let mut document = envweave_diff::open_text(&path)?;
        document.read_only = true;
        Ok(document)
    }
    pub fn knowledge_catalog(
        &self,
        knowledge_directory: &Path,
    ) -> Result<KnowledgeCatalog, ApplicationError> {
        Ok(envweave_discovery::load_catalog(Some(knowledge_directory))?)
    }
    pub fn save_user_knowledge(
        &self,
        knowledge_directory: &Path,
        application: &ApplicationKnowledge,
    ) -> Result<KnowledgeCatalog, ApplicationError> {
        envweave_discovery::save_user_application(knowledge_directory, application)?;
        self.knowledge_catalog(knowledge_directory)
    }
    pub fn delete_user_knowledge(
        &self,
        knowledge_directory: &Path,
        id: &str,
    ) -> Result<KnowledgeCatalog, ApplicationError> {
        envweave_discovery::delete_user_application(knowledge_directory, id)?;
        self.knowledge_catalog(knowledge_directory)
    }
    pub fn save_package_inventory(
        &self,
        repository: &Path,
        packages: &[envweave_packages::PackageIdentity],
    ) -> Result<(), ApplicationError> {
        envweave_packages::save_manifest(&repository.join("packages.toml"), packages)?;
        Ok(())
    }
    pub fn package_install_plan(
        &self,
        repository: &Path,
        aur_helper: Option<&str>,
    ) -> Result<Vec<envweave_packages::InstallAction>, ApplicationError> {
        let desired = envweave_packages::load_manifest(&repository.join("packages.toml"))?.packages;
        let current = self.scan_packages();
        let missing = envweave_packages::missing(&desired, &current.packages);
        Ok(envweave_packages::plan_install(&missing, aur_helper)?)
    }
    pub fn restore_plan(&self, repository: &Path) -> Result<RestorePlan, ApplicationError> {
        let manifest = Manifest::load(&repository.join("envweave.toml"))?;
        let facts = envweave_restore::inspect_machine()?;
        Ok(envweave_restore::build_plan(repository, &manifest, facts))
    }
    pub fn migration_plan(
        &self,
        repository: &Path,
        aur_helper: Option<&str>,
    ) -> Result<MigrationPlan, ApplicationError> {
        let configuration = self.restore_plan(repository)?;
        let package_path = repository.join("packages.toml");
        if !package_path.is_file() {
            return Ok(MigrationPlan {
                configuration,
                packages: vec![],
                package_warnings: vec!["仓库中没有 packages.toml，软件准备阶段已跳过".into()],
            });
        }
        let desired = envweave_packages::load_manifest(&package_path)?.packages;
        let current = self.scan_packages();
        let missing = envweave_packages::missing(&desired, &current.packages);
        Ok(MigrationPlan {
            configuration,
            packages: build_package_migration_steps(&missing, aur_helper),
            package_warnings: current.warnings,
        })
    }
    pub fn execute_restore(
        &self,
        repository: &Path,
        plan_id: &str,
        selected_ids: &[String],
    ) -> Result<RestoreRun, ApplicationError> {
        ensure_backup_excluded(repository)?;
        let manifest = Manifest::load(&repository.join("envweave.toml"))?;
        let facts = envweave_restore::inspect_machine()?;
        let selected = selected_ids.iter().cloned().collect();
        Ok(envweave_restore::execute_plan(
            repository, &manifest, facts, plan_id, &selected,
        )?)
    }
    pub fn incomplete_restore_runs(
        &self,
        repository: &Path,
    ) -> Result<Vec<RestoreRun>, ApplicationError> {
        Ok(envweave_restore::list_incomplete_runs(repository)?)
    }
    pub fn recover_restore_run(
        &self,
        repository: &Path,
        run_id: &str,
    ) -> Result<RestoreRun, ApplicationError> {
        ensure_backup_excluded(repository)?;
        Ok(envweave_restore::recover_incomplete_run(
            repository,
            &user_home()?,
            run_id,
        )?)
    }
    pub fn keep_restore_run(
        &self,
        repository: &Path,
        run_id: &str,
    ) -> Result<RestoreRun, ApplicationError> {
        Ok(envweave_restore::keep_incomplete_run(repository, run_id)?)
    }
    pub fn install_package(
        &self,
        identity: &envweave_packages::PackageIdentity,
        aur_helper: Option<&str>,
    ) -> Result<(), ApplicationError> {
        let action =
            envweave_packages::plan_install(std::slice::from_ref(identity), aur_helper)?.remove(0);
        envweave_packages::execute_action(&action)?;
        Ok(())
    }
    pub fn list_backups(&self, repository: &Path) -> Result<Vec<Backup>, ApplicationError> {
        Ok(envweave_backup::list(
            &repository.join(".envweave-backups"),
        )?)
    }
    pub fn restore_backup(&self, repository: &Path, id: &str) -> Result<Backup, ApplicationError> {
        let root = repository.join(".envweave-backups");
        let backup = envweave_backup::load_id(&root, id)?;
        let home = user_home()?;
        envweave_restore::validate_recovery_target(repository, &home, &backup.original_path)?;
        envweave_backup::restore(&backup)?;
        Ok(backup)
    }
}

fn find_item(repository: &Path, id: &str) -> Result<ConfigItem, ApplicationError> {
    Manifest::load(&repository.join("envweave.toml"))?
        .items
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| ApplicationError::ItemNotFound(id.into()))
}

fn build_package_migration_steps(
    packages: &[PackageIdentity],
    aur_helper: Option<&str>,
) -> Vec<MigrationPackageStep> {
    packages
        .iter()
        .map(|package| {
            if package.provider == envweave_packages::ProviderKind::Portable {
                let mut reasons = vec!["便携应用需要人工下载并确认可执行文件位置".into()];
                if let Some(url) = package
                    .source
                    .download_url
                    .as_ref()
                    .or(package.source.page_url.as_ref())
                {
                    reasons.push(format!("记录的来源：{url}"));
                } else {
                    reasons.push("尚未记录下载来源，请在软件包页面补充".into());
                }
                return MigrationPackageStep {
                    package: package.clone(),
                    action: None,
                    disposition: MigrationPackageDisposition::Review,
                    reasons,
                };
            }
            match envweave_packages::plan_install(std::slice::from_ref(package), aur_helper) {
                Ok(mut actions) => {
                    let action = actions.remove(0);
                    let environment = envweave_packages::validate_action_environment(&action);
                    let (disposition, reasons) = match environment {
                        Err(error) => (
                            MigrationPackageDisposition::Blocked,
                            vec![error.to_string()],
                        ),
                        Ok(()) if action.third_party => (
                            MigrationPackageDisposition::Review,
                            vec!["来自第三方软件源，需要明确确认".into()],
                        ),
                        Ok(()) if action.requires_privilege => (
                            MigrationPackageDisposition::Review,
                            vec!["安装时需要系统授权".into()],
                        ),
                        Ok(()) => (
                            MigrationPackageDisposition::Ready,
                            vec!["当前机器可以安装".into()],
                        ),
                    };
                    MigrationPackageStep {
                        package: package.clone(),
                        action: Some(action),
                        disposition,
                        reasons,
                    }
                }
                Err(error) => MigrationPackageStep {
                    package: package.clone(),
                    action: None,
                    disposition: MigrationPackageDisposition::Blocked,
                    reasons: vec![error.to_string()],
                },
            }
        })
        .collect()
}
fn user_home() -> Result<PathBuf, ApplicationError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(ApplicationError::MissingHome)
}

fn merge_recorded_package_sources(
    installed: &mut [InstalledPackage],
    recorded: &[envweave_packages::PackageIdentity],
) {
    for installed in installed {
        let Some(recorded) = recorded.iter().find(|recorded| {
            recorded.provider == installed.identity.provider
                && recorded.kind == installed.identity.kind
                && recorded.app_id.as_ref().unwrap_or(&recorded.name)
                    == installed
                        .identity
                        .app_id
                        .as_ref()
                        .unwrap_or(&installed.identity.name)
        }) else {
            continue;
        };
        installed.identity.source.page_url = recorded.source.page_url.clone();
        installed.identity.source.download_url = recorded.source.download_url.clone();
        if installed.identity.source.repository.is_none() {
            installed.identity.source.repository = recorded.source.repository.clone();
        }
    }
}

fn validate_config_target(
    repository: &Path,
    home: &Path,
    target: &Path,
) -> Result<(), ApplicationError> {
    let canonical_target =
        fs::canonicalize(target).map_err(envweave_manifest::ManifestError::Read)?;
    let canonical_home = fs::canonicalize(home).map_err(envweave_manifest::ManifestError::Read)?;
    let canonical_repository =
        fs::canonicalize(repository).map_err(envweave_manifest::ManifestError::Read)?;
    let lexical_user_path = target.starts_with(home);
    let escaped_home = lexical_user_path && !canonical_target.starts_with(&canonical_home);
    let overlaps_repository = canonical_target.starts_with(&canonical_repository)
        || canonical_repository.starts_with(&canonical_target);
    if canonical_target == canonical_home || escaped_home || overlaps_repository {
        return Err(ApplicationError::UnsafeConfigTarget(
            target.display().to_string(),
        ));
    }
    Ok(())
}

fn ensure_backup_excluded(repository: &Path) -> Result<(), ApplicationError> {
    ensure_line(&repository.join(".gitignore"), ".envweave-backups/")?;
    let git_exclude = repository.join(".git/info/exclude");
    if git_exclude.parent().is_some_and(Path::is_dir) {
        ensure_line(&git_exclude, ".envweave-backups/")?;
    }
    Ok(())
}

fn ensure_line(path: &Path, line: &str) -> Result<(), ApplicationError> {
    let mut contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(envweave_manifest::ManifestError::Read(error).into()),
    };
    if contents.lines().any(|existing| existing.trim() == line) {
        return Ok(());
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(line);
    contents.push('\n');
    fs::write(path, contents).map_err(envweave_manifest::ManifestError::Read)?;
    Ok(())
}
fn envweave_security_path(root: &Path, relative: &Path) -> Result<PathBuf, ApplicationError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(
            envweave_manifest::ManifestError::UnsafeSource(relative.display().to_string()).into(),
        );
    }
    Ok(root.join(relative))
}

fn slug(value: &str) -> String {
    let text: String = value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    let text = text.trim_matches('-');
    if text.is_empty() {
        "config".into()
    } else {
        text.into()
    }
}

fn classify_target(target: &Path, home: &Path) -> (Portability, bool, Vec<String>) {
    let portable = target.strip_prefix(home).ok();
    let normalized = target
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let machine_bound = [
        "/etc/fstab",
        "/etc/crypttab",
        "/etc/hostname",
        "/etc/machine-id",
        "/etc/network/",
        "/etc/systemd/network/",
        "/etc/networkmanager/system-connections/",
        "/.local/share/kscreen",
        "/.config/kdeconnect",
    ]
    .iter()
    .any(|pattern| {
        normalized == *pattern || normalized.starts_with(pattern) || normalized.contains(pattern)
    });
    let sensitive = envweave_security::sensitive_hint(target)
        || envweave_security::sensitive_content_hint(target)
        || normalized.contains("system-connections")
        || normalized.contains("kdeconnect");
    let portability = if machine_bound {
        Portability::MachineBound
    } else if portable.is_some() {
        Portability::Portable
    } else {
        Portability::Review
    };
    let mut tags = Vec::new();
    if machine_bound {
        tags.push("machine-bound".into());
    }
    if sensitive {
        tags.push("sensitive".into());
    }
    (portability, sensitive, tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_ui_independent() {
        let snapshot = AppService.system_snapshot();
        assert_eq!(snapshot.app_name, "EnvWeave");
        assert!(!snapshot.version.is_empty());
    }

    #[test]
    fn creates_a_valid_repository() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("repo");
        let summary = AppService.create_repository(&path, true).unwrap();
        assert!(summary.has_manifest && summary.has_git && path.join("files").is_dir());
        assert!(
            fs::read_to_string(path.join(".gitignore"))
                .unwrap()
                .contains(".envweave-backups/")
        );
        assert!(
            fs::read_to_string(path.join(".git/info/exclude"))
                .unwrap()
                .contains(".envweave-backups/")
        );
        assert_eq!(
            envweave_packages::load_manifest(&path.join("packages.toml"))
                .unwrap()
                .format_version,
            2
        );
    }

    #[test]
    fn keeps_user_recorded_download_sources_across_package_scans() {
        use envweave_packages::{PackageIdentity, PackageSource, ProviderKind};
        let identity = PackageIdentity {
            provider: ProviderKind::Portable,
            kind: "appimage".into(),
            name: "Example".into(),
            app_id: Some("example".into()),
            source: PackageSource {
                executable_path: Some("/home/test/Applications/Example.AppImage".into()),
                ..PackageSource::default()
            },
        };
        let mut installed = vec![InstalledPackage {
            identity: identity.clone(),
            version: None,
            explicit: true,
        }];
        let mut recorded = identity;
        recorded.source.page_url = Some("https://example.com/download".into());
        recorded.source.download_url = Some("https://example.com/Example.AppImage".into());

        merge_recorded_package_sources(&mut installed, &[recorded]);

        assert_eq!(
            installed[0].identity.source.page_url.as_deref(),
            Some("https://example.com/download")
        );
        assert_eq!(
            installed[0].identity.source.executable_path.as_deref(),
            Some("/home/test/Applications/Example.AppImage")
        );
    }

    #[test]
    fn classifies_hardware_identity_and_user_configs() {
        let home = Path::new("/home/test");
        assert_eq!(
            classify_target(Path::new("/home/test/.zshrc"), home).0,
            Portability::Portable
        );
        let network = classify_target(
            Path::new("/etc/NetworkManager/system-connections/home.nmconnection"),
            home,
        );
        assert_eq!(network.0, Portability::MachineBound);
        assert!(network.1);
        assert_eq!(
            classify_target(Path::new("/etc/fstab"), home).0,
            Portability::MachineBound
        );
    }

    #[test]
    fn rejects_whole_home_and_repository_overlap() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let repository = home.join("envweave-repository");
        fs::create_dir_all(&repository).unwrap();
        assert!(validate_config_target(&repository, &home, &home).is_err());
        assert!(validate_config_target(&repository, &home, &repository).is_err());
        assert!(validate_config_target(&repository, &home, directory.path()).is_err());
    }

    #[test]
    fn migration_plan_keeps_an_unconfigured_aur_package_as_a_blocked_step() {
        let package = PackageIdentity {
            provider: envweave_packages::ProviderKind::Aur,
            kind: "foreign".into(),
            name: "example-aur-package".into(),
            app_id: None,
            source: envweave_packages::PackageSource::default(),
        };

        let steps = build_package_migration_steps(&[package], None);

        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].disposition, MigrationPackageDisposition::Blocked);
        assert!(steps[0].action.is_none());
        assert!(steps[0].reasons[0].contains("paru or yay"));
    }
}
