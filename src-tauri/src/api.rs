use cryptex_core::api::{API_VERSION, HealthResponse};

#[tauri::command]
pub fn health() -> HealthResponse {
    HealthResponse {
        api_version: API_VERSION,
        application: "CrypTex".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}
