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

    let is_outerwilds = is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(&game_path);
    if is_outerwilds {
        let owml_launcher = game_path.join("OWML").join("OWML.Launcher.exe");
        if !owml_launcher.exists() {
            return Err("OWML.Launcher.exe not found. Please install OWML first.".to_string());
        }
        eprintln!("[launch_game_with_mods] Launching OWML for Outer Wilds: {:?}", owml_launcher);
        return launch_windows_direct_game(&owml_launcher.parent().unwrap_or(&game_path));
    }

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
    let game_path = std::path::PathBuf::from(&game_path_str);

    let is_outerwilds = is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(&game_path);
    if is_outerwilds {
        // Unpatch the game before launching vanilla
        let managed_dir = game_path.join("OuterWilds_Data").join("Managed");
        if managed_dir.exists() {
            let dll_path = managed_dir.join("Assembly-CSharp.dll");
            let bak_path = managed_dir.join("Assembly-CSharp.dll.bak");
            if bak_path.exists() {
                let _ = std::fs::copy(&bak_path, &dll_path);
                let _ = std::fs::remove_file(&bak_path);
                eprintln!("[launch_game_vanilla] Restored vanilla Assembly-CSharp.dll for Outer Wilds");
            }
        }
        return launch_windows_game(&app, &game_path);
    }

    if is_windows_profile {
        return launch_windows_game(&app, &game_path);
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
