mod commands;
mod dto;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_snapshot,
            commands::repository_inspect,
            commands::repository_create,
            commands::repository_clone,
            commands::config_list,
            commands::config_add,
            commands::discovery_scan,
            commands::discovery_add,
            commands::discovery_preview_index,
            commands::discovery_preview_read,
            commands::knowledge_list,
            commands::knowledge_save,
            commands::knowledge_delete,
            commands::config_remove,
            commands::config_capture,
            commands::config_apply,
            commands::diff_open,
            commands::diff_history,
            commands::diff_open_revision,
            commands::diff_save_repository,
            commands::packages_scan,
            commands::packages_save,
            commands::packages_plan,
            commands::package_install,
            commands::restore_preflight,
            commands::migration_preflight,
            commands::restore_execute,
            commands::restore_incomplete,
            commands::restore_recover,
            commands::restore_keep_current,
            commands::git_status,
            commands::git_commit,
            commands::git_fetch,
            commands::git_pull,
            commands::git_push,
            commands::git_set_origin,
            commands::git_set_identity,
            commands::backups_list,
            commands::backup_restore,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run EnvWeave desktop application");
}
