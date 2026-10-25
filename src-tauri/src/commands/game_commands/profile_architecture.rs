//! What the profile's Architecture setting shows, and "Try Apple Silicon again".
//! The decisions live in macos/architecture.rs; these only find the game.

use super::*;
use tauri::command;

fn macos_runtime_for_profile(
    app: &AppHandle,
    game_path: &str,
    profile_id: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let runtime_root = resolve_macos_runtime_root(std::path::Path::new(game_path));
    let tree_root = bepinex_install_root(app, profile_id, &runtime_root)?;
    Ok((runtime_root, tree_root))
}

/// `mode` is the setting as the interface has it right now: profiles are saved
/// a moment after a change, so the file on disk can still hold the old one.
#[command]
pub async fn get_profile_architecture_status(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    mode: Option<String>,
) -> Result<ArchitectureStatus, String> {
    let game_path = get_game_path(app.clone(), game_identifier, Some("mac".to_string()))
        .await?
        .ok_or_else(|| "GAME_PATH_NOT_CONFIGURED".to_string())?;
    let (runtime_root, tree_root) = macos_runtime_for_profile(&app, &game_path, &profile_id)?;
    let facts = gather_facts(
        bundled_apple_silicon_resources(&app).as_deref(),
        &runtime_root,
        &tree_root,
        &sanitize_profile_key(&profile_id),
    );
    let mode = match mode.as_deref() {
        Some(mode) => ArchitectureMode::parse(Some(mode)),
        None => get_profile_architecture(&app, &profile_id),
    };
    Ok(compute_status(mode, &facts))
}

#[command]
pub async fn clear_profile_architecture_pin(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
) -> Result<(), String> {
    let game_path = get_game_path(app.clone(), game_identifier, Some("mac".to_string()))
        .await?
        .ok_or_else(|| "GAME_PATH_NOT_CONFIGURED".to_string())?;
    let (runtime_root, _) = macos_runtime_for_profile(&app, &game_path, &profile_id)?;
    clear_architecture_pin(&runtime_root, &sanitize_profile_key(&profile_id))
}
