use std::fs;
use tauri::{command, AppHandle, Manager};
use crate::models::shared::*;
use crate::utils::file_ops::*;

fn normalized_platform(platform: Option<&str>) -> Option<&'static str> {
    match platform {
        Some("windows") => Some("windows"),
        Some("mac") => Some("mac"),
        _ => None,
    }
}

fn manual_override_keys(game_identifier: &str, platform: Option<&str>) -> Vec<String> {
    if let Some(p) = normalized_platform(platform) {
        vec![format!("{}::{}", game_identifier, p), game_identifier.to_string()]
    } else {
        vec![game_identifier.to_string()]
    }
}

fn manual_path_matches_platform(path: &std::path::Path, is_windows_profile: bool) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    let mut has_app_bundle = false;
    let mut has_windows_exe = false;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".app") {
                has_app_bundle = true;
            }
            if name.ends_with(".exe") && !name.contains("UnityCrashHandler") {
                has_windows_exe = true;
            }
        }
    }

    if is_windows_profile {
        has_windows_exe || !has_app_bundle
    } else {
        has_app_bundle || !has_windows_exe
    }
}

fn get_profile_platform(app: &AppHandle, profile_id: &str) -> String {
    let profiles_path = app.path().app_data_dir().unwrap_or_default().join("profiles.json");
    if profiles_path.exists() {
        if let Ok(data) = fs::read_to_string(&profiles_path) {
            if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                if let Some(profile) = profiles.iter().find(|p| p["id"].as_str() == Some(profile_id)) {
                    if let Some(platform) = profile["platform"].as_str() {
                        if platform == "mac" || platform == "windows" {
                            return platform.to_string();
                        }
                    }
                }
            }
        }
    }
    "windows".to_string()
}

#[command]
pub async fn get_game_path(app: AppHandle, game_identifier: String, platform: Option<String>) -> Result<Option<String>, String> {
    let settings = load_settings_impl(&app);
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");

    // Check manual override first
    for key in manual_override_keys(&game_identifier, platform) {
        if let Some(path) = settings.game_paths.get(&key) {
            let path_obj = std::path::Path::new(path);
            if !path_obj.exists() {
                continue;
            }
            if platform.is_some() && key == game_identifier && !manual_path_matches_platform(path_obj, is_windows_profile) {
                eprintln!("[get_game_path] Ignoring legacy manual path due to platform mismatch: {}", path);
                continue;
            }
            eprintln!("[get_game_path] Found manual override (key={}): {}", key, path);
            return Ok(Some(path.clone()));
        }
    }

    let mut steam_paths_to_check = Vec::new();
    
    // Only check native macOS Steam for mac profiles.
    // Windows (CrossOver/Wine) profiles must NEVER use the macOS native Steam path.
    if !is_windows_profile {
        if let Some(home) = dirs::home_dir() {
            let mac_steam = home.join("Library/Application Support/Steam");
            if mac_steam.exists() {
                steam_paths_to_check.push(mac_steam);
            }
        }
    }

    if let Some(steam_path_str) = &settings.steam_path {
        let crossover_steam = std::path::PathBuf::from(steam_path_str);
        if crossover_steam.exists() {
            steam_paths_to_check.push(crossover_steam);
        }
    }

    if steam_paths_to_check.is_empty() {
        if is_windows_profile {
            return Err("No CrossOver/Wine Steam path configured. Go to Settings and set your Steam directory inside the CrossOver bottle.".to_string());
        }
        return Err("No Steam installation found (Native macOS or CrossOver)".to_string());
    }

    let normalized_id = normalize_for_matching(&game_identifier);
    eprintln!("[get_game_path] platform={:?} Looking for game: {} (normalized: {})", platform, game_identifier, normalized_id);

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
                    if normalized_folder == normalized_id || 
                       normalized_folder.contains(&normalized_id) || 
                       normalized_id.contains(&normalized_folder) {
                        let game_path = entry.path().to_string_lossy().to_string();
                        eprintln!("[get_game_path] Found match: {} -> {}", folder_name, game_path);
                        return Ok(Some(game_path));
                    }
                }
            }
        }
    }

    eprintln!("[get_game_path] No match found for: {}", game_identifier);
    Ok(None)
}

#[command]
pub async fn set_game_path(app: AppHandle, game_identifier: String, path: String, platform: Option<String>) -> Result<(), String> {
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
pub async fn open_game_folder(app: AppHandle, game_identifier: String, platform: Option<String>) -> Result<(), String> {
    let settings = load_settings_impl(&app);
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");
    
    // Check manual override first
    for key in manual_override_keys(&game_identifier, platform) {
        if let Some(path) = settings.game_paths.get(&key) {
            let path_obj = std::path::Path::new(path);
            if !path_obj.exists() {
                continue;
            }
            if platform.is_some() && key == game_identifier && !manual_path_matches_platform(path_obj, is_windows_profile) {
                continue;
            }
            let _ = open::that(path_obj);
            return Ok(());
        }
    }

    let mut steam_paths_to_check = Vec::new();
    
    if !is_windows_profile {
        if let Some(home) = dirs::home_dir() {
            let mac_steam = home.join("Library/Application Support/Steam");
            if mac_steam.exists() {
                steam_paths_to_check.push(mac_steam);
            }
        }
    }

    if let Some(steam_path_str) = &settings.steam_path {
        let crossover_steam = std::path::PathBuf::from(steam_path_str);
        if crossover_steam.exists() {
            steam_paths_to_check.push(crossover_steam);
        }
    }

    if steam_paths_to_check.is_empty() {
        return Err("No Steam installation found (Native macOS or CrossOver)".to_string());
    }

    for base_steam in steam_paths_to_check {
        for lib_folder in get_steam_library_folders(&base_steam) {
            let common = lib_folder.join("steamapps").join("common");
            if !common.exists() {
                continue;
            }
            
            if let Ok(entries) = fs::read_dir(&common) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        if folder_name.to_lowercase().contains(&game_identifier.to_lowercase()) {
                            let game_path = entry.path();
                            let _ = open::that(&game_path);
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
    
    Err("Game directory not found".to_string())
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
    
    // Fallback: look for .exe (for Wine/Proton)
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".exe") && !name.contains("UnityCrashHandler") {
                return Ok(Some(entry.path().to_string_lossy().to_string()));
            }
        }
    }
    
    Ok(None)
}

#[command]
pub async fn install_to_game(app: AppHandle, game_identifier: String, profile_id: String, disabled_mods: Vec<String>, is_vanilla_override: Option<bool>) -> Result<(), String> {
    let profile_platform = get_profile_platform(&app, &profile_id);
    let is_mac_profile = profile_platform == "mac";

    // 1. Find game path
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), Some(profile_platform)).await?
        .ok_or("Game not found in Steam library")?;
    let game_path = std::path::Path::new(&game_path_str);

    // 2. Get profile path
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);

    // In non-legacy mode, profile dir might not exist - that's OK for vanilla mode
    // if !profile_dir.exists() {
    //     return Err("Profile not found".to_string());
    // }

    // Check is_vanilla - prefer override from frontend (for timing issues)
    let is_vanilla = if let Some(override_val) = is_vanilla_override {
        eprintln!("[install_to_game] Using is_vanilla override: {}", override_val);
        override_val
    } else {
        // Fallback: Read from profiles.json
        let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
        let mut vanilla = false;
        if profiles_path.exists() {
             if let Ok(data) = fs::read_to_string(&profiles_path) {
                 if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                     if let Some(p) = profiles.iter().find(|p| p["id"].as_str() == Some(&profile_id)) {
                         vanilla = p["is_vanilla"].as_bool().unwrap_or(false);
                     }
                 }
             }
        }
        vanilla
    };

    eprintln!("[install_to_game] Profile is_mac_profile: {}", is_mac_profile);

    if is_vanilla {
        eprintln!("[install_to_game] Profile is in VANILLA mode. Cleaning game folder.");
    }

    eprintln!("[install_to_game] Disabled mods: {:?}", disabled_mods);

    // --- FIX BEPINEX STRUCTURE START ---
    if !is_vanilla {
    // Always check for BepInExPack in plugins and ensure it's properly installed at root
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");
    if plugins_dir.exists() {
        let bepinex_sentinel_file = if is_mac_profile { "doorstop_libs" } else { "winhttp.dll" };
        let sentinel_is_dir = is_mac_profile; // doorstop_libs is a directory, winhttp.dll is a file
        
        let mut best_candidate: Option<(std::path::PathBuf, i32)> = None;
        
        if let Ok(entries) = fs::read_dir(&plugins_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_dir() { continue; }

                let folder_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                
                // Pattern 1: Nested BepInExPack (Standard Thunderstore structure)
                let nested_pack = path.join("BepInExPack");
                let sentinel_exists = if sentinel_is_dir {
                    nested_pack.join(bepinex_sentinel_file).is_dir()
                } else {
                    nested_pack.join(bepinex_sentinel_file).exists()
                };
                if sentinel_exists {
                    eprintln!("[install_to_game] Found nested BepInExPack candidate: {:?}", nested_pack);
                    let score = 3;
                    if best_candidate.as_ref().map_or(true, |(_, s)| score > *s) {
                        best_candidate = Some((nested_pack, score));
                    }
                    continue; 
                }
                
                // Pattern 2: Direct folder
                let direct_sentinel_exists = if sentinel_is_dir {
                    path.join(bepinex_sentinel_file).is_dir()
                } else {
                    path.join(bepinex_sentinel_file).exists()
                };
                if direct_sentinel_exists {
                    let mut score = 1;
                    if folder_name.contains("bepinex") { score += 1; }
                    eprintln!("[install_to_game] Found direct BepInExPack candidate: {:?} (score: {})", path, score);
                    if best_candidate.as_ref().map_or(true, |(_, s)| score > *s) {
                         best_candidate = Some((path.clone(), score));
                    }
                    continue;
                }

                // Pattern 3: Search subdirectories
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.filter_map(|e| e.ok()) {
                        let sub_path = sub_entry.path();
                        let sub_sentinel_exists = if sentinel_is_dir {
                            sub_path.is_dir() && sub_path.join(bepinex_sentinel_file).is_dir()
                        } else {
                            sub_path.is_dir() && sub_path.join(bepinex_sentinel_file).exists()
                        };
                        if sub_sentinel_exists {
                             let sub_name = sub_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                             let mut score = 1;
                             if sub_name.contains("bepinex") { score += 1; }
                             if best_candidate.as_ref().map_or(true, |(_, s)| score > *s) {
                                best_candidate = Some((sub_path, score));
                             }
                        }
                    }
                }
            }
        }

        if let Some((pack_dir, score)) = best_candidate {
            eprintln!("[install_to_game] Selected BepInExPack: {:?} (score: {})", pack_dir, score);
            
            if is_mac_profile {
                // === macOS: copy doorstop_libs/ + run_bepinex.sh + doorstop_config.ini ===
                // The actual BepInEx Unix pack structure:
                // - doorstop_libs/ (folder with all dylib/so files)
                // - run_bepinex.sh
                // - doorstop_config.ini
                let doorstop_libs_src = pack_dir.join("doorstop_libs");
                let doorstop_libs_dst = profile_dir.join("doorstop_libs");
                if doorstop_libs_src.exists() && !doorstop_libs_dst.exists() {
                    eprintln!("[install_to_game] Copying doorstop_libs/ to profile root");
                    copy_dir_recursive(&doorstop_libs_src, &doorstop_libs_dst)
                        .map_err(|e| format!("Failed to copy doorstop_libs: {}", e))?;
                }
                
                let run_script_src = pack_dir.join("run_bepinex.sh");
                let run_script_dst = profile_dir.join("run_bepinex.sh");
                if run_script_src.exists() && !run_script_dst.exists() {
                    eprintln!("[install_to_game] Copying run_bepinex.sh to profile root");
                    fs::copy(&run_script_src, &run_script_dst)
                        .map_err(|e| format!("Failed to copy run_bepinex.sh: {}", e))?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = fs::metadata(&run_script_dst).map_err(|e| e.to_string())?.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(&run_script_dst, perms);
                    }
                }

                // Also copy doorstop_config.ini for macOS
                let doorstop_cfg_src = pack_dir.join("doorstop_config.ini");
                let doorstop_cfg_dst = profile_dir.join("doorstop_config.ini");
                if doorstop_cfg_src.exists() && !doorstop_cfg_dst.exists() {
                    eprintln!("[install_to_game] Copying doorstop_config.ini (macOS) to profile root");
                    fs::copy(&doorstop_cfg_src, &doorstop_cfg_dst)
                        .map_err(|e| format!("Failed to copy doorstop_config.ini: {}", e))?;
                }
            } else {
                // === Windows: copy winhttp.dll + doorstop_config.ini ===
                let winhttp_src = pack_dir.join("winhttp.dll");
                let winhttp_dst = profile_dir.join("winhttp.dll");
                if winhttp_src.exists() && !winhttp_dst.exists() {
                    eprintln!("[install_to_game] Copying winhttp.dll to profile root");
                    fs::copy(&winhttp_src, &winhttp_dst)
                        .map_err(|e| format!("Failed to copy winhttp.dll: {}", e))?;
                }
                
                let doorstop_src = pack_dir.join("doorstop_config.ini");
                let doorstop_dst = profile_dir.join("doorstop_config.ini");
                if doorstop_src.exists() && !doorstop_dst.exists() {
                    eprintln!("[install_to_game] Copying doorstop_config.ini to profile root");
                    fs::copy(&doorstop_src, &doorstop_dst)
                        .map_err(|e| format!("Failed to copy doorstop_config.ini: {}", e))?;
                    // FORCE ENABLE DOORSTOP
                    if let Ok(content) = fs::read_to_string(&doorstop_dst) {
                        if !content.contains("enabled=true") && !content.contains("enabled = true") {
                             eprintln!("[install_to_game] Enforcing enabled=true in doorstop_config.ini");
                             let new_content = content.replace("enabled=false", "enabled=true")
                                                      .replace("enabled = false", "enabled = true");
                             let _ = fs::write(&doorstop_dst, new_content);
                        }
                    }
                }
            }

            // Merge BepInEx core/config from the pack (shared for both platforms)
            let pack_bepinex = pack_dir.join("BepInEx");
            if pack_bepinex.exists() {
                eprintln!("[install_to_game] Merging BepInEx core/config from pack...");
                let target_bepinex = profile_dir.join("BepInEx");
                copy_dir_recursive(&pack_bepinex, &target_bepinex)
                    .map_err(|e| format!("Failed to merge BepInEx folder: {}", e))?;
            }
        } else {
            eprintln!("[install_to_game] Warning: No BepInExPack found in plugins for {}!",
                if is_mac_profile { "macOS (looking for libdoorstop.dylib)" } else { "Windows (looking for winhttp.dll)" });
        }
    }
    } // End if !is_vanilla
    // --- FIX BEPINEX STRUCTURE END ---

    eprintln!("[install_to_game] Installing profile {} to game {}", profile_id, game_path.display());

    // --- SYNC: Remove mods from game that are not in profile OR are disabled ---
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");
    let game_plugins = game_path.join("BepInEx").join("plugins");
    
    // Create set of enabled mod names (lowercase for comparison)
    let disabled_set: std::collections::HashSet<String> = disabled_mods.iter()
        .map(|s| s.to_lowercase())
        .collect();
    
    if is_vanilla {
        // Vanilla mode: RENAME BepInEx folder to BepInEx_DISABLED (preserves mods!)
        let bepinex_folder = game_path.join("BepInEx");
        let bepinex_disabled = game_path.join("BepInEx_DISABLED");
        
        if bepinex_folder.exists() {
            // If disabled folder already exists, remove it first
            if bepinex_disabled.exists() {
                eprintln!("[install_to_game] Vanilla mode: Removing old disabled folder");
                let _ = fs::remove_dir_all(&bepinex_disabled);
            }
            eprintln!("[install_to_game] Vanilla mode: Renaming BepInEx -> BepInEx_DISABLED");
            fs::rename(&bepinex_folder, &bepinex_disabled)
                .map_err(|e| format!("Failed to disable BepInEx: {}", e))?;
        }
    } else {
        // Normal mode: Check if BepInEx_DISABLED exists and restore it
        let bepinex_folder = game_path.join("BepInEx");
        let bepinex_disabled = game_path.join("BepInEx_DISABLED");
        
        if bepinex_disabled.exists() && !bepinex_folder.exists() {
            eprintln!("[install_to_game] Restoring BepInEx_DISABLED -> BepInEx");
            fs::rename(&bepinex_disabled, &bepinex_folder)
                .map_err(|e| format!("Failed to restore BepInEx: {}", e))?;
        }
    }
    
    if !is_vanilla && profile_plugins.exists() && game_plugins.exists() {
        // Get list of ENABLED mod folders in profile
        let enabled_profile_mods: std::collections::HashSet<String> = fs::read_dir(&profile_plugins)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter_map(|e| {
                        let name = e.file_name().to_str().map(|s| s.to_string())?;
                        // Check if this mod is disabled
                        let is_disabled = disabled_set.iter().any(|d| name.to_lowercase().contains(d));
                        if is_disabled {
                            eprintln!("[install_to_game] Skipping disabled mod in profile: {}", name);
                            None
                        } else {
                            Some(name)
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        
        // Check game plugins and remove those not in profile OR disabled
        if let Ok(game_entries) = fs::read_dir(&game_plugins) {
            for entry in game_entries.filter_map(|e| e.ok()) {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    
                    // Check if mod is disabled or not in enabled list
                    let is_disabled = disabled_set.iter().any(|d| folder_name.to_lowercase().contains(d));
                    let not_in_profile = !enabled_profile_mods.contains(&folder_name);
                    
                    if is_disabled || not_in_profile {
                        eprintln!("[install_to_game] Removing mod from game (disabled={}, orphan={}): {}", 
                                  is_disabled, not_in_profile, folder_name);
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
    }
    // --- END SYNC ---

    if !is_vanilla {
    // 3. Copy BepInEx structure with filtering for disabled mods
    let source_bepinex = profile_dir.join("BepInEx");
    let dest_bepinex = game_path.join("BepInEx");
    
    if source_bepinex.exists() {
        // Create BepInEx dir if needed
        if !dest_bepinex.exists() {
            fs::create_dir_all(&dest_bepinex).map_err(|e| e.to_string())?;
        }
        
        // Copy everything except plugins (we'll handle that specially)
        if let Ok(entries) = fs::read_dir(&source_bepinex) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                let src_path = entry.path();
                let dst_path = dest_bepinex.join(&name);
                
                if name == "plugins" {
                    // Handle plugins specially - use SYMLINKS to save disk space!
                    if !dst_path.exists() {
                        fs::create_dir_all(&dst_path).map_err(|e| e.to_string())?;
                    }
                    
                    // First, clean up any plugins in destination that are no longer in source or are disabled
                    if let Ok(dest_entries) = fs::read_dir(&dst_path) {
                        for dest_entry in dest_entries.filter_map(|e| e.ok()) {
                            let dest_plugin_name = dest_entry.file_name().to_string_lossy().to_string();
                            let source_plugin_path = src_path.join(&dest_plugin_name);
                            let is_disabled = disabled_set.iter().any(|d| dest_plugin_name.to_lowercase().contains(d));
                            
                            // Remove if disabled or not in source
                            if is_disabled || !source_plugin_path.exists() {
                                let dest_plugin_path = dest_entry.path();
                                if dest_plugin_path.is_symlink() {
                                    let _ = fs::remove_file(&dest_plugin_path);
                                } else if dest_plugin_path.is_dir() {
                                    let _ = fs::remove_dir_all(&dest_plugin_path);
                                } else {
                                    let _ = fs::remove_file(&dest_plugin_path);
                                }
                                eprintln!("[install_to_game] Removed old/disabled plugin: {}", dest_plugin_name);
                            }
                        }
                    }
                    
                    // Now create symlinks for enabled plugins
                    if let Ok(plugin_entries) = fs::read_dir(&src_path) {
                        for plugin_entry in plugin_entries.filter_map(|e| e.ok()) {
                            let plugin_name = plugin_entry.file_name().to_string_lossy().to_string();
                            
                            // Check if this plugin is disabled
                            let is_disabled = disabled_set.iter().any(|d| plugin_name.to_lowercase().contains(d));
                            
                            if is_disabled {
                                eprintln!("[install_to_game] Skipping disabled plugin: {}", plugin_name);
                                continue;
                            }
                            
                            let plugin_dst = dst_path.join(&plugin_name);
                            let plugin_src = plugin_entry.path();
                            
                            // Remove existing file/dir/symlink if present
                            if plugin_dst.exists() || plugin_dst.is_symlink() {
                                if plugin_dst.is_symlink() {
                                    let _ = fs::remove_file(&plugin_dst);
                                } else if plugin_dst.is_dir() {
                                    let _ = fs::remove_dir_all(&plugin_dst);
                                } else {
                                    let _ = fs::remove_file(&plugin_dst);
                                }
                            }
                            
                            // Create symlink instead of copying
                            #[cfg(unix)]
                            {
                                std::os::unix::fs::symlink(&plugin_src, &plugin_dst)
                                    .map_err(|e| format!("Failed to create symlink for {}: {}", plugin_name, e))?;
                                eprintln!("[install_to_game] Created symlink: {} -> {:?}", plugin_name, plugin_src);
                            }
                            
                            #[cfg(windows)]
                            {
                                // Fallback to copy on Windows (symlinks require admin)
                                if plugin_src.is_dir() {
                                    copy_dir_recursive(&plugin_src, &plugin_dst)
                                        .map_err(|e| format!("Failed to copy plugin {}: {}", plugin_name, e))?;
                                } else {
                                    fs::copy(&plugin_src, &plugin_dst)
                                        .map_err(|e| format!("Failed to copy plugin file {}: {}", plugin_name, e))?;
                                }
                            }
                        }
                    }
                } else {
                    // Copy other BepInEx folders normally
                    if src_path.is_dir() {
                        copy_dir_recursive(&src_path, &dst_path)
                            .map_err(|e| format!("Failed to copy {}: {}", name, e))?;
                    } else {
                        if dst_path.exists() {
                            let _ = fs::remove_file(&dst_path);
                        }
                        fs::copy(&src_path, &dst_path)
                            .map_err(|e| format!("Failed to copy {}: {}", name, e))?;
                    }
                }
            }
        }
        eprintln!("[install_to_game] Synced BepInEx to game folder");
    }
    } // End if !is_vanilla

    // 4. Rename (or restore) root files based on platform
    let root_files: &[&str] = if is_mac_profile {
        &["libdoorstop.dylib", "run_bepinex.sh"]
    } else {
        &["doorstop_config.ini", "winhttp.dll"]
    };
    
    for item_name in root_files {
        let dest = game_path.join(item_name);
        let disabled_name = format!("{}_DISABLED", item_name);
        let disabled_dest = game_path.join(&disabled_name);
        
        if is_vanilla {
            // Rename to _DISABLED instead of deleting
            if dest.exists() {
                if disabled_dest.exists() {
                    let _ = fs::remove_file(&disabled_dest);
                }
                let _ = fs::rename(&dest, &disabled_dest);
                eprintln!("[install_to_game] Vanilla mode: Renamed {} -> {}", item_name, disabled_name);
            }
        } else {
            // Restore from _DISABLED if it exists
            if disabled_dest.exists() && !dest.exists() {
                let _ = fs::rename(&disabled_dest, &dest);
                eprintln!("[install_to_game] Restored {} from disabled", item_name);
            }
            
            // Copy from profile if needed
            let source = profile_dir.join(item_name);
            if source.exists() && !dest.exists() {
                fs::copy(&source, &dest).map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
                eprintln!("[install_to_game] Synced {} to game folder", item_name);
                
                // For run_bepinex.sh: make executable and patch executable_name
                if *item_name == "run_bepinex.sh" {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = fs::metadata(&dest).map_err(|e| e.to_string())?.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(&dest, perms);
                    }
                    // Patch executable_name in the script to match the .app in game dir
                    if let Ok(script_content) = fs::read_to_string(&dest) {
                        // Find .app bundle in game directory
                        let app_name = fs::read_dir(game_path)
                            .ok()
                            .and_then(|entries| {
                                entries.filter_map(|e| e.ok())
                                    .find(|e| e.file_name().to_string_lossy().ends_with(".app"))
                                    .map(|e| e.file_name().to_string_lossy().to_string())
                            });
                        if let Some(app) = app_name {
                            eprintln!("[install_to_game] Patching run_bepinex.sh with executable: {}", app);
                            let patched = script_content
                                .replace("executable_name=\"\"", &format!("executable_name=\"{}\"", app))
                                .replace("executable_name=", &format!("executable_name=\"{}\"", app));
                            // Only write if not already patched
                            if patched != script_content {
                                let _ = fs::write(&dest, patched);
                            }
                        }
                    }
                }
            }
        }
    }

    eprintln!("[install_to_game] Sync complete!");
    Ok(())
}

#[command]
pub async fn launch_game_with_mods(app: AppHandle, game_identifier: String, platform: Option<String>) -> Result<(), String> {
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);
    let run_script = game_path.join("run_bepinex.sh");
    
    if run_script.exists() {
        // Make sure it's executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&run_script).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&run_script, perms);
        }
        
        // Patch executable_name if needed
        if let Ok(script_content) = fs::read_to_string(&run_script) {
            if script_content.contains("executable_name=\"\"")
                || script_content.contains("executable_name= ") {
                // Find .app in game directory
                let app_name = fs::read_dir(&game_path)
                    .ok()
                    .and_then(|entries| {
                        entries.filter_map(|e| e.ok())
                            .find(|e| e.file_name().to_string_lossy().ends_with(".app"))
                            .map(|e| e.file_name().to_string_lossy().to_string())
                    });
                if let Some(app_bundle) = app_name {
                    let patched = script_content
                        .replace("executable_name=\"\"", &format!("executable_name=\"{}\"", app_bundle));
                    if patched != script_content {
                        eprintln!("[launch_game_with_mods] Patching run_bepinex.sh: executable_name={}", app_bundle);
                        let _ = fs::write(&run_script, &patched);
                    }
                }
            }
        }
        
        eprintln!("[launch_game_with_mods] Launching via run_bepinex.sh at {:?}", run_script);
        
        std::process::Command::new(&run_script)
            .current_dir(&game_path)
            .spawn()
            .map_err(|e| format!("Failed to launch run_bepinex.sh: {}", e))?;
        
        Ok(())
    } else {
        // Fallback: open the .app directly
        let app_bundle = fs::read_dir(&game_path)
            .ok()
            .and_then(|entries| {
                entries.filter_map(|e| e.ok())
                    .find(|e| e.file_name().to_string_lossy().ends_with(".app"))
                    .map(|e| e.path())
            });
        if let Some(bundle) = app_bundle {
            let _ = open::that(&bundle);
            Ok(())
        } else {
            Err("run_bepinex.sh not found and no .app bundle found either".to_string())
        }
    }
}

#[command]
pub async fn sync_profile_to_game(app: AppHandle, profile_id: String, game_identifier: String, use_legacy_cache: Option<bool>) -> Result<serde_json::Value, String> {
    let use_cache = use_legacy_cache.unwrap_or(false);

    // 1. Read profile mods and platform from profiles.json
    let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> = serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;
    
    let profile = profiles.iter()
        .find(|p| p["id"].as_str() == Some(&profile_id))
        .ok_or("Profile not found")?;
    let profile_platform = profile["platform"].as_str().unwrap_or("windows").to_string();

    // 2. Get game path for this specific profile platform
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), Some(profile_platform)).await?
        .ok_or("Game path not configured. Please set it in Settings.")?;
    let game_path = std::path::Path::new(&game_path_str);
    let game_plugins = game_path.join("BepInEx").join("plugins");
    
    // Profile cache path
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");

    eprintln!("[sync_profile_to_game] Syncing profile {} to game {:?} (legacy_cache: {})", profile_id, game_path, use_cache);
    
    // Get list of mod names from profile (format: "Author-ModName-Version")
    // We keep the full name for matching
    // IMPORTANT: Only include ENABLED mods (disabled mods should not be installed)
    let profile_mod_full_names: Vec<String> = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|m| m["enabled"].as_bool().unwrap_or(true)) // Only enabled mods (default true for backwards compat)
        .filter_map(|m| m["fullName"].as_str().map(|s| s.to_string()))
        .collect();
    
    // Also create a set of "Author-ModName" keys for fuzzy matching
    let profile_mod_keys: Vec<String> = profile_mod_full_names.iter()
        .map(|s| {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() >= 2 {
                format!("{}-{}", parts[0], parts[1])
            } else {
                s.clone()
            }
        })
        .collect();

    eprintln!("[sync_profile_to_game] Profile has {} mods", profile_mod_full_names.len());

    // 3. Scan game plugins folder for currently installed mods
    // Store both the folder name AND the derived key
    let mut game_mod_folders: Vec<(String, String)> = vec![]; // (folder_name, author-modname key)
    if game_plugins.exists() {
        if let Ok(entries) = fs::read_dir(&game_plugins) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    // Extract "Author-ModName" from folder (format: Author-ModName-Version)
                    let parts: Vec<&str> = folder_name.split('-').collect();
                    let mod_key = if parts.len() >= 2 {
                        format!("{}-{}", parts[0], parts[1])
                    } else {
                        folder_name.clone()
                    };
                    game_mod_folders.push((folder_name, mod_key));
                }
            }
        }
    }

    eprintln!("[sync_profile_to_game] Game has {} mods installed", game_mod_folders.len());

    // 4. Calculate diff using the Author-ModName keys for comparison
    
    // to_remove: in game but not in profile (by key)
    let to_remove: Vec<&(String, String)> = game_mod_folders.iter()
        .filter(|(_, gm_key)| !profile_mod_keys.iter().any(|pm_key| pm_key.to_lowercase() == gm_key.to_lowercase()))
        .collect();

    // to_install: in profile but not in game (by key)
    // Special case: BepInExPack installs to game root, not plugins - check if BepInEx folder exists
    let bepinex_installed = game_path.join("BepInEx").join("core").exists();
    
    let to_install: Vec<&String> = profile_mod_keys.iter()
        .filter(|pm_key| {
            // Skip BepInExPack if BepInEx is already installed
            if pm_key.to_lowercase().contains("bepinex") && bepinex_installed {
                return false;
            }
            // Check if not already in game plugins
            !game_mod_folders.iter().any(|(_, gm_key)| gm_key.to_lowercase() == pm_key.to_lowercase())
        })
        .collect();

    eprintln!("[sync_profile_to_game] To remove: {:?}, To install: {:?}", to_remove.len(), to_install.len());

    // 5. Remove mods not in profile (we have the exact folder names from the tuple)
    let mut removed = 0;
    for (folder_name, _key) in &to_remove {
        let folder_path = game_plugins.join(folder_name);
        if folder_path.exists() {
            eprintln!("[sync_profile_to_game] Removing: {}", folder_name);
            let _ = fs::remove_dir_all(&folder_path);
            removed += 1;
        }
    }

    // 6. If legacy cache enabled, copy mods from game to profile cache (reverse sync)
    let mut cached = 0;
    if use_cache && game_plugins.exists() {
        // Create profile plugins dir if needed
        if !profile_plugins.exists() {
            let _ = fs::create_dir_all(&profile_plugins);
        }
        
        // Iterate game mods and copy to cache if not present
        if let Ok(entries) = fs::read_dir(&game_plugins) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    let cache_path = profile_plugins.join(&folder_name);
                    
                    // Only copy if not already cached
                    if !cache_path.exists() {
                        eprintln!("[sync_profile_to_game] Caching mod from game: {}", folder_name);
                        if copy_dir_recursive(&entry.path(), &cache_path).is_ok() {
                            cached += 1;
                        }
                    }
                }
            }
        }
        
        if cached > 0 {
            eprintln!("[sync_profile_to_game] Cached {} mods from game to profile", cached);
        }
    }

    // 7. Return info about what needs to be installed (frontend will handle download)
    let to_install_names: Vec<String> = to_install.iter().map(|s| s.to_string()).collect();
    let already_installed = game_mod_folders.len() - removed;

    Ok(serde_json::json!({
        "removed": removed,
        "to_install": to_install_names,
        "already_installed": already_installed,
        "cached": cached
    }))
}
