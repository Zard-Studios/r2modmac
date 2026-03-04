use std::fs;
use futures_util::StreamExt;
use tauri::{command, AppHandle, Emitter, Manager};
use crate::models::shared::*;
use crate::utils::file_ops::*;
use crate::commands::game_commands::get_game_path;

fn extract_mod_key(input: &str) -> String {
    let parts: Vec<&str> = input.split('-').collect();
    if parts.len() >= 2 {
        format!("{}-{}", parts[0].to_lowercase(), parts[1].to_lowercase())
    } else {
        input.to_lowercase()
    }
}

fn normalize_alnum(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

fn folder_matches_mod(folder_name: &str, mod_query: &str, mod_key: &str) -> bool {
    let folder_lower = folder_name.to_lowercase();
    if folder_lower.contains(mod_query) || mod_query.contains(&folder_lower) {
        return true;
    }

    let folder_norm = normalize_alnum(folder_name);
    let query_norm = normalize_alnum(mod_query);
    if !folder_norm.is_empty()
        && !query_norm.is_empty()
        && (folder_norm.contains(&query_norm) || query_norm.contains(&folder_norm))
    {
        return true;
    }

    let folder_tokens = tokenize(folder_name);
    let query_tokens = tokenize(mod_query);
    let overlap = folder_tokens
        .iter()
        .filter(|token| query_tokens.iter().any(|q| q == *token))
        .count();
    if overlap >= 2 {
        return true;
    }

    extract_mod_key(folder_name) == mod_key
}

fn find_mod_entry_recursive(base: &std::path::Path, mod_name: &str, depth: usize) -> Option<std::path::PathBuf> {
    if depth == 0 || !base.exists() {
        return None;
    }

    let mod_query = mod_name.to_lowercase();
    let mod_key = extract_mod_key(&mod_query);

    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if folder_matches_mod(&name, &mod_query, &mod_key) {
                return Some(path);
            }

            let file_type = entry.file_type().ok();
            let is_dir_like = file_type
                .as_ref()
                .map(|t| t.is_dir() || t.is_symlink())
                .unwrap_or(false);
            if is_dir_like {
                if let Some(found) = find_mod_entry_recursive(&path, mod_name, depth - 1) {
                    return Some(found);
                }
            }
        }
    }

    None
}

fn find_mod_folder_in(base: &std::path::Path, mod_name: &str) -> Option<String> {
    if !base.exists() {
        return None;
    }

    let mod_query = mod_name.to_lowercase();
    let mod_key = extract_mod_key(&mod_query);
    let mut fallback: Option<String> = None;

    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let folder_name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().ok();
            let is_container = file_type
                .as_ref()
                .map(|t| t.is_dir() || t.is_symlink())
                .unwrap_or(false);
            if !is_container {
                continue;
            }

            let folder_key = extract_mod_key(&folder_name);
            if folder_key == mod_key {
                return Some(folder_name);
            }

            if fallback.is_none() && folder_matches_mod(&folder_name, &mod_query, &mod_key) {
                fallback = Some(folder_name);
            }
        }
    }

    fallback
}

#[command]
pub async fn install_mod(app: AppHandle, profile_id: String, download_url: String, mod_name: String, game_path: String, use_profile_cache: Option<bool>) -> Result<serde_json::Value, String> {
    // Install DIRECTLY to game folder
    let game_dir = std::path::Path::new(&game_path);
    let plugins_dir = game_dir.join("BepInEx").join("plugins");
    let mod_dir = plugins_dir.join(&mod_name);

    eprintln!("[install_mod] Installing {} directly to game: {:?}", mod_name, game_dir);

    // Download with live progress events (bytes + speed)
    let client = reqwest::Client::new();
    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Download failed with status {}", status));
    }

    let total_bytes = response.content_length();
    let mut stream = response.bytes_stream();
    let mut bytes: Vec<u8> = Vec::new();
    let mut downloaded: u64 = 0;
    let download_started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Some(next_chunk) = stream.next().await {
        let chunk = next_chunk.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);

        let now = std::time::Instant::now();
        if now.duration_since(last_emit).as_millis() >= 120 {
            let elapsed = download_started.elapsed().as_secs_f64().max(0.001);
            let speed_bps = downloaded as f64 / elapsed;
            let progress_percent = total_bytes
                .map(|total| {
                    ((downloaded as f64 / total as f64) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8
                })
                .unwrap_or(0);

            let _ = app.emit(
                "mod-download-progress",
                serde_json::json!({
                    "mod_name": mod_name.as_str(),
                    "downloaded_bytes": downloaded,
                    "total_bytes": total_bytes,
                    "speed_bps": speed_bps,
                    "progress_percent": progress_percent,
                    "done": false
                }),
            );
            last_emit = now;
        }
    }

    let elapsed = download_started.elapsed().as_secs_f64().max(0.001);
    let final_speed_bps = downloaded as f64 / elapsed;
    let _ = app.emit(
        "mod-download-progress",
        serde_json::json!({
            "mod_name": mod_name.as_str(),
            "downloaded_bytes": downloaded,
            "total_bytes": total_bytes,
            "speed_bps": final_speed_bps,
            "progress_percent": 100,
            "done": true
        }),
    );
    
    // Smart detection: Check if this is BepInEx framework (not just "BepInExPack/" prefix)
    let cursor = std::io::Cursor::new(&bytes);
    let mut archive_for_detect = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    let (is_bepinex_pack, bepinex_prefix) = detect_bepinex_structure(&mut archive_for_detect);

    // Install to game folder
    let cursor = std::io::Cursor::new(&bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    if is_bepinex_pack {
        // Install BepInExPack to GAME root (not profile!)
        let prefix = bepinex_prefix.clone().unwrap_or_default();
        eprintln!("[install_mod] Detected BepInExPack - installing to game root (stripping prefix: '{}')", prefix);
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();
            
            // Strip detected prefix (e.g., "BepInExPack/", "BepInExPack_PEAK/", etc.)
            let relative_path = if !prefix.is_empty() && name.starts_with(&prefix) {
                &name[prefix.len()..]
            } else {
                &name
            };
            
            if relative_path.is_empty() { continue; }
            
            let outpath = game_dir.join(relative_path);
            
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
                eprintln!("[install_mod] Extracted: {}", relative_path);
            }
        }
    } else {
        // Normal mod installation to game/BepInEx/plugins/{mod_name}
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

    // LEGACY MODE: Also save to profile cache folder
    if use_profile_cache.unwrap_or(false) {
        let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
            .join("profiles").join(&profile_id);
        let profile_plugins_dir = profile_dir.join("BepInEx").join("plugins");
        let profile_mod_dir = profile_plugins_dir.join(&mod_name);

        eprintln!("[install_mod] LEGACY: Also caching to profile: {:?}", profile_dir);

        let cursor = std::io::Cursor::new(&bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

        if is_bepinex_pack {
            // Cache BepInExPack to profile root using same dynamic prefix
            let prefix = bepinex_prefix.clone().unwrap_or_default();
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                let name = file.name().to_string();
                
                // Strip detected prefix
                let relative_path = if !prefix.is_empty() && name.starts_with(&prefix) {
                    &name[prefix.len()..]
                } else {
                    &name
                };
                
                if relative_path.is_empty() { continue; }
                
                let outpath = profile_dir.join(relative_path);
                
                if name.ends_with('/') {
                    let _ = fs::create_dir_all(&outpath);
                } else {
                    if let Some(p) = outpath.parent() {
                        let _ = fs::create_dir_all(p);
                    }
                    if let Ok(mut outfile) = fs::File::create(&outpath) {
                        let _ = std::io::copy(&mut file, &mut outfile);
                    }
                }
            }
        } else {
            // Cache normal mod to profile/BepInEx/plugins/{mod_name}
            eprintln!("[install_mod] Creating profile cache dir: {:?}", profile_mod_dir);
            if let Err(e) = fs::create_dir_all(&profile_mod_dir) {
                eprintln!("[install_mod] ERROR creating profile cache dir: {}", e);
            }

            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                let outpath = match file.enclosed_name() {
                    Some(path) => profile_mod_dir.join(path),
                    None => continue,
                };

                if (*file.name()).ends_with('/') {
                    let _ = fs::create_dir_all(&outpath);
                } else {
                    if let Some(p) = outpath.parent() {
                        let _ = fs::create_dir_all(p);
                    }
                    if let Ok(mut outfile) = fs::File::create(&outpath) {
                        let _ = std::io::copy(&mut file, &mut outfile);
                    }
                }
            }
        }
    }

    eprintln!("[install_mod] Successfully installed {} to game folder", mod_name);
    Ok(serde_json::json!({ "success": true }))
}

#[command]
pub async fn remove_mod(app: AppHandle, profile_id: String, mod_name: String) -> Result<bool, String> {
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
pub async fn open_mod_folder(
    app: AppHandle,
    _profile_id: String,
    mod_name: String,
    game_identifier: String,
    platform: Option<String>,
) -> Result<(), String> {
    let game_path = get_game_path(app.clone(), game_identifier, platform)
        .await?
        .ok_or_else(|| "GAME_PATH_NOT_CONFIGURED".to_string())?;
    let game_root = std::path::Path::new(&game_path);
    let mod_key = extract_mod_key(&mod_name);

    // BepInExPack is installed to game root (BepInEx/, doorstop files), not under plugins/.
    if mod_key.contains("bepinexpack") {
        let bepinex_root = game_root.join("BepInEx");
        if bepinex_root.exists() {
            open::that(&bepinex_root).map_err(|e| format!("Failed to open BepInEx folder: {}", e))?;
            return Ok(());
        }

        let has_root_injection_files = [
            game_root.join("run_bepinex.sh"),
            game_root.join("doorstop_libs"),
            game_root.join("doorstop_config.ini"),
            game_root.join("winhttp.dll"),
        ]
        .iter()
        .any(|p| p.exists());

        if has_root_injection_files {
            open::that(game_root).map_err(|e| format!("Failed to open game root folder: {}", e))?;
            return Ok(());
        }

        return Err("MODS_NOT_APPLIED".to_string());
    }

    let bepinex_root = game_root.join("BepInEx");
    if !bepinex_root.exists() {
        return Err("MODS_NOT_APPLIED".to_string());
    }

    // First check common mod locations.
    let common_dirs = [
        bepinex_root.join("plugins"),
        bepinex_root.join("patchers"),
        bepinex_root.join("core"),
    ];

    for dir in common_dirs {
        if let Some(entry_name) = find_mod_folder_in(&dir, &mod_name) {
            let target = dir.join(entry_name);
            open::that(&target).map_err(|e| format!("Failed to open mod folder: {}", e))?;
            return Ok(());
        }
    }

    // Recursive fallback for packages that place files in non-standard nested paths.
    if let Some(found_path) = find_mod_entry_recursive(&bepinex_root, &mod_name, 4) {
        let target = if found_path.is_file() {
            found_path.parent().map(|p| p.to_path_buf()).unwrap_or(found_path)
        } else {
            found_path
        };
        open::that(&target).map_err(|e| format!("Failed to open mod folder: {}", e))?;
        return Ok(());
    }

    // MonoMod-based packs may resolve under patchers without a clear folder match.
    if mod_name.to_lowercase().contains("monomod") {
        let patchers_dir = bepinex_root.join("patchers");
        if patchers_dir.exists() {
            open::that(&patchers_dir).map_err(|e| format!("Failed to open patchers folder: {}", e))?;
            return Ok(());
        }
    }

    Err("MOD_NOT_INSTALLED".to_string())
}

#[command]
pub async fn toggle_mod(app: AppHandle, profile_id: String, mod_name: String, enabled: bool, game_identifier: Option<String>, platform: Option<String>) -> Result<(), String> {
    eprintln!("[toggle_mod] Toggle mod: {} enabled: {} in profile: {}", mod_name, enabled, profile_id);
    
    // Get game path for sync (optional - toggle still works without it)
    let game_plugins = if let Some(ref game_id) = game_identifier {
        if let Ok(Some(game_path_str)) = get_game_path(app.clone(), game_id.clone(), platform.clone()).await {
            Some(std::path::Path::new(&game_path_str).to_path_buf().join("BepInEx").join("plugins"))
        } else {
            None
        }
    } else {
        None
    };
    
    // Get profile cache path (may or may not exist depending on legacy mode)
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
    let profile_plugins_dir = profile_dir.join("BepInEx").join("plugins");
    
    // Find mod in profile cache OR game folder
    // Try profile cache first
    let mut found_folder_name = find_mod_folder_in(&profile_plugins_dir, &mod_name);
    
    // If not found in profile cache, try game folder
    if found_folder_name.is_none() {
        if let Some(ref game_plugins_path) = game_plugins {
            found_folder_name = find_mod_folder_in(game_plugins_path, &mod_name);
        }
    }
    
    // If we have a game folder, sync the mod state
    if let Some(ref game_plugins_path) = game_plugins {
        if let Some(ref folder_name) = found_folder_name {
            let game_mod_path = game_plugins_path.join(folder_name);
            let profile_mod_path = profile_plugins_dir.join(folder_name);
            
            if enabled {
                // Need to add mod to game - copy from profile cache if available
                if profile_mod_path.exists() && !game_mod_path.exists() {
                    eprintln!("[toggle_mod] Enabling mod - copying from cache to game: {}", folder_name);
                    copy_dir_recursive(&profile_mod_path, &game_mod_path)
                        .map_err(|e| format!("Failed to sync mod to game: {}", e))?;
                }
            } else {
                // Remove mod from game folder (keep in cache)
                if game_mod_path.exists() || game_mod_path.is_symlink() {
                    eprintln!("[toggle_mod] Disabling mod - removing from game: {}", folder_name);
                    if game_mod_path.is_symlink() || game_mod_path.is_file() {
                        fs::remove_file(&game_mod_path)
                            .map_err(|e| format!("Failed to remove mod symlink/file from game: {}", e))?;
                    } else {
                        fs::remove_dir_all(&game_mod_path)
                            .map_err(|e| format!("Failed to remove mod directory from game: {}", e))?;
                    }
                }
            }
        }
    }
    
    // Always succeed - the enabled state is tracked in profiles.json, not file system
    eprintln!("[toggle_mod] Toggle complete for mod: {}", mod_name);
    Ok(())
}

#[command]
pub async fn copy_mod_from_cache(app: AppHandle, profile_id: String, mod_name: String, game_path: String) -> Result<serde_json::Value, String> {
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
    let profile_plugins_dir = profile_dir.join("BepInEx").join("plugins");
    
    let game_dir = std::path::Path::new(&game_path);
    let game_plugins_dir = game_dir.join("BepInEx").join("plugins");
    
    // Find the mod folder in profile cache (case insensitive partial match)
    let mod_name_lower = mod_name.to_lowercase();
    
    if let Ok(entries) = fs::read_dir(&profile_plugins_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let folder_name = entry.file_name().to_string_lossy().to_string();
            
            if folder_name.to_lowercase().contains(&mod_name_lower) || 
               mod_name_lower.contains(&folder_name.to_lowercase()) {
                let src_path = entry.path();
                let dst_path = game_plugins_dir.join(&folder_name);
                
                if src_path.is_dir() {
                    eprintln!("[copy_mod_from_cache] Copying {} from cache to game", folder_name);
                    
                    // Ensure target dir exists
                    fs::create_dir_all(&game_plugins_dir).map_err(|e| e.to_string())?;
                    
                    // Remove existing if present
                    if dst_path.exists() {
                        let _ = fs::remove_dir_all(&dst_path);
                    }
                    
                    // Copy
                    copy_dir_recursive(&src_path, &dst_path).map_err(|e| e.to_string())?;
                    
                    return Ok(serde_json::json!({ "success": true, "copied": true }));
                }
            }
        }
    }
    
    // Not found in cache
    eprintln!("[copy_mod_from_cache] Mod {} not found in profile cache", mod_name);
    Ok(serde_json::json!({ "success": false, "copied": false }))
}

#[command]
pub async fn fetch_packages(app: AppHandle, state: tauri::State<'_, AppState>, game_id: String) -> Result<usize, String> {
    use std::time::SystemTime;
    use std::time::Duration;

    let start_time = SystemTime::now();
    
    // 0. Check if we already have packages in memory (instant return)
    {
        let packages_lock = state.packages.read().await;
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
        .user_agent("r2modmac/0.5.2")
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(25))
        .gzip(true)
        .build()
        .map_err(|e| e.to_string())?;

    fn decode_gzip_or_plain(bytes: &[u8], label: &str) -> Result<String, String> {
        // Thunderstore blobs are usually raw gzip bytes, but support plain JSON fallback.
        if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut gz = flate2::read::GzDecoder::new(bytes);
            let mut out = String::new();
            std::io::Read::read_to_string(&mut gz, &mut out)
                .map_err(|e| format!("Failed to decompress {}: {}", label, e))?;
            Ok(out)
        } else {
            String::from_utf8(bytes.to_vec())
                .map_err(|e| format!("Failed to decode {} as utf-8: {}", label, e))
        }
    }

    fn parse_index_chunk_urls(index_json: &str) -> Result<Vec<String>, String> {
        let parsed: serde_json::Value = serde_json::from_str(index_json)
            .map_err(|e| format!("Failed to parse index JSON: {}", e))?;

        let extract_urls = |arr: &[serde_json::Value]| -> Vec<String> {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        };

        if let Some(arr) = parsed.as_array() {
            let urls = extract_urls(arr);
            if !urls.is_empty() {
                return Ok(urls);
            }
        }

        if let Some(arr) = parsed.get("chunks").and_then(|v| v.as_array()) {
            let urls = extract_urls(arr);
            if !urls.is_empty() {
                return Ok(urls);
            }
        }

        Err("Index JSON has no valid chunk URLs".to_string())
    }

    let resp = client
        .get(&index_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch index: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Index request failed with status {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let index_json = decode_gzip_or_plain(&bytes, "index")?;
    let chunk_urls: Vec<String> = parse_index_chunk_urls(&index_json)?;
    let total_chunks = chunk_urls.len();
    eprintln!("[fetch_packages] Found {} chunks", total_chunks);
    if total_chunks == 0 {
        return Ok(0);
    }

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
        
        // Check cache (Async)
        if cache_file.exists() {
            if let Ok(bytes) = tokio::fs::read(&cache_file).await {
                if let Ok(mut packages) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
                    // Filter out Manager packages from cache too
                    packages.retain(|pkg| {
                        let full_name = pkg["full_name"].as_str().unwrap_or("");
                        !full_name.contains("ebkr-r2modman") && !full_name.contains("Tslat-ThunderstoreModManager")
                    });
                    return Ok(packages);
                }
            }
        }
        
        let mut last_error: Option<String> = None;
        for attempt in 1..=3 {
            let resp = match client.get(url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    last_error = Some(format!("Attempt {} network error: {}", attempt, e));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            };

            if !resp.status().is_success() {
                last_error = Some(format!("Attempt {} failed with status {}", attempt, resp.status()));
                tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                continue;
            }

            let bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    last_error = Some(format!("Attempt {} failed reading body: {}", attempt, e));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            };

            let json_str = if bytes.starts_with(&[0x1f, 0x8b]) {
                let mut gz = flate2::read::GzDecoder::new(&bytes[..]);
                let mut out = String::new();
                match std::io::Read::read_to_string(&mut gz, &mut out) {
                    Ok(_) => out,
                    Err(e) => {
                        last_error = Some(format!("Attempt {} failed decompressing gzip: {}", attempt, e));
                        tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                        continue;
                    }
                }
            } else {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(out) => out,
                    Err(e) => {
                        last_error = Some(format!("Attempt {} invalid utf-8 chunk: {}", attempt, e));
                        tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                        continue;
                    }
                }
            };

            let mut packages: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
                Ok(packages) => packages,
                Err(e) => {
                    last_error = Some(format!("Attempt {} failed parsing JSON chunk: {}", attempt, e));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            };
        
            // Filter out Manager packages (e.g. r2modman, Thunderstore Mod Manager) if they appear
            packages.retain(|pkg| {
                let full_name = pkg["full_name"].as_str().unwrap_or("");
                !full_name.contains("ebkr-r2modman") && !full_name.contains("Tslat-ThunderstoreModManager")
            });
        
            // Save to cache (Async)
            // We re-serialize to save clean JSON
            if let Ok(cache_data) = serde_json::to_vec(&packages) {
                let _ = tokio::fs::write(&cache_file, cache_data).await;
            }
        
            return Ok(packages);
        }

        Err(last_error.unwrap_or_else(|| "Failed to load chunk".to_string()))
    }

    // 3. Load FIRST successful chunk immediately for instant UI
    // If chunk #0 is slow/unavailable, try next few chunks to avoid infinite spinner.
    let mut first_loaded_index: Option<usize> = None;
    let first_attempts = std::cmp::min(5, chunk_urls.len());
    for idx in 0..first_attempts {
        let first_url = &chunk_urls[idx];
        match load_chunk(&client, first_url, &cache_dir).await {
            Ok(first_packages) => {
                let count = first_packages.len();
                eprintln!("[fetch_packages] First chunk loaded (index {}): {} packages", idx, count);
                
                // Update state immediately so UI can show something
                let mut packages_lock = state.packages.write().await;
                packages_lock.insert(game_id.clone(), first_packages);
                first_loaded_index = Some(idx);
                break;
            }
            Err(e) => {
                eprintln!("[fetch_packages] Failed first chunk attempt idx {}: {}", idx, e);
            }
        }
    }

    if first_loaded_index.is_none() {
        return Err("Failed to load initial package chunks (network timeout or CDN issue)".to_string());
    }

    // 4. Load remaining chunks in parallel (streaming to state)
    let remaining_urls: Vec<String> = chunk_urls
        .into_iter()
        .enumerate()
        .filter_map(|(idx, url)| if Some(idx) == first_loaded_index { None } else { Some(url) })
        .collect();
    
    if !remaining_urls.is_empty() {
        let packages_arc = state.packages.clone();
        let game_id_clone = game_id.clone();
        let cache_dir_clone = cache_dir.clone();
        let app_handle = app.clone();
        
        // Spawn background task for remaining chunks
        tokio::spawn(async move {
            let parallelism = 10usize;
            let mut stream = futures_util::stream::iter(remaining_urls)
                .map(|url| {
                    let cache_dir = cache_dir_clone.clone();
                    let client = client.clone();
                    async move { load_chunk(&client, &url, &cache_dir).await }
                })
                .buffer_unordered(parallelism);

            // Collect and add to state as they complete
            while let Some(result) = stream.next().await {
                match result {
                    Ok(packages) => {
                        let mut packages_lock = packages_arc.write().await;
                        if let Some(existing) = packages_lock.get_mut(&game_id_clone) {
                            existing.extend(packages);
                        }
                    }
                    Err(e) => eprintln!("[fetch_packages] Chunk error: {}", e),
                }
            }
            
            // Get final count and emit event for frontend
            let final_count = {
                let packages_lock = packages_arc.read().await;
                packages_lock.get(&game_id_clone).map(|p| p.len()).unwrap_or(0)
            };
            
            eprintln!("[fetch_packages] Background loading complete. Total: {} packages", final_count);
            
            // Emit event so frontend knows more packages are available
            let _ = app_handle.emit("packages-loaded", serde_json::json!({
                "game_id": game_id_clone,
                "total_count": final_count
            }));
        });
    }

    // 5. Return immediately with first chunk count
    let packages_lock = state.packages.read().await;
    let count = packages_lock.get(&game_id).map(|p| p.len()).unwrap_or(0);
    
    if let Ok(elapsed) = start_time.elapsed() {
        eprintln!("[fetch_packages] Initial load in {:.2?} ({} packages ready, {} chunks loading in background)", 
            elapsed, count, total_chunks - 1);
    }

    Ok(count)
}

#[command]
pub async fn get_available_categories(
    state: tauri::State<'_, AppState>,
    game_id: String
) -> Result<Vec<String>, String> {
    let packages_lock = state.packages.read().await;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        let mut categories: std::collections::HashSet<String> = std::collections::HashSet::new();
        
        for p in packages.iter() {
            if let Some(cats) = p.get("categories").and_then(|v| v.as_array()) {
                for cat in cats {
                    if let Some(cat_str) = cat.as_str() {
                        categories.insert(cat_str.to_string());
                    }
                }
            }
        }
        
        let mut result: Vec<String> = categories.into_iter().collect();
        result.sort();
        Ok(result)
    } else {
        Ok(vec![])
    }
}

#[command]
pub async fn get_packages(
    state: tauri::State<'_, AppState>, 
    game_id: String, 
    page: usize, 
    page_size: usize, 
    search: String,
    sort: Option<String>,
    nsfw: Option<bool>,
    deprecated: Option<bool>,
    sort_direction: Option<String>,
    categories: Option<Vec<String>>,
    mods: Option<bool>,
    modpacks: Option<bool>
) -> Result<Vec<serde_json::Value>, String> {
    let packages_lock = state.packages.read().await;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        // Initial filtering
        let mut filtered: Vec<&serde_json::Value> = packages.iter().filter(|p| {
            // 1. Search Filter
            if !search.is_empty() {
                let search_lower = search.to_lowercase();
                let name = p["name"].as_str().unwrap_or("").to_lowercase();
                let full_name = p["full_name"].as_str().unwrap_or("").to_lowercase();
                if !name.contains(&search_lower) && !full_name.contains(&search_lower) {
                    return false;
                }
            }

            // 2. NSFW Filter
            // If nsfw tag is FALSE (default): Hide NSFW content
            // If nsfw tag is TRUE: Show ONLY NSFW content
            let nsfw_tag_active = nsfw.unwrap_or(false);
            let is_nsfw = p.get("has_nsfw_content").and_then(|v| v.as_bool()).unwrap_or(false);
            if nsfw_tag_active {
                // Show ONLY NSFW
                if !is_nsfw { return false; }
            } else {
                // Hide NSFW
                if is_nsfw { return false; }
            }

            // 3. Deprecated Filter
            // If deprecated tag is FALSE (default): Hide Deprecated content
            // If deprecated tag is TRUE: Show ONLY Deprecated content
            let deprecated_tag_active = deprecated.unwrap_or(false);
            let is_deprecated = p.get("is_deprecated").and_then(|v| v.as_bool()).unwrap_or(false);
            if deprecated_tag_active {
                // Show ONLY Deprecated
                if !is_deprecated { return false; }
            } else {
                // Hide Deprecated
                if is_deprecated { return false; }
            }

            // 4. Mods/Modpacks Filter
            // Logic: Both OFF = show all, Both ON = show all, Only one ON = show only that type
            let mods_active = mods.unwrap_or(false);
            let modpacks_active = modpacks.unwrap_or(false);
            
            // Check if package is a modpack (has "Modpacks" in categories)
            let pkg_categories: Vec<String> = p.get("categories")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|c| c.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let is_modpack = pkg_categories.iter().any(|c| c.to_lowercase() == "modpacks");
            
            // Apply filter only if exactly one is active
            if mods_active != modpacks_active {
                if mods_active && is_modpack {
                    return false; // Show only mods, this is a modpack - hide it
                }
                if modpacks_active && !is_modpack {
                    return false; // Show only modpacks, this is a mod - hide it
                }
            }

            // 5. Category/Tag Filter
            // If categories is empty or None, show all
            // If categories has values, package must match at least one:
            // - Match in categories array
            // - OR match in package name
            // - OR match in package description
            if let Some(ref filter_cats) = categories {
                if !filter_cats.is_empty() {
                    let pkg_categories: Vec<String> = p.get("categories")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|c| c.as_str().map(|s| s.to_lowercase())).collect())
                        .unwrap_or_default();
                    
                    let pkg_name = p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let pkg_full_name = p.get("full_name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    
                    // Check if any filter tag matches
                    let has_match = filter_cats.iter().any(|fc| {
                        let fc_lower = fc.to_lowercase();
                        // Match in categories
                        pkg_categories.iter().any(|c| c.contains(&fc_lower)) ||
                        // Match in name
                        pkg_name.contains(&fc_lower) ||
                        pkg_full_name.contains(&fc_lower)
                    });
                    
                    if !has_match {
                        return false;
                    }
                }
            }

            true
        }).collect();

        // Sorting
        if let Some(sort_by) = sort {
            let direction = sort_direction.unwrap_or("desc".to_string());
            let is_asc = direction == "asc";

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
                    if is_asc { da.cmp(&db) } else { db.cmp(&da) }
                }),
                "rating" => filtered.sort_by(|a, b| {
                    let ra = a.get("rating_score").and_then(|v| v.as_i64()).unwrap_or(0);
                    let rb = b.get("rating_score").and_then(|v| v.as_i64()).unwrap_or(0);
                    if is_asc { ra.cmp(&rb) } else { rb.cmp(&ra) }
                }),
                "updated" => filtered.sort_by(|a, b| {
                    let da = a.get("date_updated").and_then(|v| v.as_str()).unwrap_or("");
                    let db = b.get("date_updated").and_then(|v| v.as_str()).unwrap_or("");
                    if is_asc { da.cmp(db) } else { db.cmp(da) }
                }),
                "alphabetical" => filtered.sort_by(|a, b| {
                    let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    // Ascending: A-Z (na vs nb)
                    // Descending: Z-A (nb vs na)
                    if is_asc { na.cmp(&nb) } else { nb.cmp(&na) }
                }),
                _ => {}
            }
        }

        let start = page * page_size;
        if start >= filtered.len() {
            return Ok(vec![]);
        }
        
        let end = std::cmp::min(start + page_size, filtered.len());
        let slice: Vec<serde_json::Value> = filtered[start..end].iter().map(|&v| v.clone()).collect();
        Ok(slice)
    } else {
        Ok(vec![])
    }
}

#[command]
pub async fn lookup_packages_by_names(
    state: tauri::State<'_, AppState>,
    game_id: String,
    names: Vec<String>
) -> Result<serde_json::Value, String> {
    let packages_lock = state.packages.read().await;
    
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
pub async fn fetch_package_by_name(state: tauri::State<'_, AppState>, name: String, game_id: Option<String>) -> Result<Option<serde_json::Value>, String> {
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
        let packages_lock = state.packages.read().await;
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
