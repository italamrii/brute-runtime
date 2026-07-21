mod commands;
#[cfg(test)]
mod no_window_flash_guard_test;
mod paths;
mod state;

/// The main BRUTE window must never navigate away from its own app
/// pages - not to `localhost` (a stray dev-server reference), not to a
/// real external site, not to `file://`/`javascript:`. This is a
/// cannot-be-bypassed guard at the WebView level: it rejects every
/// navigation attempt regardless of what triggered it (a frontend bug,
/// a future regression, a malformed link) - opening an external URL is
/// only ever done via `tauri-plugin-opener`'s `open_url`, which launches
/// the user's real default browser as a separate process and never
/// navigates this window at all. See docs/tauri-security-boundary.md.
fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("brute-navigation-guard")
        .on_navigation(|_webview, url| {
            let scheme = url.scheme();
            let host = url.host_str().unwrap_or("");
            if cfg!(dev) {
                // Dev mode: the app's own pages are served by the Vite
                // dev server on the configured devUrl - nothing else.
                return scheme == "http" && host == "localhost";
            }
            // Release: the app's own pages are served via the `tauri://`
            // custom protocol (macOS/Linux) or the `tauri.localhost`
            // virtual host WebView2 uses on Windows - never a real
            // network origin. Anything else is rejected before it loads.
            scheme == "tauri" || host == "tauri.localhost"
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(navigation_guard())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::hardware::hardware_profile,
            commands::catalog::catalog_list,
            commands::catalog::catalog_show,
            commands::catalog::calibrations_list,
            commands::catalog::fit_evaluate,
            commands::catalog::recommend_model,
            commands::catalog::explain_fit,
            commands::backends::backends_verify,
            commands::library::library_list,
            commands::library::library_show,
            commands::library::library_associations,
            commands::library::library_import,
            commands::library::library_scan,
            commands::library::library_import_directory,
            commands::library::library_verify,
            commands::library::library_refresh,
            commands::library::library_audit,
            commands::library::library_duplicates,
            commands::library::library_storage,
            commands::library::library_locate,
            commands::library::library_alias,
            commands::library::library_note,
            commands::library::library_forget,
            commands::library::library_quarantine,
            commands::library::library_unquarantine,
            commands::library::library_quarantined,
            commands::library::library_export,
            commands::tuning::tune_dry_run,
            commands::tuning::tune_run,
            commands::tuning::tune_cancel,
            commands::profiles::profiles_list,
            commands::profiles::profiles_show,
            commands::profiles::profiles_verify,
            commands::profiles::profiles_export,
            commands::profiles::profiles_delete,
            commands::run::local_run_generate,
            commands::run::local_run_cancel,
            commands::runtime::resolve_runtime,
            commands::discovery::list_common_model_locations,
            commands::discovery::scan_common_model_locations,
            commands::download::download_model,
            commands::download::cancel_download,
            commands::conversations::conversations_list,
            commands::conversations::conversations_show,
            commands::conversations::conversations_create,
            commands::conversations::conversations_save,
            commands::conversations::conversations_delete,
            commands::conversations::conversations_clear_all,
            commands::preferences::preferences_get,
            commands::preferences::preferences_save,
            commands::preferences::preferences_reset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
