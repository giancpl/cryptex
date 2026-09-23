mod api;
mod builds;

use cryptex_core::{
    catalog::CommandCatalog,
    notation::NotationService,
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
        .manage(api::ProjectIndexes::default())
        .manage(CommandCatalog::bundled().expect("bundled command catalog must be valid"))
        .setup(|app| {
            let settings = app.path().app_config_dir()?.join("root-documents.json");
            app.manage(Mutex::new(RootPreferences::load(settings)?));
            app.manage(Mutex::new(EnginePreferences::load(
                app.path().app_config_dir()?.join("latex-engines.json"),
            )?));
            app.manage(Mutex::new(TrustService::load(
                app.path().app_config_dir()?.join("project-trust.json"),
            )?));
            app.manage(Mutex::new(NotationService::load(
                app.path().app_config_dir()?.join("notation-profiles.json"),
            )?));
            app.manage(RecoveryService::new(
                app.path().app_data_dir()?.join("recovery"),
            ));
            app.manage(ToolchainService::new(
                app.path().app_data_dir()?.join("toolchains"),
            ));
            app.manage(builds::BuildRuntime::new(
                app.path().app_cache_dir()?.join("builds"),
            )?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::health,
            api::toolchain_readiness,
            api::open_project,
            api::project_index,
            api::search_command_catalog,
            api::notation_profile,
            api::notation_usage,
            api::notation_diagnostics,
            api::preview_notation_rename_command,
            api::apply_notation_rename,
            api::notation_suppressions,
            api::set_notation_suppressions,
            api::set_global_notation_profile,
            api::reset_global_notation_profile,
            api::set_project_notation_overrides,
            api::import_notation_profile,
            api::export_notation_profile,
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
            api::delete_recovery_snapshot,
            builds::request_build,
            builds::cancel_build,
            builds::clean_build_artifacts,
            builds::read_build_log,
            builds::read_build_pdf,
            builds::forward_synctex,
            builds::inverse_synctex
        ])
        .run(tauri::generate_context!())
        .expect("failed to run CrypTex");
}
