mod api;

use cryptex_core::{project::ProjectService, settings::RootPreferences};
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::health,
            api::open_project,
            api::list_directory,
            api::read_text_file,
            api::write_text_file,
            api::detect_root_documents,
            api::set_root_document
        ])
        .run(tauri::generate_context!())
        .expect("failed to run CrypTex");
}
