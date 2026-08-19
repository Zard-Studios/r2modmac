use super::*;
use tauri::command;

#[command]
pub async fn get_game_path(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<Option<String>, String> {
    let settings = load_settings_impl(&app);
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");
    let cache_key = if let Some(p) = platform {
        format!("{}::{}", game_identifier, p)
    } else {
        game_identifier.clone()
    };

    // Check manual override first
    for key in manual_override_keys(&game_identifier, platform) {
        if let Some(path) = settings.game_paths.get(&key) {
            let path_obj = std::path::Path::new(path);
            if !path_obj.exists() {
                continue;
            }
            if platform.is_some()
                && key == game_identifier
                && !manual_path_matches_platform(path_obj, is_windows_profile)
            {
                log::warn!(
                    "[get_game_path] Ignoring legacy manual path due to platform mismatch: {}",
                    path
                );
                continue;
            }
            log_manual_override_once(&key, path);
            return Ok(Some(path.clone()));
        }
    }

    let steam_paths_to_check = get_steam_roots_for_platform(&app, is_windows_profile);

    if steam_paths_to_check.is_empty() {
        if is_windows_profile {
            // A Windows profile means a compatibility prefix only when the host
            // is not Windows itself; on Windows the prefix wording is nonsense.
            return Err(if cfg!(target_os = "windows") {
                "No Steam path configured. Go to Settings and set your Steam directory (for example C:\\Program Files (x86)\\Steam), or set the game directory manually.".to_string()
            } else {
                "No Windows Steam path configured. Go to Settings and set your Steam directory inside the Wine/CrossOver/Wineskin prefix, or set the game directory manually.".to_string()
            });
        }
        return Err("No Steam installation found (Native macOS or CrossOver)".to_string());
    }

    let normalized_id = normalize_for_matching(&game_identifier);
    log::debug!(
        "[get_game_path] platform={:?} Looking for game: {} (normalized: {})",
        platform,
        game_identifier,
        normalized_id
    );

    // Scan all Steam library folders
    for base_steam in steam_paths_to_check {
        for lib_folder in get_steam_library_folders(&base_steam) {
            let common_path = lib_folder.join("steamapps").join("common");
            if !common_path.exists() {
                continue;
            }

            if let Ok(entries) = fs::read_dir(&common_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    let normalized_folder = normalize_for_matching(&folder_name);

                    // Check for match (exact or high similarity)
                    if normalized_folder == normalized_id
                        || normalized_folder.contains(&normalized_id)
                        || normalized_id.contains(&normalized_folder)
                    {
                        let game_path = entry.path().to_string_lossy().to_string();
                        log::debug!(
                            "[get_game_path] Found match: {} -> {}",
                            folder_name,
                            game_path
                        );
                        if settings.game_paths.get(&cache_key) != Some(&game_path) {
                            let mut updated_settings = settings.clone();
                            updated_settings
                                .game_paths
                                .insert(cache_key.clone(), game_path.clone());
                            let _ = save_settings_impl(&app, &updated_settings);
                        }
                        return Ok(Some(game_path));
                    }
                }
            }
        }
    }

    log::debug!("[get_game_path] No match found for: {}", game_identifier);
    Ok(None)
}

#[command]
pub async fn get_game_source(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<String, String> {
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");

    let Some(game_path_str) = get_game_path(
        app.clone(),
        game_identifier,
        platform.map(|p| p.to_string()),
    )
    .await?
    else {
        return Ok("unknown".to_string());
    };

    Ok(infer_distribution_from_game_path(
        &app,
        std::path::Path::new(&game_path_str),
        is_windows_profile,
    ))
}

/// The library folder itself, which a folder chooser hands back if the user
/// presses Open from inside it. A loader installed there would scatter across
/// every game in the library.
fn is_steam_library_root(path: &std::path::Path) -> bool {
    let is_named_common = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("common"));

    let parent_is_steamapps = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("steamapps"));

    is_named_common && parent_is_steamapps
}

#[command]
pub async fn set_game_path(
    app: AppHandle,
    game_identifier: String,
    path: String,
    platform: Option<String>,
) -> Result<(), String> {
    if is_steam_library_root(std::path::Path::new(&path)) {
        return Err(format!(
            "{} is the Steam library that holds all your games, not a game folder. Pick the game's own folder inside it.",
            path
        ));
    }

    let mut settings = load_settings_impl(&app);
    let key = if let Some(p) = normalized_platform(platform.as_deref()) {
        format!("{}::{}", game_identifier, p)
    } else {
        game_identifier
    };
    settings.game_paths.insert(key, path);
    save_settings_impl(&app, &settings)?;
    Ok(())
}

#[command]
pub async fn open_game_folder(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<(), String> {
    let Some(game_path_str) = get_game_path(app.clone(), game_identifier, platform).await? else {
        return Err("Game directory not found".to_string());
    };

    open::that(std::path::Path::new(&game_path_str))
        .map_err(|e| format!("Failed to open game directory: {}", e))?;
    Ok(())
}

#[command]
pub async fn find_game_executable(game_path: String) -> Result<Option<String>, String> {
    let path = std::path::Path::new(&game_path);

    // On macOS, look for .app bundles first
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".app") {
                return Ok(Some(entry.path().to_string_lossy().to_string()));
            }
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(executable_path) = find_linux_executable_path(path) {
        return Ok(Some(executable_path.to_string_lossy().to_string()));
    }

    // Fallback: look for .exe (for Wine/Proton)
    if let Some(executable_path) = find_pe_game_executable_path(path) {
        return Ok(Some(executable_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

#[cfg(test)]
mod manual_game_path_tests {
    use super::is_steam_library_root;
    use std::path::Path;

    #[test]
    fn the_library_that_holds_every_game_is_not_a_game_folder() {
        for root in [
            "/Volumes/Feduzi/SteamLibrary/steamapps/common",
            "/Users/x/Library/Application Support/Steam/steamapps/common",
            "/Volumes/Feduzi/SteamLibrary/steamapps/Common",
        ] {
            assert!(is_steam_library_root(Path::new(root)), "{root}");
        }
    }

    #[test]
    fn a_game_inside_the_library_is_accepted() {
        for game in [
            "/Volumes/Feduzi/SteamLibrary/steamapps/common/Inscryption",
            "/Users/x/Library/Application Support/Steam/steamapps/common/Muck",
            "/Users/x/Games/common",
        ] {
            assert!(!is_steam_library_root(Path::new(game)), "{game}");
        }
    }
}
