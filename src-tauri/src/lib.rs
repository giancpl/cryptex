mod api;

use cryptex_core::project::ProjectService;
use std::{collections::HashMap, sync::Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(ProjectService::default()))
        .manage(api::ProjectWatchers::new(HashMap::new()))
        .invoke_handler(tauri::generate_handler![
            api::health,
            api::open_project,
            api::list_directory,
            api::read_text_file,
            api::write_text_file
        ])
        .run(tauri::generate_context!())
        .expect("failed to run CrypTex");
}
