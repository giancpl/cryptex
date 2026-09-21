mod api;

use cryptex_core::{
    project::ProjectService,
    recovery::RecoveryService,
    settings::{EnginePreferences, RootPreferences},
    toolchain::ToolchainService,
    trust::TrustService,
};
use std::{collections::HashMap, sync::Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(ProjectService::default()))
        .manage(api::ProjectWatchers::new(HashMap::new()))
        .setup(|app| {
            let settings = app.path().app_config_dir()?.join("root-documents.json");
            app.manage(Mutex::new(RootPreferences::load(settings)?));
            app.manage(Mutex::new(EnginePreferences::load(
                app.path().app_config_dir()?.join("latex-engines.json"),
            )?));
            app.manage(Mutex::new(TrustService::load(
                app.path().app_config_dir()?.join("project-trust.json"),
            )?));
            app.manage(RecoveryService::new(
                app.path().app_data_dir()?.join("recovery"),
            ));
            app.manage(ToolchainService::new(
                app.path().app_data_dir()?.join("toolchains"),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::health,
            api::toolchain_readiness,
            api::open_project,
            api::list_directory,
            api::read_text_file,
            api::write_text_file,
            api::detect_root_documents,
            api::set_root_document,
            api::resolve_build_configuration,
            api::set_project_engine,
            api::project_trust,
            api::set_project_permission,
            api::revoke_project_trust,
            api::store_recovery_snapshot,
            api::list_recovery_snapshots,
            api::delete_recovery_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("failed to run CrypTex");
}
