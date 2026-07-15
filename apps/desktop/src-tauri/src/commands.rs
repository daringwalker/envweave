use crate::dto::{
    ApiError, AppSnapshotDto, BackupDto, BulkAddDto, ConfigItemDto, DiffSessionDto,
    DiscoveryScanDto, FileRevisionDto, GitStatusDto, InstallActionDto, KnowledgeApplicationDto,
    KnowledgeCatalogDto, MigrationPlanDto, OperationDto, PackageDto, PackageScanDto,
    PreviewFileDto, RepositoryInspectionDto, RestorePlanDto, RestoreRunDto, TextDocumentDto,
};
use envweave_application::{AppService, ApplicationError};
use std::path::{Path, PathBuf};
use tauri::Manager;

fn knowledge_directory(app: &tauri::AppHandle) -> Result<PathBuf, ApiError> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("knowledge.d"))
        .map_err(|error| ApiError {
            code: "knowledge.config_path_failed".into(),
            message: format!("无法确定用户知识库目录：{error}"),
        })
}

async fn background<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, ApplicationError> + Send + 'static,
) -> Result<T, ApiError> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| ApiError {
            code: "task.join_failed".into(),
            message: error.to_string(),
        })?
        .map_err(ApiError::from)
}

#[tauri::command]
pub fn app_snapshot() -> AppSnapshotDto {
    AppService.system_snapshot().into()
}

#[tauri::command]
pub fn repository_inspect(path: String) -> Result<RepositoryInspectionDto, ApiError> {
    AppService
        .inspect_repository(PathBuf::from(path))
        .map(Into::into)
        .map_err(|error| ApiError {
            code: "repository.invalid_path".to_owned(),
            message: error.to_string(),
        })
}

#[tauri::command]
pub async fn repository_create(
    path: String,
    initialize_git: bool,
) -> Result<RepositoryInspectionDto, ApiError> {
    background(move || AppService.create_repository(Path::new(&path), initialize_git))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn repository_clone(
    remote: String,
    destination: String,
) -> Result<RepositoryInspectionDto, ApiError> {
    background(move || AppService.clone_repository(&remote, Path::new(&destination)))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn config_list(repository: String) -> Result<Vec<ConfigItemDto>, ApiError> {
    background(move || AppService.list_config_items(Path::new(&repository)))
        .await
        .map(|items| items.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn config_add(
    repository: String,
    target: String,
    allow_sensitive: bool,
) -> Result<ConfigItemDto, ApiError> {
    background(move || {
        AppService.add_config_item(Path::new(&repository), Path::new(&target), allow_sensitive)
    })
    .await
    .map(Into::into)
}

#[tauri::command]
pub async fn discovery_scan(
    app: tauri::AppHandle,
    repository: String,
) -> Result<DiscoveryScanDto, ApiError> {
    let knowledge = knowledge_directory(&app)?;
    background(move || AppService.discover_configurations(Path::new(&repository), &knowledge))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn discovery_add(
    repository: String,
    candidates: Vec<crate::dto::DiscoveryCandidateDto>,
) -> Result<BulkAddDto, ApiError> {
    let candidates = candidates
        .iter()
        .map(crate::dto::DiscoveryCandidateDto::candidate)
        .collect::<Result<Vec<_>, _>>()?;
    background(move || {
        AppService.add_discovered_configurations(Path::new(&repository), &candidates)
    })
    .await
    .map(Into::into)
}

#[tauri::command]
pub async fn discovery_preview_index(
    app: tauri::AppHandle,
    root: String,
) -> Result<Vec<PreviewFileDto>, ApiError> {
    let knowledge = knowledge_directory(&app)?;
    background(move || AppService.configuration_preview_index(Path::new(&root), &knowledge))
        .await
        .map(|files| files.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn discovery_preview_read(
    app: tauri::AppHandle,
    root: String,
    file: String,
) -> Result<TextDocumentDto, ApiError> {
    let knowledge = knowledge_directory(&app)?;
    background(move || {
        AppService.read_configuration_preview(Path::new(&root), Path::new(&file), &knowledge)
    })
    .await
    .map(Into::into)
}

#[tauri::command]
pub fn knowledge_list(app: tauri::AppHandle) -> Result<KnowledgeCatalogDto, ApiError> {
    let directory = knowledge_directory(&app)?;
    AppService
        .knowledge_catalog(&directory)
        .map(|catalog| KnowledgeCatalogDto::from_catalog(catalog, &directory))
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn knowledge_save(
    app: tauri::AppHandle,
    application: KnowledgeApplicationDto,
) -> Result<KnowledgeCatalogDto, ApiError> {
    let directory = knowledge_directory(&app)?;
    let knowledge = application.application();
    let result_directory = directory.clone();
    background(move || AppService.save_user_knowledge(&directory, &knowledge))
        .await
        .map(|catalog| KnowledgeCatalogDto::from_catalog(catalog, &result_directory))
}

#[tauri::command]
pub async fn knowledge_delete(
    app: tauri::AppHandle,
    id: String,
) -> Result<KnowledgeCatalogDto, ApiError> {
    let directory = knowledge_directory(&app)?;
    let result_directory = directory.clone();
    background(move || AppService.delete_user_knowledge(&directory, &id))
        .await
        .map(|catalog| KnowledgeCatalogDto::from_catalog(catalog, &result_directory))
}

#[tauri::command]
pub fn config_remove(repository: String, item_id: String) -> Result<OperationDto, ApiError> {
    AppService
        .remove_config_item(Path::new(&repository), &item_id)
        .map(|_| OperationDto {
            message: "配置项已从仓库删除，本机文件未受影响".into(),
        })
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn config_capture(repository: String, item_id: String) -> Result<OperationDto, ApiError> {
    background(move || AppService.capture_item(Path::new(&repository), &item_id))
        .await
        .map(|_| OperationDto {
            message: "已将本机配置收集到仓库".into(),
        })
}

#[tauri::command]
pub async fn config_apply(repository: String, item_id: String) -> Result<OperationDto, ApiError> {
    background(move || AppService.apply_item(Path::new(&repository), &item_id))
        .await
        .map(|backup| OperationDto {
            message: format!("配置已应用，备份 {} 已创建", backup.id),
        })
}

#[tauri::command]
pub fn diff_open(repository: String, item_id: String) -> Result<DiffSessionDto, ApiError> {
    AppService
        .open_diff(Path::new(&repository), &item_id)
        .map(Into::into)
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn diff_history(
    repository: String,
    item_id: String,
) -> Result<Vec<FileRevisionDto>, ApiError> {
    background(move || AppService.diff_history(Path::new(&repository), &item_id))
        .await
        .map(|items| items.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn diff_open_revision(
    repository: String,
    item_id: String,
    revision: String,
) -> Result<TextDocumentDto, ApiError> {
    background(move || {
        AppService.open_repository_revision(Path::new(&repository), &item_id, &revision)
    })
    .await
    .map(Into::into)
}

#[tauri::command]
pub fn diff_save_repository(
    repository: String,
    item_id: String,
    expected_revision: String,
    content: String,
) -> Result<TextDocumentDto, ApiError> {
    AppService
        .save_repository_text(
            Path::new(&repository),
            &item_id,
            &expected_revision,
            &content,
        )
        .map(Into::into)
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn packages_scan(repository: String) -> Result<PackageScanDto, ApiError> {
    tauri::async_runtime::spawn_blocking(move || {
        AppService.scan_repository_packages(Path::new(&repository))
    })
    .await
    .map(Into::into)
    .map_err(|error| ApiError {
        code: "task.join_failed".into(),
        message: error.to_string(),
    })
}

#[tauri::command]
pub fn packages_save(
    repository: String,
    packages: Vec<PackageDto>,
) -> Result<OperationDto, ApiError> {
    let identities = packages
        .iter()
        .map(PackageDto::identity)
        .collect::<Result<Vec<_>, _>>()?;
    AppService
        .save_package_inventory(Path::new(&repository), &identities)
        .map(|_| OperationDto {
            message: format!("已保存 {} 个软件包", identities.len()),
        })
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn packages_plan(
    repository: String,
    aur_helper: Option<String>,
) -> Result<Vec<InstallActionDto>, ApiError> {
    background(move || {
        AppService.package_install_plan(Path::new(&repository), aur_helper.as_deref())
    })
    .await
    .map(|actions| actions.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn package_install(
    package: PackageDto,
    aur_helper: Option<String>,
) -> Result<OperationDto, ApiError> {
    let identity = package.identity()?;
    let name = identity.name.clone();
    background(move || AppService.install_package(&identity, aur_helper.as_deref()))
        .await
        .map(|_| OperationDto {
            message: format!("{} 安装完成", name),
        })
}

#[tauri::command]
pub async fn restore_preflight(repository: String) -> Result<RestorePlanDto, ApiError> {
    background(move || AppService.restore_plan(Path::new(&repository)))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn migration_preflight(
    repository: String,
    aur_helper: Option<String>,
) -> Result<MigrationPlanDto, ApiError> {
    background(move || AppService.migration_plan(Path::new(&repository), aur_helper.as_deref()))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn restore_execute(
    repository: String,
    plan_id: String,
    selected_ids: Vec<String>,
) -> Result<RestoreRunDto, ApiError> {
    background(move || AppService.execute_restore(Path::new(&repository), &plan_id, &selected_ids))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn restore_incomplete(repository: String) -> Result<Vec<RestoreRunDto>, ApiError> {
    background(move || AppService.incomplete_restore_runs(Path::new(&repository)))
        .await
        .map(|runs| runs.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn restore_recover(
    repository: String,
    run_id: String,
) -> Result<RestoreRunDto, ApiError> {
    background(move || AppService.recover_restore_run(Path::new(&repository), &run_id))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn restore_keep_current(
    repository: String,
    run_id: String,
) -> Result<RestoreRunDto, ApiError> {
    background(move || AppService.keep_restore_run(Path::new(&repository), &run_id))
        .await
        .map(Into::into)
}

#[tauri::command]
pub fn git_status(repository: String) -> Result<GitStatusDto, ApiError> {
    AppService
        .git_status(Path::new(&repository))
        .map(Into::into)
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn git_commit(repository: String, message: String) -> Result<GitStatusDto, ApiError> {
    background(move || AppService.git_commit(Path::new(&repository), &message))
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn git_fetch(repository: String) -> Result<GitStatusDto, ApiError> {
    background(move || AppService.git_fetch(Path::new(&repository)))
        .await
        .map(Into::into)
}
#[tauri::command]
pub async fn git_pull(repository: String) -> Result<GitStatusDto, ApiError> {
    background(move || AppService.git_pull(Path::new(&repository)))
        .await
        .map(Into::into)
}
#[tauri::command]
pub async fn git_continue_rebase(repository: String) -> Result<GitStatusDto, ApiError> {
    background(move || AppService.git_continue_rebase(Path::new(&repository)))
        .await
        .map(Into::into)
}
#[tauri::command]
pub async fn git_abort_rebase(repository: String) -> Result<GitStatusDto, ApiError> {
    background(move || AppService.git_abort_rebase(Path::new(&repository)))
        .await
        .map(Into::into)
}
#[tauri::command]
pub async fn git_push(repository: String) -> Result<GitStatusDto, ApiError> {
    background(move || AppService.git_push(Path::new(&repository)))
        .await
        .map(Into::into)
}

#[tauri::command]
pub fn git_set_origin(repository: String, remote: String) -> Result<GitStatusDto, ApiError> {
    AppService
        .git_set_origin(Path::new(&repository), &remote)
        .map(Into::into)
        .map_err(ApiError::from)
}

#[tauri::command]
pub fn git_set_identity(
    repository: String,
    name: String,
    email: String,
) -> Result<OperationDto, ApiError> {
    AppService
        .git_set_identity(Path::new(&repository), &name, &email)
        .map(|_| OperationDto {
            message: "Git 提交身份已保存到当前仓库".into(),
        })
        .map_err(ApiError::from)
}

#[tauri::command]
pub fn backups_list(repository: String) -> Result<Vec<BackupDto>, ApiError> {
    AppService
        .list_backups(Path::new(&repository))
        .map(|items| items.into_iter().map(Into::into).collect())
        .map_err(ApiError::from)
}

#[tauri::command]
pub fn backup_restore(repository: String, id: String) -> Result<OperationDto, ApiError> {
    AppService
        .restore_backup(Path::new(&repository), &id)
        .map(|backup| OperationDto {
            message: format!("已恢复 {}", backup.original_path.display()),
        })
        .map_err(ApiError::from)
}
