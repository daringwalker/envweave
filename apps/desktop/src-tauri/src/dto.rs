use envweave_application::{
    ApplicationError, BulkAddResult, ConfigItemStatus, ConfigurationDiscovery, DiffSession,
    MachineFacts, MigrationPackageDisposition, MigrationPackageStep, MigrationPlan, PackageScan,
    RepositorySummary, RestoreDisposition, RestoreError, RestoreItemStatus, RestorePlan,
    RestoreRun, RestoreRunItem, RestoreRunStatus, RestoreStep, SystemSnapshot,
};
use envweave_backup::Backup;
use envweave_diff::{LineEnding, TextDocument};
use envweave_discovery::{
    ApplicationKnowledge, CandidateKind, ConfigKnowledge, DiscoveryCandidate, KnowledgeCatalog,
    KnowledgeSource, PreviewFile,
};
use envweave_files::FileStatus;
use envweave_git::{FileRevision, GitStatus};
use envweave_packages::{
    InstallAction, InstalledPackage, PackageIdentity, PackageSource, ProviderKind,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct AppSnapshotDto {
    pub app_name: String,
    pub version: String,
    pub platform: String,
}

impl From<SystemSnapshot> for AppSnapshotDto {
    fn from(value: SystemSnapshot) -> Self {
        Self {
            app_name: value.app_name.to_owned(),
            version: value.version.to_owned(),
            platform: value.platform.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RepositoryInspectionDto {
    pub path: String,
    pub has_manifest: bool,
    pub has_git: bool,
}

impl From<RepositorySummary> for RepositoryInspectionDto {
    fn from(value: RepositorySummary) -> Self {
        Self {
            path: value.path.to_string_lossy().into_owned(),
            has_manifest: value.has_manifest,
            has_git: value.has_git,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        let code = match &error {
            ApplicationError::SensitiveConfirmationRequired(_) => {
                "config.sensitive_confirmation_required"
            }
            ApplicationError::Restore(RestoreError::PlanChanged) => "restore.plan_changed",
            ApplicationError::Restore(RestoreError::InvalidSelection(_)) => {
                "restore.invalid_selection"
            }
            ApplicationError::Restore(RestoreError::RunNotIncomplete(_)) => {
                "restore.transaction_already_finished"
            }
            ApplicationError::Restore(
                RestoreError::InvalidRunId | RestoreError::RunNotFound(_),
            ) => "restore.invalid_transaction",
            ApplicationError::Restore(RestoreError::UnsafeRecoveryTarget(_)) => {
                "restore.unsafe_recovery_target"
            }
            ApplicationError::Restore(RestoreError::UnresolvedTransactions) => {
                "restore.unresolved_transactions"
            }
            _ => "operation.failed",
        };
        Self {
            code: code.into(),
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct OperationDto {
    pub message: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ToolFactDto {
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MachineFactsDto {
    pub os: String,
    pub distribution: String,
    pub distribution_version: String,
    pub architecture: String,
    pub desktop: String,
    pub shell: String,
    pub home: String,
    pub privilege_tool: Option<String>,
    pub tools: Vec<ToolFactDto>,
}
impl From<MachineFacts> for MachineFactsDto {
    fn from(value: MachineFacts) -> Self {
        Self {
            os: value.os,
            distribution: value.distribution,
            distribution_version: value.distribution_version,
            architecture: value.architecture,
            desktop: value.desktop,
            shell: value.shell,
            home: value.home.to_string_lossy().into_owned(),
            privilege_tool: value.privilege_tool,
            tools: value
                .tools
                .into_iter()
                .map(|tool| ToolFactDto {
                    name: tool.name,
                    available: tool.available,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RestoreStepDto {
    pub id: String,
    pub application_id: String,
    pub name: String,
    pub target: String,
    pub disposition: String,
    pub reasons: Vec<String>,
    pub dependencies: Vec<String>,
}
fn disposition(value: RestoreDisposition) -> String {
    match value {
        RestoreDisposition::Ready => "ready",
        RestoreDisposition::Review => "review",
        RestoreDisposition::Skipped => "skipped",
        RestoreDisposition::Inapplicable => "inapplicable",
        RestoreDisposition::Blocked => "blocked",
    }
    .into()
}
impl From<RestoreStep> for RestoreStepDto {
    fn from(value: RestoreStep) -> Self {
        Self {
            id: value.id,
            application_id: value.application_id,
            name: value.name,
            target: value.target,
            disposition: disposition(value.disposition),
            reasons: value.reasons,
            dependencies: value.dependencies,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RestoreCountsDto {
    pub ready: usize,
    pub review: usize,
    pub skipped: usize,
    pub inapplicable: usize,
    pub blocked: usize,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RestorePlanDto {
    pub id: String,
    pub facts: MachineFactsDto,
    pub steps: Vec<RestoreStepDto>,
    pub counts: RestoreCountsDto,
}
impl From<RestorePlan> for RestorePlanDto {
    fn from(value: RestorePlan) -> Self {
        let count = |kind| value.counts.get(&kind).copied().unwrap_or_default();
        Self {
            id: value.id,
            facts: value.facts.into(),
            steps: value.steps.into_iter().map(Into::into).collect(),
            counts: RestoreCountsDto {
                ready: count(RestoreDisposition::Ready),
                review: count(RestoreDisposition::Review),
                skipped: count(RestoreDisposition::Skipped),
                inapplicable: count(RestoreDisposition::Inapplicable),
                blocked: count(RestoreDisposition::Blocked),
            },
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MigrationPackageStepDto {
    pub package: PackageDto,
    pub action: Option<InstallActionDto>,
    pub disposition: String,
    pub reasons: Vec<String>,
}
impl From<MigrationPackageStep> for MigrationPackageStepDto {
    fn from(value: MigrationPackageStep) -> Self {
        let package = InstalledPackage {
            identity: value.package,
            version: None,
            explicit: true,
        }
        .into();
        Self {
            package,
            action: value.action.map(Into::into),
            disposition: match value.disposition {
                MigrationPackageDisposition::Ready => "ready",
                MigrationPackageDisposition::Review => "review",
                MigrationPackageDisposition::Blocked => "blocked",
            }
            .into(),
            reasons: value.reasons,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct MigrationPlanDto {
    pub configuration: RestorePlanDto,
    pub packages: Vec<MigrationPackageStepDto>,
    pub package_warnings: Vec<String>,
}
impl From<MigrationPlan> for MigrationPlanDto {
    fn from(value: MigrationPlan) -> Self {
        Self {
            configuration: value.configuration.into(),
            packages: value.packages.into_iter().map(Into::into).collect(),
            package_warnings: value.package_warnings,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RestoreRunItemDto {
    pub item_id: String,
    pub name: String,
    pub target: String,
    pub status: String,
    pub backup_id: Option<String>,
    pub message: String,
}
impl From<RestoreRunItem> for RestoreRunItemDto {
    fn from(value: RestoreRunItem) -> Self {
        Self {
            item_id: value.item_id,
            name: value.name,
            target: value.target,
            status: match value.status {
                RestoreItemStatus::Prepared => "prepared",
                RestoreItemStatus::Applied => "applied",
                RestoreItemStatus::Skipped => "skipped",
                RestoreItemStatus::Failed => "failed",
                RestoreItemStatus::RolledBack => "rolled-back",
                RestoreItemStatus::RollbackFailed => "rollback-failed",
            }
            .into(),
            backup_id: value.backup_id,
            message: value.message,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct RestoreRunDto {
    pub id: String,
    pub created_at_epoch_ms: String,
    pub status: String,
    pub items: Vec<RestoreRunItemDto>,
}
impl From<RestoreRun> for RestoreRunDto {
    fn from(value: RestoreRun) -> Self {
        Self {
            id: value.id,
            created_at_epoch_ms: value.created_at_epoch_ms.to_string(),
            status: match value.status {
                RestoreRunStatus::Running => "running",
                RestoreRunStatus::Completed => "completed",
                RestoreRunStatus::RolledBack => "rolled-back",
                RestoreRunStatus::RollbackFailed => "rollback-failed",
                RestoreRunStatus::KeptCurrent => "kept-current",
            }
            .into(),
            items: value.items.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ConfigItemDto {
    pub id: String,
    pub name: String,
    pub source: String,
    pub target: String,
    pub kind: String,
    pub scope: String,
    pub portability: String,
    pub sensitive: bool,
    pub status: String,
    pub enabled: bool,
    pub tags: Vec<String>,
}
impl From<ConfigItemStatus> for ConfigItemDto {
    fn from(value: ConfigItemStatus) -> Self {
        Self {
            id: value.item.id,
            name: value.item.name,
            source: value.item.source.to_string_lossy().into_owned(),
            target: value.item.target,
            kind: match value.item.kind {
                envweave_manifest::ItemKind::File => "file",
                envweave_manifest::ItemKind::Directory => "directory",
            }
            .into(),
            scope: match value.item.scope {
                envweave_manifest::ConfigScope::User => "user",
                envweave_manifest::ConfigScope::System => "system",
            }
            .into(),
            portability: match value.item.portability {
                envweave_manifest::Portability::Portable => "portable",
                envweave_manifest::Portability::Review => "review",
                envweave_manifest::Portability::MachineBound => "machine-bound",
            }
            .into(),
            sensitive: value.item.sensitive,
            status: match value.status {
                FileStatus::InSync => "in-sync",
                FileStatus::Modified => "modified",
                FileStatus::MissingTarget => "missing-target",
                FileStatus::MissingRepositoryCopy => "missing-repository",
                FileStatus::TypeMismatch => "type-mismatch",
            }
            .into(),
            enabled: value.item.enabled,
            tags: value.item.tags,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct TextDocumentDto {
    pub path: String,
    pub content: String,
    pub revision: String,
    pub line_ending: String,
    pub read_only: bool,
}
impl From<TextDocument> for TextDocumentDto {
    fn from(value: TextDocument) -> Self {
        Self {
            path: value.path.to_string_lossy().into_owned(),
            content: value.content,
            revision: value.revision,
            line_ending: match value.line_ending {
                LineEnding::Lf => "LF",
                LineEnding::CrLf => "CRLF",
            }
            .into(),
            read_only: value.read_only,
        }
    }
}
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DiffSessionDto {
    pub local: TextDocumentDto,
    pub repository: TextDocumentDto,
}
impl From<DiffSession> for DiffSessionDto {
    fn from(value: DiffSession) -> Self {
        Self {
            local: value.local.into(),
            repository: value.repository.into(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FileRevisionDto {
    pub commit: String,
    pub short_commit: String,
    pub authored_at: String,
    pub author: String,
    pub subject: String,
}
impl From<FileRevision> for FileRevisionDto {
    fn from(value: FileRevision) -> Self {
        Self {
            commit: value.commit,
            short_commit: value.short_commit,
            authored_at: value.authored_at,
            author: value.author,
            subject: value.subject,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PackageDto {
    pub provider: String,
    pub kind: String,
    pub name: String,
    pub app_id: Option<String>,
    pub version: Option<String>,
    pub source_page: Option<String>,
    pub download_url: Option<String>,
    pub executable_path: Option<String>,
    pub desktop_file: Option<String>,
    pub repository: Option<String>,
}
impl PackageDto {
    pub fn identity(&self) -> Result<PackageIdentity, ApiError> {
        let provider = match self.provider.as_str() {
            "pacman" => ProviderKind::Pacman,
            "aur" => ProviderKind::Aur,
            "brew" => ProviderKind::Homebrew,
            "mas" => ProviderKind::MacAppStore,
            "flatpak" => ProviderKind::Flatpak,
            "portable" => ProviderKind::Portable,
            _ => {
                return Err(ApiError {
                    code: "package.invalid_provider".into(),
                    message: format!("未知软件源 {}", self.provider),
                });
            }
        };
        Ok(PackageIdentity {
            provider,
            kind: self.kind.clone(),
            name: self.name.clone(),
            app_id: self.app_id.clone(),
            source: PackageSource {
                page_url: self.source_page.clone(),
                download_url: self.download_url.clone(),
                executable_path: self.executable_path.clone(),
                desktop_file: self.desktop_file.clone(),
                repository: self.repository.clone(),
            },
        })
    }
}
impl From<InstalledPackage> for PackageDto {
    fn from(value: InstalledPackage) -> Self {
        Self {
            provider: match value.identity.provider {
                ProviderKind::Pacman => "pacman",
                ProviderKind::Aur => "aur",
                ProviderKind::Homebrew => "brew",
                ProviderKind::MacAppStore => "mas",
                ProviderKind::Flatpak => "flatpak",
                ProviderKind::Portable => "portable",
            }
            .into(),
            kind: value.identity.kind,
            name: value.identity.name,
            app_id: value.identity.app_id,
            version: value.version,
            source_page: value.identity.source.page_url,
            download_url: value.identity.source.download_url,
            executable_path: value.identity.source.executable_path,
            desktop_file: value.identity.source.desktop_file,
            repository: value.identity.source.repository,
        }
    }
}
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PackageScanDto {
    pub packages: Vec<PackageDto>,
    pub warnings: Vec<String>,
}
impl From<PackageScan> for PackageScanDto {
    fn from(value: PackageScan) -> Self {
        Self {
            packages: value.packages.into_iter().map(Into::into).collect(),
            warnings: value.warnings,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct InstallActionDto {
    pub package: PackageDto,
    pub command_preview: String,
    pub requires_privilege: bool,
    pub third_party: bool,
}
impl From<InstallAction> for InstallActionDto {
    fn from(value: InstallAction) -> Self {
        let package = InstalledPackage {
            identity: value.identity,
            version: None,
            explicit: true,
        }
        .into();
        Self {
            package,
            command_preview: std::iter::once(value.program)
                .chain(value.arguments)
                .collect::<Vec<_>>()
                .join(" "),
            requires_privilege: value.requires_privilege,
            third_party: value.third_party,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ChangedPathDto {
    pub code: String,
    pub path: String,
}
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct GitStatusDto {
    pub branch: Option<String>,
    pub origin_url: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: Vec<ChangedPathDto>,
}
impl From<GitStatus> for GitStatusDto {
    fn from(value: GitStatus) -> Self {
        Self {
            branch: value.branch,
            origin_url: value.origin_url,
            upstream: value.upstream,
            ahead: value.ahead,
            behind: value.behind,
            changed: value
                .changed
                .into_iter()
                .map(|v| ChangedPathDto {
                    code: v.code,
                    path: v.path.to_string_lossy().into_owned(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct BackupDto {
    pub id: String,
    pub original_path: String,
    pub existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DiscoveryCandidateDto {
    pub id: String,
    pub application_id: String,
    pub application_name: String,
    pub path: String,
    pub target: String,
    pub role: String,
    pub scope: String,
    pub kind: String,
    pub sensitive: bool,
    pub recommended: bool,
    pub description: String,
    pub managed: bool,
    pub detected_by: Vec<String>,
}
impl DiscoveryCandidateDto {
    pub fn candidate(&self) -> Result<DiscoveryCandidate, ApiError> {
        let kind = match self.kind.as_str() {
            "file" => CandidateKind::File,
            "directory" => CandidateKind::Directory,
            _ => {
                return Err(ApiError {
                    code: "discovery.invalid_kind".into(),
                    message: format!("未知配置类型 {}", self.kind),
                });
            }
        };
        Ok(DiscoveryCandidate {
            id: self.id.clone(),
            application_id: self.application_id.clone(),
            application_name: self.application_name.clone(),
            path: self.path.clone().into(),
            target: self.target.clone(),
            role: self.role.clone(),
            scope: self.scope.clone(),
            kind,
            sensitive: self.sensitive,
            recommended: self.recommended,
            description: self.description.clone(),
            managed: self.managed,
            detected_by: self.detected_by.clone(),
        })
    }
}
impl From<DiscoveryCandidate> for DiscoveryCandidateDto {
    fn from(value: DiscoveryCandidate) -> Self {
        Self {
            id: value.id,
            application_id: value.application_id,
            application_name: value.application_name,
            path: value.path.to_string_lossy().into_owned(),
            target: value.target,
            role: value.role,
            scope: value.scope,
            kind: match value.kind {
                CandidateKind::File => "file",
                CandidateKind::Directory => "directory",
            }
            .into(),
            sensitive: value.sensitive,
            recommended: value.recommended,
            description: value.description,
            managed: value.managed,
            detected_by: value.detected_by,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DiscoveryScanDto {
    pub candidates: Vec<DiscoveryCandidateDto>,
    pub package_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KnowledgeConfigDto {
    pub id: String,
    pub path: String,
    pub role: String,
    pub scope: String,
    pub platforms: Vec<String>,
    pub sensitive: bool,
    pub recommended: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KnowledgeApplicationDto {
    pub id: String,
    pub name: String,
    pub category: String,
    pub packages: Vec<String>,
    pub executables: Vec<String>,
    pub configs: Vec<KnowledgeConfigDto>,
    pub source: String,
}

impl KnowledgeApplicationDto {
    pub fn application(&self) -> ApplicationKnowledge {
        ApplicationKnowledge {
            id: self.id.clone(),
            name: self.name.clone(),
            category: self.category.clone(),
            packages: self.packages.clone(),
            executables: self.executables.clone(),
            configs: self
                .configs
                .iter()
                .map(|config| ConfigKnowledge {
                    id: config.id.clone(),
                    path: config.path.clone(),
                    role: config.role.clone(),
                    scope: config.scope.clone(),
                    platforms: config.platforms.clone(),
                    sensitive: config.sensitive,
                    recommended: config.recommended,
                    description: config.description.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KnowledgeCatalogDto {
    pub applications: Vec<KnowledgeApplicationDto>,
    pub warnings: Vec<String>,
    pub directory: String,
}

impl KnowledgeCatalogDto {
    pub fn from_catalog(value: KnowledgeCatalog, directory: &Path) -> Self {
        Self {
            applications: value
                .applications
                .into_iter()
                .map(|item| KnowledgeApplicationDto {
                    id: item.application.id,
                    name: item.application.name,
                    category: item.application.category,
                    packages: item.application.packages,
                    executables: item.application.executables,
                    configs: item
                        .application
                        .configs
                        .into_iter()
                        .map(|config| KnowledgeConfigDto {
                            id: config.id,
                            path: config.path,
                            role: config.role,
                            scope: config.scope,
                            platforms: config.platforms,
                            sensitive: config.sensitive,
                            recommended: config.recommended,
                            description: config.description,
                        })
                        .collect(),
                    source: match item.source {
                        KnowledgeSource::Builtin => "builtin",
                        KnowledgeSource::User => "user",
                    }
                    .into(),
                })
                .collect(),
            warnings: value.warnings,
            directory: directory.to_string_lossy().into_owned(),
        }
    }
}
impl From<ConfigurationDiscovery> for DiscoveryScanDto {
    fn from(value: ConfigurationDiscovery) -> Self {
        Self {
            candidates: value.candidates.into_iter().map(Into::into).collect(),
            package_count: value.package_count,
            warnings: value.warnings,
        }
    }
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct DiscoveryFailureDto {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct BulkAddDto {
    pub added: Vec<ConfigItemDto>,
    pub skipped: Vec<String>,
    pub failed: Vec<DiscoveryFailureDto>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PreviewFileDto {
    pub path: String,
    pub relative_path: String,
    pub size: usize,
}
impl From<PreviewFile> for PreviewFileDto {
    fn from(value: PreviewFile) -> Self {
        Self {
            path: value.path.to_string_lossy().into_owned(),
            relative_path: value.relative_path.to_string_lossy().into_owned(),
            size: value.size,
        }
    }
}
impl From<BulkAddResult> for BulkAddDto {
    fn from(value: BulkAddResult) -> Self {
        Self {
            added: value.added.into_iter().map(Into::into).collect(),
            skipped: value.skipped,
            failed: value
                .failed
                .into_iter()
                .map(|(path, message)| DiscoveryFailureDto { path, message })
                .collect(),
        }
    }
}
impl From<Backup> for BackupDto {
    fn from(value: Backup) -> Self {
        Self {
            id: value.id,
            original_path: value.original_path.to_string_lossy().into_owned(),
            existed: value.existed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn export_typescript_bindings() {
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/shared/bindings.ts");
        let config = ts_rs::Config::default();
        let text = format!(
            "// Generated from Rust DTOs. Do not edit manually.\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\nexport {}\n",
            AppSnapshotDto::decl(&config),
            RepositoryInspectionDto::decl(&config),
            OperationDto::decl(&config),
            ConfigItemDto::decl(&config),
            TextDocumentDto::decl(&config),
            DiffSessionDto::decl(&config),
            FileRevisionDto::decl(&config),
            PackageDto::decl(&config),
            PackageScanDto::decl(&config),
            InstallActionDto::decl(&config),
            ChangedPathDto::decl(&config),
            GitStatusDto::decl(&config),
            BackupDto::decl(&config),
            DiscoveryCandidateDto::decl(&config),
            DiscoveryScanDto::decl(&config),
            DiscoveryFailureDto::decl(&config),
            BulkAddDto::decl(&config),
            PreviewFileDto::decl(&config),
            KnowledgeConfigDto::decl(&config),
            KnowledgeApplicationDto::decl(&config),
            KnowledgeCatalogDto::decl(&config),
            ToolFactDto::decl(&config),
            MachineFactsDto::decl(&config),
            RestoreStepDto::decl(&config),
            RestoreCountsDto::decl(&config),
            RestorePlanDto::decl(&config),
            RestoreRunItemDto::decl(&config),
            RestoreRunDto::decl(&config),
            MigrationPackageStepDto::decl(&config),
            MigrationPlanDto::decl(&config),
        );
        fs::write(target, text).expect("write TypeScript bindings");
    }
}
