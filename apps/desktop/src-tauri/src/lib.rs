mod commands;
mod dto;

#[cfg(target_os = "linux")]
fn should_disable_dmabuf_renderer(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    gdk_backend: Option<&str>,
    dmabuf_override: Option<&str>,
    compositing_override: Option<&str>,
) -> bool {
    if dmabuf_override.is_some() || compositing_override.is_some() {
        return false;
    }
    if gdk_backend.is_some_and(|backend| backend.eq_ignore_ascii_case("x11")) {
        return false;
    }
    session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || wayland_display.is_some_and(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn configure_linux_webkit_renderer() {
    let read = |name| {
        std::env::var(name)
            .ok()
            .and_then(|value| (!value.is_empty()).then_some(value))
    };
    let session_type = read("XDG_SESSION_TYPE");
    let wayland_display = read("WAYLAND_DISPLAY");
    let gdk_backend = read("GDK_BACKEND");
    let dmabuf_override = read("WEBKIT_DISABLE_DMABUF_RENDERER");
    let compositing_override = read("WEBKIT_DISABLE_COMPOSITING_MODE");
    if should_disable_dmabuf_renderer(
        session_type.as_deref(),
        wayland_display.as_deref(),
        gdk_backend.as_deref(),
        dmabuf_override.as_deref(),
        compositing_override.as_deref(),
    ) {
        // SAFETY: this runs on the main thread before Tauri, WebKitGTK, or any
        // application worker thread is created.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    configure_linux_webkit_renderer();

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
            commands::git_continue_rebase,
            commands::git_abort_rebase,
            commands::git_push,
            commands::git_set_origin,
            commands::git_set_identity,
            commands::backups_list,
            commands::backup_restore,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run EnvWeave desktop application");
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::should_disable_dmabuf_renderer;

    #[test]
    fn disables_dmabuf_for_wayland_sessions() {
        assert!(should_disable_dmabuf_renderer(
            Some("wayland"),
            None,
            None,
            None,
            None
        ));
        assert!(should_disable_dmabuf_renderer(
            None,
            Some("wayland-0"),
            None,
            None,
            None
        ));
    }

    #[test]
    fn preserves_x11_and_explicit_webkit_choices() {
        assert!(!should_disable_dmabuf_renderer(
            Some("x11"),
            None,
            None,
            None,
            None
        ));
        assert!(!should_disable_dmabuf_renderer(
            Some("wayland"),
            Some("wayland-0"),
            Some("x11"),
            None,
            None
        ));
        assert!(!should_disable_dmabuf_renderer(
            Some("wayland"),
            None,
            None,
            Some("0"),
            None
        ));
        assert!(!should_disable_dmabuf_renderer(
            Some("wayland"),
            None,
            None,
            None,
            Some("1")
        ));
    }
}
