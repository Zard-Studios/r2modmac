use super::*;
use tauri::command;

#[command]
pub async fn launch_game_with_mods(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    platform: Option<String>,
) -> Result<(), String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);

    if is_windows_profile {
        return launch_windows_game(&app, &game_path);
    }

    launch_game_with_mods_for_macos(&app, &game_identifier, &profile_id, &game_path).await
}

#[command]
pub async fn launch_game_vanilla(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    platform: Option<String>,
) -> Result<(), String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;

    if is_windows_profile {
        return launch_windows_game(&app, std::path::Path::new(&game_path_str));
    }

    launch_game_vanilla_for_macos(&app, &game_identifier, &profile_id, &game_path_str).await
}

#[command]
pub async fn is_game_running(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<bool, String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = match get_game_path(app.clone(), game_identifier.clone(), platform).await? {
        Some(path) => path,
        None => return Ok(false),
    };
    let game_path = std::path::PathBuf::from(&game_path_str);

    if is_windows_profile || is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(&game_path) {
        return is_game_running_for_windows(&game_path);
    }

    is_game_running_for_macos(&game_path)
}

#[command]
pub async fn stop_game(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<(), String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);

    if is_windows_profile || is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(&game_path) {
        return stop_game_for_windows(&game_path);
    }

    #[cfg(target_os = "macos")]
    return stop_game_for_macos(&game_path);

    #[cfg(not(target_os = "macos"))]
    Err("stop_game is not supported on this platform".to_string())
}
