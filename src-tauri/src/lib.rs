use tauri::{command, AppHandle, Manager};
use std::{fs, sync::{Arc, Mutex}, collections::HashMap};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mod {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    // Add other fields as needed
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mods: Vec<Mod>,
    // Add other fields
}

#[command]
fn get_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let profile_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profile_path.exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(profile_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(profiles)
}

#[command]
fn save_profiles(app: AppHandle, profiles: Vec<serde_json::Value>) -> Result<bool, String> {
    let profile_path = app.path().app_data_dir().unwrap().join("profiles.json");
    // Ensure dir exists
    if let Some(parent) = profile_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?;
    fs::write(profile_path, data).map_err(|e| e.to_string())?;
    Ok(true)
}

#[command]
async fn fetch_communities() -> Result<Vec<serde_json::Value>, String> {
    let mut url = Some("https://thunderstore.io/api/experimental/community/".to_string());
    let mut all_results = Vec::new();

    while let Some(current_url) = url {
        let resp = reqwest::get(&current_url).await.map_err(|e| e.to_string())?;
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        
        if let Some(results) = json.get("results").and_then(|v| v.as_array()) {
            eprintln!("[fetch_communities] Fetched {} communities from {}", results.len(), current_url);
            all_results.extend(results.clone());
        }

        // API uses pagination.next_link instead of "next"
        url = json.get("pagination")
            .and_then(|p| p.get("next_link"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    
    eprintln!("[fetch_communities] Total communities fetched: {}", all_results.len());
    Ok(all_results)
}

#[command]
async fn fetch_community_images() -> Result<std::collections::HashMap<String, String>, String> {
    let url = "https://thunderstore.io/communities/";
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let html = resp.text().await.map_err(|e| e.to_string())?;
    
    let mut images = std::collections::HashMap::new();
    
    // Regex for preload links
    // Matches: <link rel="preload" href="https://gcdn.thunderstore.io/live/community/risk-of-rain-2/..." as="image">
    let re_preload = regex::Regex::new(r#"<link rel="preload" href="(https://gcdn\.thunderstore\.io/live/community/([^/]+)/[^"]+)" as="image">"#)
        .map_err(|e| e.to_string())?;
    
    for cap in re_preload.captures_iter(&html) {
        if let (Some(url), Some(id)) = (cap.get(1), cap.get(2)) {
            images.insert(id.as_str().to_string(), url.as_str().to_string());
        }
    }

    // Regex for img tags (fallback)
    // Matches: <img ... src="https://gcdn.thunderstore.io/live/community/risk-of-rain-2/..." ...>
    let re_img = regex::Regex::new(r#"src="(https://gcdn\.thunderstore\.io/live/community/([^/]+)/[^"]+)""#)
        .map_err(|e| e.to_string())?;
        
    for cap in re_img.captures_iter(&html) {
        if let (Some(url), Some(id)) = (cap.get(1), cap.get(2)) {
             images.entry(id.as_str().to_string()).or_insert(url.as_str().to_string());
        }
    }
    
    Ok(images)
}

// AppState to hold packages in memory
struct AppState {
    // Cache: GameID -> List of Packages (wrapped in Arc for sharing across tasks)
    packages: Arc<Mutex<HashMap<String, Vec<serde_json::Value>>>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Settings {
    steam_path: Option<String>,
    #[serde(default)]
    favorite_games: Vec<String>,
    #[serde(default)]
    game_paths: HashMap<String, String>,
}

impl Settings {
    fn default() -> Self {
        // Default Steam path on macOS
        let home = dirs::home_dir().unwrap_or_default();
        let steam_path = home.join("Library/Application Support/Steam");
        Self {
            steam_path: if steam_path.exists() { Some(steam_path.to_string_lossy().to_string()) } else { None },
            favorite_games: Vec::new(),
            game_paths: HashMap::new(),
        }
    }
}

fn get_settings_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap().join("settings.json")
}

fn load_settings_impl(app: &AppHandle) -> Settings {
    let path = get_settings_path(app);
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str(&data) {
                return settings;
            }
        }
    }
    Settings::default()
}

#[command]
async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(load_settings_impl(&app))
}

fn save_settings_impl(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = get_settings_path(app);
    let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(())
}

#[command]
async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    save_settings_impl(&app, &settings)
}

/// Normalize a string for fuzzy matching: lowercase, remove non-alphanumeric
fn normalize_for_matching(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Get all Steam library folders
fn get_steam_library_folders(steam_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut folders = vec![steam_path.to_path_buf()];
    
    let library_folders_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    if library_folders_path.exists() {
        if let Ok(content) = fs::read_to_string(&library_folders_path) {
            let re = regex::Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
            for cap in re.captures_iter(&content) {
                folders.push(std::path::PathBuf::from(&cap[1]));
            }
        }
    }
    
    folders
}

#[command]
async fn get_game_path(app: AppHandle, game_identifier: String) -> Result<Option<String>, String> {
    let settings = load_settings_impl(&app);

    // Check manual override first
    if let Some(path) = settings.game_paths.get(&game_identifier) {
        if std::path::Path::new(path).exists() {
            eprintln!("[get_game_path] Found manual override: {}", path);
            return Ok(Some(path.clone()));
        }
    }

    let steam_path_str = settings.steam_path.ok_or("Steam path not configured")?;
    let steam_path = std::path::Path::new(&steam_path_str);

    let normalized_id = normalize_for_matching(&game_identifier);
    eprintln!("[get_game_path] Looking for game: {} (normalized: {})", game_identifier, normalized_id);

    // Scan all Steam library folders
    for lib_folder in get_steam_library_folders(steam_path) {
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

    eprintln!("[get_game_path] No match found for: {}", game_identifier);
    Ok(None)
}

#[command]
async fn set_game_path(app: AppHandle, game_identifier: String, path: String) -> Result<(), String> {
    let mut settings = load_settings_impl(&app);
    settings.game_paths.insert(game_identifier, path);
    save_settings_impl(&app, &settings)?;
    Ok(())
}

#[command]
async fn open_game_folder(app: AppHandle, game_identifier: String) -> Result<(), String> {
    let settings = load_settings_impl(&app);
    
    // Check manual override first
    if let Some(path) = settings.game_paths.get(&game_identifier) {
        let path_obj = std::path::Path::new(path);
        if path_obj.exists() {
             let _ = open::that(path_obj);
             return Ok(());
        }
    }

    if let Some(steam_path_str) = settings.steam_path {
        let steam_path = std::path::Path::new(&steam_path_str);
        
        for lib_folder in get_steam_library_folders(steam_path) {
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
        
        return Err("Game directory not found".to_string());
    }
    
    Err("Steam path not configured".to_string())
}


#[command]
async fn find_game_executable(game_path: String) -> Result<Option<String>, String> {
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
/// Find Steam App ID by matching the game folder name in appmanifest files
fn find_steam_app_id(steam_path: &std::path::Path, game_folder: &str) -> Option<String> {
    for lib_folder in get_steam_library_folders(steam_path) {
        let steamapps = lib_folder.join("steamapps");
        if let Ok(entries) = fs::read_dir(&steamapps) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("appmanifest_") && name.ends_with(".acf") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        // Parse installdir from manifest
                        let re = regex::Regex::new(r#""installdir"\s+"([^"]+)""#).unwrap();
                        if let Some(cap) = re.captures(&content) {
                            if cap[1].to_lowercase() == game_folder.to_lowercase() {
                                // Extract app_id from filename
                                let app_id = name
                                    .strip_prefix("appmanifest_")
                                    .and_then(|s| s.strip_suffix(".acf"))
                                    .unwrap_or("");
                                return Some(app_id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[command]
async fn install_to_game(app: AppHandle, game_identifier: String, profile_id: String, disabled_mods: Vec<String>) -> Result<(), String> {
    // 1. Find game path
    let game_path_str = get_game_path(app.clone(), game_identifier.clone()).await?
        .ok_or("Game not found in Steam library")?;
    let game_path = std::path::Path::new(&game_path_str);

    // 2. Get profile path
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);

    if !profile_dir.exists() {
        return Err("Profile not found".to_string());
    }

    eprintln!("[install_to_game] Disabled mods: {:?}", disabled_mods);

    // --- FIX BEPINEX STRUCTURE START ---
    // Always check for BepInExPack in plugins and ensure it's properly installed at root
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");
    if plugins_dir.exists() {
        // Find BepInExPack folder - search for various naming patterns and PRIORITIZE content
        // Score: 3 = Explicit BepInExPack folder structure, 2 = winhttp.dll present + name match, 1 = winhttp.dll present
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
                // plugins/ModName/BepInExPack/winhttp.dll
                let nested_pack = path.join("BepInExPack");
                if nested_pack.join("winhttp.dll").exists() {
                    eprintln!("[install_to_game] Found nested BepInExPack candidate: {:?}", nested_pack);
                    let score = 3;
                    if best_candidate.as_ref().map_or(true, |(_, s)| score > *s) {
                        best_candidate = Some((nested_pack, score));
                    }
                    continue; 
                }
                
                // Pattern 2: Direct or subdirectory check
                // Check if this folder ITSELF is the pack
                if path.join("winhttp.dll").exists() {
                    let mut score = 1;
                    if folder_name.contains("bepinex") { score += 1; }
                    
                    eprintln!("[install_to_game] Found direct BepInExPack candidate: {:?} (score: {})", path, score);
                    if best_candidate.as_ref().map_or(true, |(_, s)| score > *s) {
                         best_candidate = Some((path.clone(), score));
                    }
                    continue;
                }

                // Pattern 3: Search subdirectories (e.g. plugins/ModName/BepInExPack_GameName)
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.filter_map(|e| e.ok()) {
                        let sub_path = sub_entry.path();
                         if sub_path.is_dir() && sub_path.join("winhttp.dll").exists() {
                             let sub_name = sub_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                             let mut score = 1;
                             if sub_name.contains("bepinex") { score += 1; }
                             
                             eprintln!("[install_to_game] Found subdirectory BepInExPack candidate: {:?} (score: {})", sub_path, score);
                             
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
            
            // 1. Ensure winhttp.dll is at profile root
            let winhttp_src = pack_dir.join("winhttp.dll");
            let winhttp_dst = profile_dir.join("winhttp.dll");
            if winhttp_src.exists() && !winhttp_dst.exists() {
                eprintln!("[install_to_game] Copying winhttp.dll to profile root");
                fs::copy(&winhttp_src, &winhttp_dst)
                    .map_err(|e| format!("Failed to copy winhttp.dll: {}", e))?;
            }
            
            // 2. Ensure doorstop_config.ini is at profile root
            let doorstop_src = pack_dir.join("doorstop_config.ini");
            let doorstop_dst = profile_dir.join("doorstop_config.ini");
            if doorstop_src.exists() && !doorstop_dst.exists() {
                eprintln!("[install_to_game] Copying doorstop_config.ini to profile root");
                fs::copy(&doorstop_src, &doorstop_dst)
                    .map_err(|e| format!("Failed to copy doorstop_config.ini: {}", e))?;
                
                // FORCE ENABLE DOORSTOP
                // We read the file, verify 'enabled=true', and write it back if needed
                 if let Ok(content) = fs::read_to_string(&doorstop_dst) {
                    if !content.contains("enabled=true") && !content.contains("enabled = true") {
                         eprintln!("[install_to_game] Enforcing enabled=true in doorstop_config.ini");
                         // Simple replacement or append if missing. 
                         // To be safe, let's just replace "enabled=false" with "enabled=true" or ensure it's there.
                         let new_content = content.replace("enabled=false", "enabled=true")
                                                  .replace("enabled = false", "enabled = true");
                         let _ = fs::write(&doorstop_dst, new_content);
                    }
                 }
            }

            // 3. Merge BepInEx core/config from the pack (if present)
            let pack_bepinex = pack_dir.join("BepInEx");
            if pack_bepinex.exists() {
                eprintln!("[install_to_game] Merging BepInEx core/config from pack...");
                let target_bepinex = profile_dir.join("BepInEx");
                copy_dir_recursive(&pack_bepinex, &target_bepinex)
                    .map_err(|e| format!("Failed to merge BepInEx folder: {}", e))?;
            }
        } else {
            eprintln!("[install_to_game] Warning: No BepInExPack found in plugins!");
        }
    }
    // --- FIX BEPINEX STRUCTURE END ---

    eprintln!("[install_to_game] Installing profile {} to game {}", profile_id, game_path.display());

    // --- SYNC: Remove mods from game that are not in profile OR are disabled ---
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");
    let game_plugins = game_path.join("BepInEx").join("plugins");
    
    // Create set of enabled mod names (lowercase for comparison)
    let disabled_set: std::collections::HashSet<String> = disabled_mods.iter()
        .map(|s| s.to_lowercase())
        .collect();
    
    if profile_plugins.exists() && game_plugins.exists() {
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
                    // Handle plugins specially - only copy enabled mods
                    if !dst_path.exists() {
                        fs::create_dir_all(&dst_path).map_err(|e| e.to_string())?;
                    }
                    
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
                            if plugin_entry.path().is_dir() {
                                copy_dir_recursive(&plugin_entry.path(), &plugin_dst)
                                    .map_err(|e| format!("Failed to copy plugin {}: {}", plugin_name, e))?;
                            } else {
                                if plugin_dst.exists() {
                                    let _ = fs::remove_file(&plugin_dst);
                                }
                                fs::copy(&plugin_entry.path(), &plugin_dst)
                                    .map_err(|e| format!("Failed to copy plugin file {}: {}", plugin_name, e))?;
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

    // 4. Copy root files (doorstop_config.ini, winhttp.dll)
    for item_name in ["doorstop_config.ini", "winhttp.dll"].iter() {
        let source = profile_dir.join(item_name);
        let dest = game_path.join(item_name);
        
        if source.exists() {
            if dest.exists() {
                let _ = fs::remove_file(&dest);
            }
            fs::copy(&source, &dest).map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
            eprintln!("[install_to_game] Synced {} to game folder", item_name);
        }
    }

    eprintln!("[install_to_game] Sync complete!");
    Ok(())
}

// Helper function for recursive directory copy with forced overwrite
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    
    let mut files_copied = 0;
    let mut dirs_created = 0;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        
        if file_type.is_dir() {
            if !dest_path.exists() {
                fs::create_dir_all(&dest_path)?;
                dirs_created += 1;
            }
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            // Check if file exists and has same size to skip copy
            let should_copy = if dest_path.exists() {
                if let (Ok(src_meta), Ok(dst_meta)) = (fs::metadata(entry.path()), fs::metadata(&dest_path)) {
                    // Start with size check - simple and fast
                    if src_meta.len() != dst_meta.len() {
                        true
                    } else {
                        // If size is same, check modification time?
                        // For now, size is a good enough heuristic for mod files (DLLs usually don't change without size change)
                        // And we want speed.
                        // eprintln!("[copy_dir_recursive] Skipping identical file: {:?}", entry.file_name());
                        false
                    }
                } else {
                    true
                }
            } else {
                true
            };

            if should_copy {
                // Remove first if exists to avoid permission issues
                if dest_path.exists() {
                    let _ = fs::remove_file(&dest_path);
                }
                fs::copy(&entry.path(), &dest_path)?;
                files_copied += 1;
            }
        }
    }
    
    if files_copied > 0 || dirs_created > 0 {
        eprintln!("[copy_dir_recursive] {:?} -> {:?}: {} files, {} dirs", 
            src.file_name().unwrap_or_default(), 
            dst.file_name().unwrap_or_default(), 
            files_copied, dirs_created);
    }
    
    Ok(())
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            packages: Arc::new(Mutex::new(HashMap::new())),
        })

        .invoke_handler(tauri::generate_handler![
            get_profiles,
            save_profiles,
            fetch_communities,
            fetch_community_images,
            select_folder,
            select_file,
            install_mod,
            import_profile_from_file,
            import_profile,
            open_mod_folder,
            fetch_packages,
            get_packages,
            lookup_packages_by_names,
            fetch_package_by_name,
            delete_profile_folder,
            remove_mod,
            toggle_mod,
            check_directory_exists,
            export_profile,
            share_profile,
            get_settings,
            save_settings,
            get_game_path,
            set_game_path,
            read_image,
            open_game_folder,
            install_to_game,
            confirm_dialog,
            alert_dialog,
            fetch_text_content,
            check_update,
            install_update,
        ])
        .setup(|app| {
            use chrono::Datelike;
            use tauri::menu::{MenuBuilder, SubmenuBuilder, MenuItemBuilder, PredefinedMenuItem};
            use tauri::Manager; // Ensure Manager is imported for .path()

            // Clear chunks cache on startup to keep app light
            if let Ok(cache_dir) = app.path().app_cache_dir() {
                let chunks_dir = cache_dir.join("chunks");
                if chunks_dir.exists() {
                    let _ = std::fs::remove_dir_all(&chunks_dir);
                }
            }

            let current_year = chrono::Local::now().year();
            let copyright_text = format!("Copyright © {} Zard Studios", current_year);

            // Create Help menu items
            let report_issue = MenuItemBuilder::with_id("report_issue", "Report an Issue")
                .build(app)?;
            let github = MenuItemBuilder::with_id("github", "GitHub Repository")
                .build(app)?;
            
            // Build Help submenu
            let help_menu = SubmenuBuilder::new(app, "Help")
                .item(&report_issue)
                .item(&github)
                .build()?;
            
            // Build the full menu bar (macOS needs app name submenu first)
            let app_menu = SubmenuBuilder::new(app, "r2modmac")
                .item(&PredefinedMenuItem::about(
                    app,
                    Some("About r2modmac"),
                    Some(
                        tauri::menu::AboutMetadataBuilder::new()
                            .copyright(Some(copyright_text))
                            .authors(Some(vec!["Zard Studios".to_string()]))
                            .build()
                    )
                )?)
                .separator()
                .item(&PredefinedMenuItem::hide(app, Some("Hide r2modmac"))?)
                .item(&PredefinedMenuItem::hide_others(app, Some("Hide Others"))?)
                .item(&PredefinedMenuItem::show_all(app, Some("Show All"))?)
                .separator()
                .item(&PredefinedMenuItem::quit(app, Some("Quit r2modmac"))?)
                .build()?;
            
            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .item(&PredefinedMenuItem::undo(app, None)?)
                .item(&PredefinedMenuItem::redo(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::cut(app, None)?)
                .item(&PredefinedMenuItem::copy(app, None)?)
                .item(&PredefinedMenuItem::paste(app, None)?)
                .item(&PredefinedMenuItem::select_all(app, None)?)
                .build()?;
            
            let window_menu = SubmenuBuilder::new(app, "Window")
                .item(&PredefinedMenuItem::minimize(app, None)?)
                .item(&PredefinedMenuItem::close_window(app, Some("Close"))?)
                .build()?;
            
            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&edit_menu)
                .item(&window_menu)
                .item(&help_menu)
                .build()?;
            
            app.set_menu(menu)?;
            
            Ok(())
        })
        .on_menu_event(|_app, event| {
            match event.id().as_ref() {
                "report_issue" => {
                    let _ = open::that("https://github.com/Zard-Studios/r2modmac/issues/new");
                }
                "github" => {
                    let _ = open::that("https://github.com/Zard-Studios/r2modmac");
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[command]
async fn fetch_text_content(url: String) -> Result<String, String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

#[command]
async fn confirm_dialog(app: AppHandle, title: String, message: String) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_dialog::MessageDialogButtons;
    
    let ans = app.dialog()
        .message(message)
        .title(title)
        .buttons(MessageDialogButtons::OkCancel)
        .blocking_show();
        
    Ok(ans)
}

#[command]
async fn alert_dialog(app: AppHandle, title: String, message: String) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_dialog::MessageDialogButtons;
    
    app.dialog()
        .message(message)
        .title(title)
        .buttons(MessageDialogButtons::Ok)
        .blocking_show();
        
    Ok(())
}

#[command]
async fn select_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file_path = app.dialog().file().blocking_pick_folder();
    Ok(file_path.map(|p| p.to_string()))
}

#[derive(serde::Deserialize)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[command]
async fn select_file(app: AppHandle, filters: Option<Vec<FileFilter>>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();

    if let Some(fs) = filters {
        for f in fs {
            // Convert Vec<String> to Vec<&str>
            let exts: Vec<&str> = f.extensions.iter().map(|s| s.as_str()).collect();
            builder = builder.add_filter(f.name, &exts);
        }
    } else {
        // Default to r2modman profile if no filters provided
        builder = builder.add_filter("r2modman Profile", &["r2z", "zip"]);
    }

    let file_path = builder.blocking_pick_file();
    Ok(file_path.map(|p| p.to_string()))
}

#[command]
async fn read_image(path: String) -> Result<Option<String>, String> {
    use base64::Engine;
    let path_buf = std::path::PathBuf::from(&path);
    if !path_buf.exists() {
        return Ok(None);
    }
    
    let bytes = fs::read(&path_buf).map_err(|e| e.to_string())?;
    let base64_str = base64::engine::general_purpose::STANDARD.encode(&bytes);
    
    // Determine mime type based on extension
    let extension = path_buf.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let mime = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream"
    };
    
    Ok(Some(format!("data:{};base64,{}", mime, base64_str)))
}

#[command]
async fn install_mod(app: AppHandle, profile_id: String, download_url: String, mod_name: String) -> Result<serde_json::Value, String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");
    let mod_dir = plugins_dir.join(&mod_name);

    // Download
    let response = reqwest::get(&download_url).await.map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(bytes);

    // Unzip
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    
    // Check if this is BepInExPack by looking for "BepInExPack" folder at root
    let is_bepinex_pack = (0..archive.len()).any(|i| {
        archive.by_index(i).ok().map(|f| f.name().starts_with("BepInExPack/")).unwrap_or(false)
    });

    if is_bepinex_pack {
        // Install BepInExPack to profile root
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();
            
            if name.starts_with("BepInExPack/") {
                // Strip "BepInExPack/" prefix
                let relative_path = &name["BepInExPack/".len()..];
                if relative_path.is_empty() { continue; }
                
                let outpath = profile_dir.join(relative_path);
                
                if name.ends_with('/') {
                    fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
                } else {
                    if let Some(p) = outpath.parent() {
                        if !p.exists() {
                            fs::create_dir_all(p).map_err(|e| e.to_string())?;
                        }
                    }
                    let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
                    std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                }
            }
        }
    } else {
        // Normal mod installation to plugins/{mod_name}
        // Create directories
        fs::create_dir_all(&mod_dir).map_err(|e| e.to_string())?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let outpath = match file.enclosed_name() {
                Some(path) => mod_dir.join(path),
                None => continue,
            };

            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).map_err(|e| e.to_string())?;
                    }
                }
                let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(serde_json::json!({ "success": true }))
}



#[command]
async fn open_mod_folder(app: AppHandle, profile_id: String, mod_name: String) -> Result<(), String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");
    
    if plugins_dir.exists() {
        for entry in walkdir::WalkDir::new(&plugins_dir)
            .min_depth(1)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok()) 
        {
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.to_lowercase().contains(&mod_name.to_lowercase()) {
                        let _ = open::that(entry.path());
                        return Ok(());
                    }
                }
            }
        }
        let _ = open::that(&plugins_dir);
    } else {
        let _ = open::that(&profile_dir);
    }
    Ok(())
}

#[command]
async fn fetch_packages(app: AppHandle, state: tauri::State<'_, AppState>, game_id: String) -> Result<usize, String> {
    use std::time::SystemTime;

    let start_time = SystemTime::now();
    
    // 0. Check if we already have packages in memory (instant return)
    {
        let packages_lock = state.packages.lock().unwrap();
        if let Some(packages) = packages_lock.get(&game_id) {
            if !packages.is_empty() {
                eprintln!("[fetch_packages] Serving {} packages from memory (instant)", packages.len());
                return Ok(packages.len());
            }
        }
    }
    
    // 1. Fetch the index (list of chunk URLs)
    let index_url = format!("https://thunderstore.io/c/{}/api/v1/package-listing-index/", game_id);
    eprintln!("[fetch_packages] Fetching index from: {}", index_url);
    
    let client = reqwest::Client::builder()
        .gzip(true)
        .build()
        .map_err(|e| e.to_string())?;
        
    let resp = client.get(&index_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch index: {}", e))?;
        
    // The index is a GZIP compressed JSON array of strings (URLs)
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let mut gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut s = String::new();
    std::io::Read::read_to_string(&mut gz, &mut s).map_err(|e| format!("Failed to decompress index: {}", e))?;
    
    let chunk_urls: Vec<String> = serde_json::from_str(&s).map_err(|e| format!("Failed to parse index: {}", e))?;
    let total_chunks = chunk_urls.len();
    eprintln!("[fetch_packages] Found {} chunks", total_chunks);

    // 2. Prepare Cache Directory
    let cache_dir = app.path().app_cache_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("chunks");
    if !cache_dir.exists() {
        let _ = fs::create_dir_all(&cache_dir);
    }

    // Helper function to load a single chunk
    async fn load_chunk(client: &reqwest::Client, url: &str, cache_dir: &std::path::Path) -> Result<Vec<serde_json::Value>, String> {
        // Extract hash from URL for cache key
        let hash = url.split("/sha256/").nth(1)
            .and_then(|s| s.split('.').next())
            .ok_or_else(|| "Invalid URL format".to_string())?;
            
        let cache_file = cache_dir.join(format!("{}.json", hash));
        
        // Check cache
        if cache_file.exists() {
            if let Ok(file) = fs::File::open(&cache_file) {
                let reader = std::io::BufReader::new(file);
                if let Ok(mut packages) = serde_json::from_reader::<_, Vec<serde_json::Value>>(reader) {
                    // Filter out Manager packages from cache too
                    packages.retain(|pkg| {
                        let full_name = pkg["full_name"].as_str().unwrap_or("");
                        !full_name.contains("ebkr-r2modman") && !full_name.contains("Tslat-ThunderstoreModManager")
                    });
                    return Ok(packages);
                }
            }
        }
        
        // Download and Decompress
        let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        let mut gz = flate2::read::GzDecoder::new(&bytes[..]);
        let mut json_str = String::new();
        std::io::Read::read_to_string(&mut gz, &mut json_str).map_err(|e| e.to_string())?;
        
        // Parse
        let mut packages: Vec<serde_json::Value> = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;
        
        // Filter out Manager packages (e.g. r2modman, Thunderstore Mod Manager) if they appear
        packages.retain(|pkg| {
            let full_name = pkg["full_name"].as_str().unwrap_or("");
            !full_name.contains("ebkr-r2modman") && !full_name.contains("Tslat-ThunderstoreModManager")
        });
        
        // Save to cache
        if let Ok(file) = fs::File::create(&cache_file) {
            let _ = serde_json::to_writer(file, &packages);
        }
        
        Ok(packages)
    }

    // 3. Load FIRST chunk immediately for instant UI
    if let Some(first_url) = chunk_urls.first() {
        match load_chunk(&client, first_url, &cache_dir).await {
            Ok(first_packages) => {
                let count = first_packages.len();
                eprintln!("[fetch_packages] First chunk loaded: {} packages (instant display ready!)", count);
                
                // Update state immediately so UI can show something
                let mut packages_lock = state.packages.lock().unwrap();
                packages_lock.insert(game_id.clone(), first_packages);
            }
            Err(e) => {
                eprintln!("[fetch_packages] Failed to load first chunk: {}", e);
            }
        }
    }

    // 4. Load remaining chunks in parallel (streaming to state)
    let remaining_urls: Vec<String> = chunk_urls.into_iter().skip(1).collect();
    
    if !remaining_urls.is_empty() {
        let packages_arc = state.packages.clone();  // Clone the Arc
        let game_id_clone = game_id.clone();
        let cache_dir_clone = cache_dir.clone();
        
        // Spawn background task for remaining chunks
        tokio::spawn(async move {
            let mut tasks = Vec::new();
            
            for url in remaining_urls {
                let cache_dir = cache_dir_clone.clone();
                let client = client.clone();
                
                tasks.push(tokio::spawn(async move {
                    load_chunk(&client, &url, &cache_dir).await
                }));
            }

            // Collect and add to state as they complete
            for task in futures_util::future::join_all(tasks).await {
                match task {
                    Ok(Ok(packages)) => {
                        let mut packages_lock = packages_arc.lock().unwrap();
                        if let Some(existing) = packages_lock.get_mut(&game_id_clone) {
                            existing.extend(packages);
                        }
                    }
                    Ok(Err(e)) => eprintln!("[fetch_packages] Chunk error: {}", e),
                    Err(e) => eprintln!("[fetch_packages] Task error: {}", e),
                }
            }
            
            // Log final count
            let packages_lock = packages_arc.lock().unwrap();
            if let Some(packages) = packages_lock.get(&game_id_clone) {
                eprintln!("[fetch_packages] Background loading complete. Total: {} packages", packages.len());
            }
        });
    }

    // 5. Return immediately with first chunk count
    let packages_lock = state.packages.lock().unwrap();
    let count = packages_lock.get(&game_id).map(|p| p.len()).unwrap_or(0);
    
    if let Ok(elapsed) = start_time.elapsed() {
        eprintln!("[fetch_packages] Initial load in {:.2?} ({} packages ready, {} chunks loading in background)", 
            elapsed, count, total_chunks - 1);
    }

    Ok(count)
}

#[command]
async fn get_packages(
    state: tauri::State<'_, AppState>, 
    game_id: String, 
    page: usize, 
    page_size: usize, 
    search: String,
    sort: Option<String>
) -> Result<Vec<serde_json::Value>, String> {
    let packages_lock = state.packages.lock().map_err(|_| "Failed to lock state".to_string())?;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        let mut filtered: Vec<&serde_json::Value> = if search.is_empty() {
            packages.iter().collect()
        } else {
            let search_lower = search.to_lowercase();
            packages.iter().filter(|p| {
                // Search by name or description
                let name = p["name"].as_str().unwrap_or("").to_lowercase();
                let full_name = p["full_name"].as_str().unwrap_or("").to_lowercase();
                // let description = p["versions"][0]["description"].as_str().unwrap_or("").to_lowercase(); // Optional: search description too
                
                name.contains(&search_lower) || full_name.contains(&search_lower)
            }).collect()
        };

        // Sorting
        if let Some(sort_by) = sort {
            match sort_by.as_str() {
                "downloads" => filtered.sort_by(|a, b| {
                    let get_downloads = |p: &serde_json::Value| -> u64 {
                        p.get("versions")
                         .and_then(|v| v.as_array())
                         .and_then(|arr| arr.first()) 
                         .and_then(|ver| ver.get("downloads"))
                         .and_then(|d| d.as_u64())
                         .unwrap_or(0)
                    };
                    let da = get_downloads(a);
                    let db = get_downloads(b);
                    db.cmp(&da) // Descending
                }),
                "rating" => filtered.sort_by(|a, b| {
                    let ra = a.get("rating_score").and_then(|v| v.as_i64()).unwrap_or(0);
                    let rb = b.get("rating_score").and_then(|v| v.as_i64()).unwrap_or(0);
                    rb.cmp(&ra) // Descending
                }),
                "updated" => filtered.sort_by(|a, b| {
                    let da = a.get("date_updated").and_then(|v| v.as_str()).unwrap_or("");
                    let db = b.get("date_updated").and_then(|v| v.as_str()).unwrap_or("");
                    db.cmp(da) // Descending (newest first)
                }),
                "alphabetical" => filtered.sort_by(|a, b| {
                    let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    na.cmp(&nb) // Ascending
                }),
                _ => {}
            }
        }

        let start = page * page_size;
        if start >= filtered.len() {
            return Ok(vec![]);
        }
        
        let end = std::cmp::min(start + page_size, filtered.len());
        let slice = filtered[start..end].iter().map(|&v| v.clone()).collect();
        Ok(slice)
    } else {
        Ok(vec![])
    }
}

#[command]
async fn lookup_packages_by_names(
    state: tauri::State<'_, AppState>,
    game_id: String,
    names: Vec<String>
) -> Result<serde_json::Value, String> {
    let packages_lock = state.packages.lock().map_err(|_| "Failed to lock state".to_string())?;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        let mut found = Vec::new();
        let mut unknown = Vec::new();
        
        let re = regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").unwrap();
        
        for name in names {
            // Strip version if present: "Author-Mod-1.0.0" -> "Author-Mod"
            let clean_name = if let Some(caps) = re.captures(&name) {
                caps.get(1).map_or(name.clone(), |m| m.as_str().to_string())
            } else {
                name.clone()
            };
            
            if let Some(pkg) = packages.iter().find(|p| {
                p["full_name"].as_str().unwrap_or("") == clean_name
            }) {
                found.push(pkg.clone());
            } else {
                unknown.push(name.clone());
            }
        }
        
        Ok(serde_json::json!({
            "found": found,
            "unknown": unknown
        }))
    } else {
        Err("Game packages not loaded".to_string())
    }
}

#[command]
async fn fetch_package_by_name(state: tauri::State<'_, AppState>, name: String, game_id: Option<String>) -> Result<Option<serde_json::Value>, String> {
    // name might be "Namespace-Name" or "Namespace-Name-Version"
    
    // 1. Strip version if present (Regex: ^(.*)-(\d+\.\d+\.\d+)$)
    let re = regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").unwrap();
    let clean_name = if let Some(caps) = re.captures(&name) {
        caps.get(1).map_or(name.clone(), |m| m.as_str().to_string())
    } else {
        name.clone()
    };

    // 2. Check Cache if game_id is provided
    if let Some(gid) = game_id {
        if let Ok(packages_lock) = state.packages.lock() {
            if let Some(packages) = packages_lock.get(&gid) {
                // Find package in cache
                // Cache structure: Array of package objects
                // We need to match full_name or name
                // The cache stores objects with "full_name": "Namespace-Name"
                
                let target_name = clean_name.to_lowercase();
                
                if let Some(pkg) = packages.iter().find(|p| {
                     p["full_name"].as_str().unwrap_or("").to_lowercase() == target_name
                }) {
                    eprintln!("[fetch_package_by_name] Found {} in cache for game {}", clean_name, gid);
                    return Ok(Some(pkg.clone()));
                }
            }
        }
    }

    eprintln!("[fetch_package_by_name] Cache miss for {}. Fetching from API...", clean_name);

    // 3. Split Namespace and Name
    let parts: Vec<&str> = clean_name.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Ok(None);
    }
    let namespace = parts[0];
    let package_name = parts[1];

    let url = format!("https://thunderstore.io/api/v1/package/{}/{}/", namespace, package_name);
    let client = reqwest::Client::builder()
        .user_agent("r2modmac/0.0.1")
        .build()
        .map_err(|e| e.to_string())?;
    
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;

    if response.status() == 404 {
        return Ok(None);
    }
    
    if !response.status().is_success() {
        return Err(format!("Failed to fetch package: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(Some(json))
}

use base64::Engine;

fn clean_mod_name(name: &str, version: &str) -> String {
    // Strategy 1: Regex match for "-X.Y.Z" at the end
    let re = regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").unwrap();
    if let Some(caps) = re.captures(name) {
        return caps.get(1).map_or(name.to_string(), |m| m.as_str().to_string());
    }
    
    // Strategy 2: Suffix check
    if name.ends_with(version) {
        let without_version = &name[0..name.len() - version.len()];
        if without_version.ends_with('-') {
            return without_version[0..without_version.len() - 1].to_string();
        }
        return without_version.to_string();
    }
    
    name.to_string()
}

fn process_zip_archive(mut archive: zip::ZipArchive<std::io::Cursor<Vec<u8>>>) -> Result<serde_json::Value, String> {
    eprintln!("[process_zip_archive] Processing zip with {} files", archive.len());
    
    let mut content = String::new();
    let is_yaml;
    
    // Check if export.r2x exists first to avoid double borrow
    let has_r2x = archive.by_name("export.r2x").is_ok();
    eprintln!("[process_zip_archive] Has export.r2x: {}", has_r2x);
    
    if has_r2x {
        let mut file = archive.by_name("export.r2x").map_err(|e| e.to_string())?;
        use std::io::Read;
        file.read_to_string(&mut content).map_err(|e| e.to_string())?;
        is_yaml = true;
        eprintln!("[process_zip_archive] Read export.r2x ({} bytes)", content.len());
    } else {
        let mut file = archive.by_name("manifest.json").map_err(|_| "Invalid profile: missing export.r2x or manifest.json".to_string())?;
        use std::io::Read;
        file.read_to_string(&mut content).map_err(|e| e.to_string())?;
        is_yaml = false;
        eprintln!("[process_zip_archive] Read manifest.json ({} bytes)", content.len());
    }
    
    eprintln!("[process_zip_archive] Content preview: {}", &content[0..content.len().min(200)]);
    
    let parsed: serde_json::Value = if is_yaml {
        eprintln!("[process_zip_archive] Parsing as YAML");
        serde_yaml::from_str(&content).map_err(|e| {
            eprintln!("[process_zip_archive] YAML parse error: {}", e);
            e.to_string()
        })?
    } else {
        eprintln!("[process_zip_archive] Parsing as JSON");
        serde_json::from_str(&content).map_err(|e| {
            eprintln!("[process_zip_archive] JSON parse error: {}", e);
            e.to_string()
        })?
    };

    eprintln!("[process_zip_archive] Parsed data: {:?}", parsed);

    // Map to expected format
    let profile_name = parsed["profileName"].as_str().unwrap_or("Imported Profile");
    eprintln!("[process_zip_archive] Profile name: {}", profile_name);
    
    let empty_vec = vec![];
    let mods_array = parsed["mods"].as_array().unwrap_or(&empty_vec);
    eprintln!("[process_zip_archive] Found {} mods", mods_array.len());
    
    let mods = mods_array.iter().enumerate().map(|(idx, m)| {
        let name = m["name"].as_str().unwrap_or("");
        
        // Handle version: could be string "1.0.0" or object {major, minor, patch}
        let version_str = if let Some(v_obj) = m["version"].as_object() {
             format!("{}.{}.{}", 
                v_obj.get("major").and_then(|v| v.as_u64()).unwrap_or(0),
                v_obj.get("minor").and_then(|v| v.as_u64()).unwrap_or(0),
                v_obj.get("patch").and_then(|v| v.as_u64()).unwrap_or(0)
            )
        } else {
            m["version"].as_str().unwrap_or("0.0.0").to_string()
        };
        
        let clean_name = clean_mod_name(name, &version_str);
        let enabled = m["enabled"].as_bool().unwrap_or(true);
        
        eprintln!("[process_zip_archive] Mod {}: {} -> {} (v{}), enabled: {}", 
            idx, name, clean_name, version_str, enabled);
        
        serde_json::json!({
            "name": clean_name,
            "version": version_str,
            "enabled": enabled
        })
    }).collect::<Vec<_>>();

    let result = serde_json::json!({
        "type": "profile",
        "name": profile_name,
        "mods": mods
    });
    
    eprintln!("[process_zip_archive] Final result: {:?}", result);
    Ok(result)
}

#[command]
async fn delete_profile_folder(app: AppHandle, profile_id: String, game_identifier: Option<String>) -> Result<bool, String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
    
    // If game_identifier is provided, clean up ALL BepInEx-related files from the game folder
    if let Some(game_id) = game_identifier {
        if let Ok(Some(game_path_str)) = get_game_path(app.clone(), game_id).await {
            let game_path = std::path::Path::new(&game_path_str);
            
            // Remove BepInEx folder
            let bepinex_path = game_path.join("BepInEx");
            if bepinex_path.exists() {
                eprintln!("[delete_profile] Removing BepInEx folder from game");
                let _ = fs::remove_dir_all(&bepinex_path);
            }
            
            // Remove winhttp.dll
            let winhttp_path = game_path.join("winhttp.dll");
            if winhttp_path.exists() {
                eprintln!("[delete_profile] Removing winhttp.dll from game");
                let _ = fs::remove_file(&winhttp_path);
            }
            
            // Remove doorstop_config.ini
            let doorstop_path = game_path.join("doorstop_config.ini");
            if doorstop_path.exists() {
                eprintln!("[delete_profile] Removing doorstop_config.ini from game");
                let _ = fs::remove_file(&doorstop_path);
            }
            
            eprintln!("[delete_profile] Cleaned up game folder: {}", game_path.display());
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
async fn toggle_mod(app: AppHandle, profile_id: String, mod_name: String, enabled: bool, game_identifier: Option<String>) -> Result<(), String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");
    
    // Get game path for live sync
    let game_plugins = if let Some(ref game_id) = game_identifier {
        if let Ok(Some(game_path_str)) = get_game_path(app.clone(), game_id.clone()).await {
            Some(std::path::Path::new(&game_path_str).join("BepInEx").join("plugins"))
        } else {
            None
        }
    } else {
        None
    };
    
    // Find the mod folder
    if let Ok(entries) = fs::read_dir(&plugins_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let folder_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            // Match mod folder by name (case insensitive, partial match)
            if folder_name.to_lowercase().contains(&mod_name.to_lowercase()) && path.is_dir() {
                eprintln!("[toggle_mod] Found mod folder: {:?}, enabled: {}", path, enabled);
                
                // Sync to game folder if available
                if let Some(ref game_plugins_path) = game_plugins {
                    let game_mod_path = game_plugins_path.join(folder_name);
                    
                    if enabled {
                        // Copy mod to game folder
                        eprintln!("[toggle_mod] Syncing enabled mod to game: {}", folder_name);
                        if game_mod_path.exists() {
                            let _ = fs::remove_dir_all(&game_mod_path);
                        }
                        copy_dir_recursive(&path, &game_mod_path)
                            .map_err(|e| format!("Failed to sync mod to game: {}", e))?;
                    } else {
                        // Remove mod from game folder
                        if game_mod_path.exists() {
                            eprintln!("[toggle_mod] Removing disabled mod from game: {}", folder_name);
                            fs::remove_dir_all(&game_mod_path)
                                .map_err(|e| format!("Failed to remove mod from game: {}", e))?;
                        }
                    }
                }
                
                return Ok(());
            }
        }
    }
    
    Err(format!("Mod '{}' not found in profile", mod_name))
}

#[command]
fn check_directory_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[command]
async fn import_profile_from_file(_app: AppHandle, path: String) -> Result<serde_json::Value, String> {
    eprintln!("[import_profile_from_file] Starting import from file: {}", path);
    let bytes = fs::read(&path).map_err(|e| {
        eprintln!("[import_profile_from_file] Failed to read file: {}", e);
        e.to_string()
    })?;
    eprintln!("[import_profile_from_file] Read {} bytes", bytes.len());
    
    let cursor = std::io::Cursor::new(bytes);
    let archive = zip::ZipArchive::new(cursor).map_err(|e| {
        eprintln!("[import_profile_from_file] Failed to create zip archive: {}", e);
        e.to_string()
    })?;
    
    eprintln!("[import_profile_from_file] Zip archive created, processing...");
    let result = process_zip_archive(archive)?;
    eprintln!("[import_profile_from_file] Result: {:?}", result);
    Ok(result)
}

#[command]
async fn import_profile(_app: AppHandle, code: String) -> Result<serde_json::Value, String> {
    eprintln!("[import_profile] Starting import with code: {}", code);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;
    
    // Strategy 1: Profile Code
    let profile_url = format!("https://thunderstore.io/api/experimental/legacyprofile/get/{}/", code);
    eprintln!("[import_profile] Strategy 1: Trying profile code URL: {}", profile_url);
    
    let response = client.get(&profile_url).send().await;
    
    match response {
        Ok(res) => {
            eprintln!("[import_profile] Strategy 1: Got response with status: {}", res.status());
            if res.status().is_success() {
                let content = res.text().await.unwrap_or_default();
                eprintln!("[import_profile] Strategy 1: Content length: {}, starts with #r2modman: {}", content.len(), content.starts_with("#r2modman"));
                
                if content.starts_with("#r2modman") {
                    eprintln!("[import_profile] Strategy 1: Detected r2modman profile, decoding base64...");
                    let base64_data = content.trim_start_matches("#r2modman").trim();
                    let zip_data = base64::engine::general_purpose::STANDARD.decode(base64_data).map_err(|e| {
                        eprintln!("[import_profile] Strategy 1: Base64 decode failed: {}", e);
                        e.to_string()
                    })?;
                    eprintln!("[import_profile] Strategy 1: Decoded {} bytes, creating zip archive...", zip_data.len());
                    let cursor = std::io::Cursor::new(zip_data);
                    let archive = zip::ZipArchive::new(cursor).map_err(|e| {
                        eprintln!("[import_profile] Strategy 1: Zip archive creation failed: {}", e);
                        e.to_string()
                    })?;
                    eprintln!("[import_profile] Strategy 1: Processing zip archive...");
                    return process_zip_archive(archive);
                }
            }
        }
        Err(e) => {
            eprintln!("[import_profile] Strategy 1: Request failed: {}", e);
        }
    }
    
    // Strategy 2: Package UUID
    eprintln!("[import_profile] Strategy 2: Trying package UUID lookup");
    let resolve_url = format!("https://thunderstore.io/api/experimental/namespace-by-id/{}/", code);
    eprintln!("[import_profile] Strategy 2: URL: {}", resolve_url);
    
    let response = client.get(&resolve_url).send().await.map_err(|e| {
        eprintln!("[import_profile] Strategy 2: Request failed: {}", e);
        e.to_string()
    })?;
    
    eprintln!("[import_profile] Strategy 2: Got response with status: {}", response.status());
    
    if !response.status().is_success() {
        return Err(format!("Import failed: Code not found as Profile or Package UUID (status: {})", response.status()));
    }
    
    let metadata: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    eprintln!("[import_profile] Strategy 2: Got metadata: {:?}", metadata);
    
    let namespace = metadata["namespace"].as_str().ok_or("Invalid metadata: missing namespace")?;
    let name = metadata["name"].as_str().ok_or("Invalid metadata: missing name")?;
    eprintln!("[import_profile] Strategy 2: Namespace: {}, Name: {}", namespace, name);
    
    let package_url = format!("https://thunderstore.io/api/v1/package/{}/{}/", namespace, name);
    eprintln!("[import_profile] Strategy 2: Fetching package from: {}", package_url);
    
    let pkg_response = client.get(&package_url).send().await.map_err(|e| e.to_string())?;
    
    if !pkg_response.status().is_success() {
        eprintln!("[import_profile] Strategy 2: Package fetch failed with status: {}", pkg_response.status());
        return Err(format!("Package details not found (status: {})", pkg_response.status()));
    }
    
    let pkg: serde_json::Value = pkg_response.json().await.map_err(|e| e.to_string())?;
    eprintln!("[import_profile] Strategy 2: Successfully got package");
    
    Ok(serde_json::json!({
        "type": "package",
        "package": pkg
    }))
}

#[command]
async fn remove_mod(app: AppHandle, profile_id: String, mod_name: String) -> Result<bool, String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");
    
    // mod_name is usually "Namespace-Name-Version" or "Namespace-Name"
    // We need to find the folder.
    // Logic similar to open_mod_folder
    
    if plugins_dir.exists() {
        for entry in walkdir::WalkDir::new(&plugins_dir)
            .min_depth(1)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok()) 
        {
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Simple check: if folder name contains mod_name (case insensitive)
                    // Better: check if folder name STARTS with mod_name (Namespace-Name)
                    // But mod_name passed from frontend is usually "Namespace-Name-Version"
                    // We should probably pass the "clean name" or handle it.
                    
                    // Let's try to match loosely for now, or require exact match if possible.
                    // Frontend passes `mod.uuid4` to store, but `removeMod` in store has `modId`.
                    // Wait, store `removeMod` takes `modId` (uuid4).
                    // But to delete file, I need the name.
                    // The store has the profile, so it knows the name.
                    
                    // I will update the store to pass the name.
                    
                    if name.to_lowercase().contains(&mod_name.to_lowercase()) {
                        fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

#[command]
async fn export_profile(app: AppHandle, profile_id: String) -> Result<serde_json::Value, String> {
    // 1. Read profiles.json
    let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profiles_path.exists() {
        return Err("No profiles found".to_string());
    }
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> = serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;
    
    let profile = profiles.iter().find(|p| p["id"] == profile_id).ok_or("Profile not found")?;
    
    // 2. Create export data
    let mods = profile["mods"].as_array().unwrap_or(&vec![]).iter().map(|m| {
        let full_name = m["fullName"].as_str().unwrap_or("");
        let version_number = m["versionNumber"].as_str().unwrap_or("0.0.0");
        let enabled = m["enabled"].as_bool().unwrap_or(true);
        
        // Clean name logic (strip version suffix)
        let clean_name = if full_name.ends_with(&format!("-{}", version_number)) {
            &full_name[0..full_name.len() - version_number.len() - 1]
        } else {
            full_name
        };
        
        let version_parts: Vec<&str> = version_number.split('.').collect();
        let major = version_parts.get(0).unwrap_or(&"0").parse().unwrap_or(0);
        let minor = version_parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
        let patch = version_parts.get(2).unwrap_or(&"0").parse().unwrap_or(0);
        
        serde_json::json!({
            "name": clean_name,
            "version": {
                "major": major,
                "minor": minor,
                "patch": patch
            },
            "enabled": enabled
        })
    }).collect::<Vec<_>>();
    
    let export_data = serde_json::json!({
        "profileName": profile["name"],
        "mods": mods
    });
    
    // 3. Convert to YAML
    let yaml_content = serde_yaml::to_string(&export_data).map_err(|e| e.to_string())?;
    
    // 4. Create Zip
    let temp_dir = std::env::temp_dir().join(format!("r2modmac-export-{}", profile_id));
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let zip_path = temp_dir.join("profile.r2z");
    let file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    
    let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("export.r2x", options).map_err(|e| e.to_string())?;
    use std::io::Write;
    zip.write_all(yaml_content.as_bytes()).map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    
    // 5. Save Dialog
    use tauri_plugin_dialog::DialogExt;
    let save_path = app.dialog().file()
        .add_filter("r2modman Profile", &["r2z"])
        .set_file_name(&format!("{}.r2z", profile["name"].as_str().unwrap_or("profile")))
        .blocking_save_file();
        
    if let Some(path) = save_path {
        let path_str = path.to_string();
        fs::copy(&zip_path, &path_str).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "success": true, "path": path_str }))
    } else {
        Ok(serde_json::json!({ "success": false, "error": "Cancelled" }))
    }
}

#[command]
async fn share_profile(app: AppHandle, profile_id: String) -> Result<String, String> {
    // 1. Read profiles.json
    let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profiles_path.exists() {
        return Err("No profiles found".to_string());
    }
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> = serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;
    
    let profile = profiles.iter().find(|p| p["id"] == profile_id).ok_or("Profile not found")?;
    
    // 2. Create export data (Same logic as export_profile)
    let mods = profile["mods"].as_array().unwrap_or(&vec![]).iter().map(|m| {
        let full_name = m["fullName"].as_str().unwrap_or("");
        let version_number = m["versionNumber"].as_str().unwrap_or("0.0.0");
        let enabled = m["enabled"].as_bool().unwrap_or(true);
        
        // Clean name logic (strip version suffix)
        let clean_name = if full_name.ends_with(&format!("-{}", version_number)) {
            &full_name[0..full_name.len() - version_number.len() - 1]
        } else {
            full_name
        };
        
        let version_parts: Vec<&str> = version_number.split('.').collect();
        let major = version_parts.get(0).unwrap_or(&"0").parse().unwrap_or(0);
        let minor = version_parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
        let patch = version_parts.get(2).unwrap_or(&"0").parse().unwrap_or(0);
        
        serde_json::json!({
            "name": clean_name,
            "version": {
                "major": major,
                "minor": minor,
                "patch": patch
            },
            "enabled": enabled
        })
    }).collect::<Vec<_>>();
    
    let export_data = serde_json::json!({
        "profileName": profile["name"],
        "mods": mods
    });
    
    // 3. Convert to YAML
    let yaml_content = serde_yaml::to_string(&export_data).map_err(|e| e.to_string())?;
    
    // 4. Create Zip in Memory
    let mut zip_buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut zip_buffer);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        
        zip.start_file("export.r2x", options).map_err(|e| e.to_string())?;
        use std::io::Write;
        zip.write_all(yaml_content.as_bytes()).map_err(|e| e.to_string())?;
        
        zip.finish().map_err(|e| e.to_string())?;
    }
    
    // 5. Base64 Encode and Prepend Header
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&zip_buffer);
    let payload = format!("#r2modman\n{}", base64_data);
    
    // 6. Upload to Thunderstore
    let client = reqwest::Client::new();
    let response = client.post("https://thunderstore.io/api/experimental/legacyprofile/create/")
        .header("Content-Type", "application/octet-stream")
        .body(payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !response.status().is_success() {
        return Err(format!("Upload failed: {}", response.status()));
    }
    
    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    
    // 7. Return Key
    let key = json["key"].as_str().ok_or("Invalid response: missing key")?;
    Ok(key.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct GithubAsset {
    browser_download_url: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateInfo {
    available: bool,
    version: String,
    notes: String,
    download_url: Option<String>,
}

#[command]
async fn check_update(current_version: String) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::new();
    // Use public GitHub API
    let resp = client.get("https://api.github.com/repos/Zard-Studios/r2modmac/releases/latest")
        .header("User-Agent", "r2modmac-updater")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API error: {}", resp.status()));
    }

    let release: GithubRelease = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
    
    // Simple version comparison (naive string compare for now, ideally use semver)
    // Assume tag_name is "vX.X.X" and current_version is "X.X.X"
    let clean_tag = release.tag_name.trim_start_matches('v');
    
    // Use semver crate if available, or simple split compare
    let is_newer = compare_versions(clean_tag, &current_version);
    
    // Detect system architecture
    let arch = std::env::consts::ARCH; // "aarch64" for Apple Silicon, "x86_64" for Intel
    eprintln!("[check_update] Detected architecture: {}", arch);
    
    // Map architecture to expected asset name patterns
    let arch_pattern = match arch {
        "aarch64" => "aarch64",
        "x86_64" => "x64",
        _ => "universal", // Fallback to universal for unknown archs
    };
    
    // Find matching DMG/archive: prioritize exact arch match, fallback to universal
    let asset = release.assets.iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            (name.ends_with(".dmg") || name.ends_with(".tar.gz") || name.ends_with(".zip"))
                && name.contains(arch_pattern)
        })
        .or_else(|| {
            // Fallback to universal if exact arch not found
            release.assets.iter().find(|a| {
                let name = a.name.to_lowercase();
                (name.ends_with(".dmg") || name.ends_with(".tar.gz") || name.ends_with(".zip"))
                    && name.contains("universal")
            })
        });
    
    eprintln!("[check_update] Selected asset: {:?}", asset.map(|a| &a.name));

    Ok(UpdateInfo {
        available: is_newer,
        version: release.tag_name,
        notes: release.body,
        download_url: asset.map(|a| a.browser_download_url.clone()),
    })
}

fn compare_versions(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
         .map(|s| s.parse().unwrap_or(0))
         .collect()
    };
    let remote_parts = parse(remote);
    let local_parts = parse(local);
    
    for i in 0..std::cmp::max(remote_parts.len(), local_parts.len()) {
        let r = *remote_parts.get(i).unwrap_or(&0);
        let l = *local_parts.get(i).unwrap_or(&0);
        if r > l { return true; }
        if r < l { return false; }
    }
    false
}

#[command]
async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    // 1. Download
    let temp_dir = app.path().temp_dir().map_err(|e| e.to_string())?.join("r2modmac_update");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let filename = download_url.split('/').last().unwrap_or("update.bin");
    let file_path = temp_dir.join(filename);

    eprintln!("[install_update] Downloading to {:?}", file_path);
    
    let response = reqwest::get(&download_url).await.map_err(|e| e.to_string())?;
    let content = response.bytes().await.map_err(|e| e.to_string())?;
    fs::write(&file_path, &content).map_err(|e| e.to_string())?;

    // 2. Prepare Update Script
    let script_path = temp_dir.join("update.sh");
    
    // GUARD: Check if we are in dev mode or not in a standard .app bundle
    // If current_exe is inside "target/debug" or "target/release", we are likely in dev/build.
    // Abort update to prevent deleting source code!
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_string = current_exe.to_string_lossy();
    if exe_string.contains("/target/debug/") || exe_string.contains("/target/release/") {
        eprintln!("Dev/Build environment detected. Skipping destructive update.");
        return Err("Cannot auto-update in development environment. Build a release bundle to test.".to_string());
    }

    // Determine extraction/mount commands based on file type
    let (extract_command, app_source) = if filename.ends_with(".tar.gz") {
        (
            format!("tar -xzf '{}' -C '{}'", file_path.to_string_lossy(), temp_dir.to_string_lossy()),
            format!("{}/r2modmac.app", temp_dir.to_string_lossy())
        )
    } else if filename.ends_with(".zip") {
        (
            format!("unzip -o '{}' -d '{}'", file_path.to_string_lossy(), temp_dir.to_string_lossy()),
            format!("{}/r2modmac.app", temp_dir.to_string_lossy())
        )
    } else if filename.ends_with(".dmg") {
        // DMG: mount readonly to private folder, copy app, unmount
        // "Extracting with style": hdiutil attach ...
        let mount_point = format!("{}/dmg_mount", temp_dir.to_string_lossy());
        (
            format!(
                "mkdir -p '{}' && hdiutil attach '{}' -mountpoint '{}' -nobrowse -quiet -readonly",
                mount_point, file_path.to_string_lossy(), mount_point
            ),
            format!("{}/r2modmac.app", mount_point)
        )
    } else {
        return Err("Unknown update format".to_string());
    };

    let is_dmg = filename.ends_with(".dmg");
    let mount_point = format!("{}/dmg_mount", temp_dir.to_string_lossy());

    // Navigate up from Contents/MacOS/executable to .app
    // Bundle path usually ends in .app. 
    // Guard ensured we are likely safe, but let's defaulting to /Applications just in case.
    let current_app_path = current_exe
        .parent().and_then(|p| p.parent()).and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or("/Applications/r2modmac.app".to_string());

    // Build script with conditional DMG unmount
    let unmount_command = if is_dmg {
        format!("hdiutil detach '{}' -force -quiet || true", mount_point)
    } else {
        String::new()
    };

    // The script waits for PID, mounts/extracts, deletes OLD app, moves NEW app, launches NEW app, cleans up.
    let script = format!(
r#"#!/bin/bash
PID={}
APP_PATH="{}"
UPDATE_DIR="{}"
APP_SOURCE="{}"

echo "Waiting for PID $PID to exit..."
while kill -0 $PID 2>/dev/null; do sleep 0.5; done

echo "Extracting/Mounting Update..."
{}

if [ ! -d "$APP_SOURCE" ]; then
    echo "Error: New app not found at $APP_SOURCE"
    exit 1
fi

echo "Replacing app at $APP_PATH..."
rm -rf "$APP_PATH"
cp -R "$APP_SOURCE" "$APP_PATH"

# Unmount if needed (DMG)
{}

echo "Launching new app..."
open "$APP_PATH"

echo "Cleaning up..."
rm -rf "$UPDATE_DIR"
"#,
        std::process::id(),
        current_app_path,
        temp_dir.to_string_lossy(),
        app_source,
        extract_command,
        unmount_command
    );

    fs::write(&script_path, script).map_err(|e| e.to_string())?;
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;

    // 3. Launch Script detached
    eprintln!("[install_update] Launching update script...");
    Command::new("sh")
        .arg(&script_path)
        .spawn()
        .map_err(|e| format!("Failed to launch script: {}", e))?;

    Ok(())
}
