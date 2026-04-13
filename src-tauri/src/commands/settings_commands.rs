use crate::models::shared::*;
use tauri::{command, AppHandle};

#[command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(load_settings_impl(&app))
}

#[command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    save_settings_impl(&app, &settings)
}
