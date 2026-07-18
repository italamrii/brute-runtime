mod commands;
mod paths;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
