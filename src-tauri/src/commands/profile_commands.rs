use crate::commands::game_commands::{ensure_macos_steam_launch_options, get_game_path};
use crate::models::shared::{
    get_balatro_mods_dir, is_balatro_game_path, is_balatro_identifier, AppState,
};
use crate::utils::file_ops::*;
use std::fs;
use tauri::{command, AppHandle, Manager, State};

fn is_bepinex_shell_script(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".sh") && lower.contains("bepinex")
}

#[command]
pub fn get_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let profile_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profile_path.exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(profile_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(profiles)
}

#[command]
pub async fn save_profiles(
    app: AppHandle,
    profiles: Vec<serde_json::Value>,
) -> Result<bool, String> {
    let profile_path = app.path().app_data_dir().unwrap().join("profiles.json");

    // Serialize first (fast operation)
    let data = serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?;

    // Write to disk in blocking thread pool to avoid blocking async runtime
    tokio::task::spawn_blocking(move || {
        // Ensure dir exists
        if let Some(parent) = profile_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&profile_path, &data)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())?;

    Ok(true)
}

#[command]
pub async fn delete_profile_folder(
    app: AppHandle,
    profile_id: String,
    game_identifier: Option<String>,
    platform: Option<String>,
) -> Result<bool, String> {
    let profile_dir = app
        .path()
        .app_data_dir()
        .unwrap()
        .join("profiles")
        .join(&profile_id);

    // If game_identifier is provided, clean up ALL BepInEx-related files from the game folder
    if let Some(game_id) = game_identifier {
        let is_mac_profile = platform.as_deref() == Some("mac");
        let is_balatro_game = is_balatro_identifier(&game_id);
        if let Ok(Some(game_path_str)) = get_game_path(app.clone(), game_id, platform.clone()).await
        {
            let game_path = std::path::Path::new(&game_path_str);
            let mut removed_runtime_artifact = false;
            let is_balatro = is_mac_profile && (is_balatro_game || is_balatro_game_path(game_path));

            // Remove BepInEx folder
            let bepinex_path = game_path.join("BepInEx");
            if bepinex_path.exists() {
                eprintln!("[delete_profile] Removing BepInEx folder from game");
                let _ = fs::remove_dir_all(&bepinex_path);
                removed_runtime_artifact = true;
            }

            // Remove winhttp.dll
            let winhttp_path = game_path.join("winhttp.dll");
            if winhttp_path.exists() {
                eprintln!("[delete_profile] Removing winhttp.dll from game");
                let _ = fs::remove_file(&winhttp_path);
                removed_runtime_artifact = true;
            }

            // Remove doorstop_config.ini
            let doorstop_path = game_path.join("doorstop_config.ini");
            if doorstop_path.exists() {
                eprintln!("[delete_profile] Removing doorstop_config.ini from game");
                let _ = fs::remove_file(&doorstop_path);
                removed_runtime_artifact = true;
            }

            if is_mac_profile {
                let _ = ensure_macos_steam_launch_options(&app, game_path, false, true);

                for dir_name in [
                    "doorstop_libs",
                    "doorstop_libs_DISABLED",
                    "plugins",
                    "plugins_DISABLED",
                    "BepInEx_DISABLED",
                ] {
                    let dir_path = game_path.join(dir_name);
                    if dir_path.exists() {
                        eprintln!("[delete_profile] Removing {} from game", dir_name);
                        let _ = fs::remove_dir_all(&dir_path);
                        removed_runtime_artifact = true;
                    }
                }

                for file_name in [
                    "libdoorstop.dylib",
                    "libdoorstop.dylib_DISABLED",
                    "run_bepinex.sh",
                    "run_bepinex.sh_DISABLED",
                    "doorstop_config.ini_DISABLED",
                    ".doorstop_version",
                    ".doorstop_version_DISABLED",
                ] {
                    let file_path = game_path.join(file_name);
                    if file_path.exists() {
                        eprintln!("[delete_profile] Removing {} from game", file_name);
                        let _ = fs::remove_file(&file_path);
                        removed_runtime_artifact = true;
                    }
                }

                if let Ok(entries) = fs::read_dir(game_path) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();
                        let name = entry.file_name().to_string_lossy().to_string();
                        if path.is_file()
                            && (is_bepinex_shell_script(&name) || name.ends_with("_DISABLED"))
                        {
                            eprintln!("[delete_profile] Removing {} from game", name);
                            let _ = fs::remove_file(&path);
                            removed_runtime_artifact = true;
                        }
                    }
                }

                if removed_runtime_artifact {
                    for file_name in ["manifest.json", "README.md", "readme.md", "icon.png"] {
                        let file_path = game_path.join(file_name);
                        if file_path.exists() {
                            eprintln!("[delete_profile] Removing {} from game", file_name);
                            let _ = fs::remove_file(&file_path);
                        }
                    }
                }

                if is_balatro {
                    for file_name in ["run_lovely_macos.sh", "liblovely.dylib"] {
                        let file_path = game_path.join(file_name);
                        if file_path.exists() {
                            eprintln!("[delete_profile] Removing {} from Balatro root", file_name);
                            let _ = fs::remove_file(&file_path);
                        }
                    }

                    if let Some(mods_dir) = get_balatro_mods_dir() {
                        let disabled_dir = mods_dir
                            .parent()
                            .map(|p| p.join("Mods_DISABLED"))
                            .unwrap_or_else(|| mods_dir.join("Mods_DISABLED"));
                        for dir_path in [mods_dir.clone(), disabled_dir] {
                            if dir_path.exists() {
                                eprintln!(
                                    "[delete_profile] Removing Balatro mods dir {:?}",
                                    dir_path
                                );
                                let _ = fs::remove_dir_all(&dir_path);
                            }
                        }
                    }
                }
            }

            eprintln!(
                "[delete_profile] Cleaned up game folder: {}",
                game_path.display()
            );
        }
    }

    // Delete the profile folder
    if profile_dir.exists() {
        fs::remove_dir_all(profile_dir).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[command]
pub async fn open_profile_folder(app: AppHandle, profile_id: String) -> Result<(), String> {
    let profile_dir = app
        .path()
        .app_data_dir()
        .unwrap()
        .join("profiles")
        .join(&profile_id);
    eprintln!(
        "[open_profile_folder] Attempting to open: {:?}",
        profile_dir
    );

    // Create the folder if it doesn't exist
    if !profile_dir.exists() {
        eprintln!("[open_profile_folder] Folder doesn't exist, creating it...");
        fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
    }

    eprintln!("[open_profile_folder] Opening folder in Finder...");
    open::that(&profile_dir).map_err(|e| {
        eprintln!("[open_profile_folder] Failed to open: {}", e);
        e.to_string()
    })?;
    eprintln!("[open_profile_folder] Success!");
    Ok(())
}

#[command]
pub async fn clear_profile_cache(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let profiles_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");

    let mut cleared = 0;
    let mut size_freed: u64 = 0;
    let mut chunk_files_removed = 0;

    if profiles_dir.exists() {
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let bepinex_dir = entry.path().join("BepInEx");
                    if bepinex_dir.exists() {
                        // Calculate size before deleting
                        if let Ok(size) = calculate_dir_size(&bepinex_dir) {
                            size_freed += size;
                        }
                        eprintln!("[clear_profile_cache] Removing: {:?}", bepinex_dir);
                        let _ = fs::remove_dir_all(&bepinex_dir);
                        cleared += 1;
                    }
                }
            }
        }
    }

    if let Ok(cache_dir) = app.path().app_cache_dir() {
        let chunks_dir = cache_dir.join("chunks");
        if chunks_dir.exists() {
            if let Ok(size) = calculate_dir_size(&chunks_dir) {
                size_freed += size;
            }
            if let Ok(entries) = fs::read_dir(&chunks_dir) {
                chunk_files_removed = entries.filter_map(|e| e.ok()).count() as i32;
            }
            let _ = fs::remove_dir_all(&chunks_dir);
            let _ = fs::create_dir_all(&chunks_dir);
        }
    }

    {
        let mut platform_cache = state.platform_cache.write().await;
        platform_cache.clear();
    }

    eprintln!(
        "[clear_profile_cache] Cleared {} profile caches, removed {} chunk files, freed {} bytes",
        cleared, chunk_files_removed, size_freed
    );

    Ok(serde_json::json!({
        "cleared": cleared,
        "chunks_cleared": chunk_files_removed,
        "bytes_freed": size_freed
    }))
}

#[command]
pub async fn toggle_profile_vanilla_mode(
    app: AppHandle,
    profile_id: String,
) -> Result<bool, String> {
    let profile_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profile_path.exists() {
        return Err("No profiles found".to_string());
    }

    let data = fs::read_to_string(&profile_path).map_err(|e| e.to_string())?;
    let mut profiles: Vec<serde_json::Value> =
        serde_json::from_str(&data).map_err(|e| e.to_string())?;

    let mut new_state = false;
    let mut found = false;

    for p in &mut profiles {
        if p["id"].as_str() == Some(&profile_id) {
            let current = p["is_vanilla"].as_bool().unwrap_or(false);
            new_state = !current;
            p["is_vanilla"] = serde_json::Value::Bool(new_state);
            found = true;
            break;
        }
    }

    if found {
        save_profiles(app, profiles).await?;
        Ok(new_state)
    } else {
        Err("Profile not found".to_string())
    }
}
