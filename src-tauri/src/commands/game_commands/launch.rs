use super::owml_patcher;
use super::*;
use tauri::command;

#[command]
pub async fn launch_game_with_mods(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    platform: Option<String>,
) -> Result<(), String> {
    // Clears any cancellation left by a previous attempt, so pressing Play
    // after stopping one launch does not abort the next one instantly.
    launch_cancel::begin_launch();
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);

    let is_outerwilds =
        is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(&game_path);
    if is_outerwilds {
        // 1. Restore OWML folder if it was disabled
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if owml_disabled.exists() && !owml_folder.exists() {
            let _ = std::fs::rename(&owml_disabled, &owml_folder);
            log::info!("[launch_game_with_mods] Restored OWML_DISABLED -> OWML");
        }

        if !owml_folder.exists() {
            return Err("OWML folder not found. Please install OWML first.".to_string());
        }

        // 2. Save a one-time vanilla backup so we always have a clean base to patch from.
        if let Err(e) = backup_outerwilds_vanilla_dll(&game_path) {
            log::warn!(
                "[launch_game_with_mods] Could not create vanilla backup (non-fatal): {}",
                e
            );
        }

        // 3. Restore vanilla DLL so the patcher always starts from a clean, unpatched state.
        //    This prevents double-patching corruption if the DLL is already patched from a
        //    previous modded launch.
        if let Err(e) = restore_outerwilds_vanilla(&game_path) {
            log::warn!(
                "[launch_game_with_mods] Could not restore vanilla DLL (non-fatal): {}",
                e
            );
        }

        // 4. Patch Assembly-CSharp.dll via our socket-free Wine patcher.
        log::info!("[launch_game_with_mods] Running OWMLPatcher.exe via Wine");
        owml_patcher::run_owml_patcher(&game_path)
            .map_err(|e| format!("Failed to patch the game for mods. Make sure OWML is installed and the game path is correct.\nPatcher error: {}", e))?;

        // 5. Launch OuterWilds.exe — mods are injected via the patched Assembly-CSharp.dll.
        log::info!("[launch_game_with_mods] Launching OuterWilds.exe directly");
        return launch_windows_game(&app, &game_path);
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
    // Clears any cancellation left by a previous attempt, so pressing Play
    // after stopping one launch does not abort the next one instantly.
    launch_cancel::begin_launch();
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);

    let is_outerwilds =
        is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(&game_path);
    if is_outerwilds {
        // Disable OWML by renaming the folder
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if owml_folder.exists() {
            if owml_disabled.exists() {
                let _ = std::fs::remove_dir_all(&owml_disabled);
            }
            let _ = std::fs::rename(&owml_folder, &owml_disabled);
            log::info!("[launch_game_vanilla] Renamed OWML -> OWML_DISABLED");
        }

        // Restore vanilla Assembly-CSharp.dll from .vanilla or .bak backup
        let _ = restore_outerwilds_vanilla(&game_path);
        let _ = restore_mscorlib_vanilla(&game_path, false);

        // Launch OuterWilds.exe directly — vanilla, no mods
        log::info!("[launch_game_vanilla] Launching OuterWilds.exe directly (vanilla)");
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

    if is_windows_profile
        || is_outerwilds_identifier(&game_identifier)
        || is_outerwilds_game_path(&game_path)
    {
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

    if is_windows_profile
        || is_outerwilds_identifier(&game_identifier)
        || is_outerwilds_game_path(&game_path)
    {
        return stop_game_for_windows(&game_path);
    }

    #[cfg(target_os = "macos")]
    return stop_game_for_macos(&game_path);

    #[cfg(not(target_os = "macos"))]
    Err("stop_game is not supported on this platform".to_string())
}
