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
}

impl Settings {
    fn default() -> Self {
        // Default Steam path on macOS
        let home = dirs::home_dir().unwrap_or_default();
        let steam_path = home.join("Library/Application Support/Steam");
        Self {
            steam_path: if steam_path.exists() { Some(steam_path.to_string_lossy().to_string()) } else { None },
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

#[command]
async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let path = get_settings_path(&app);
    let data = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(())
}

#[command]
async fn get_game_path(app: AppHandle, game_identifier: String) -> Result<Option<String>, String> {
    let settings = load_settings_impl(&app);
    let steam_path_str = settings.steam_path.ok_or("Steam path not configured")?;
    let steam_path = std::path::Path::new(&steam_path_str);

    // Map gameIdentifier to Steam AppID
    // TODO: Move this to a better place or load from ecosystem.json
    let app_id = match game_identifier.as_str() {
        "lethal-company" => "1966720",
        "risk-of-rain-2" => "632360",
        _ => return Err(format!("Unknown game identifier: {}", game_identifier)),
    };

    // 1. Check default steamapps location
    let default_manifest = steam_path.join("steamapps").join(format!("appmanifest_{}.acf", app_id));
    if default_manifest.exists() {
        return parse_manifest_for_path(&default_manifest);
    }

    // 2. Check libraryfolders.vdf
    let library_folders_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    if library_folders_path.exists() {
        let content = fs::read_to_string(&library_folders_path).map_err(|e| e.to_string())?;
        
        // Simple regex to find paths in libraryfolders.vdf
        // "path"		"/Users/username/Library/Application Support/Steam"
        let re = regex::Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
        
        for cap in re.captures_iter(&content) {
            let lib_path_str = &cap[1];
            let lib_path = std::path::Path::new(lib_path_str);
            let manifest_path = lib_path.join("steamapps").join(format!("appmanifest_{}.acf", app_id));
            
            if manifest_path.exists() {
                return parse_manifest_for_path(&manifest_path);
            }
        }
    }

    Ok(None)
}

fn parse_manifest_for_path(manifest_path: &std::path::Path) -> Result<Option<String>, String> {
    let content = fs::read_to_string(manifest_path).map_err(|e| e.to_string())?;
    
    // Regex to find installdir
    // "installdir"		"Lethal Company"
    let re = regex::Regex::new(r#""installdir"\s+"([^"]+)""#).unwrap();
    
    if let Some(cap) = re.captures(&content) {
        let install_dir_name = &cap[1];
        let full_path = manifest_path.parent().unwrap().join("common").join(install_dir_name);
        if full_path.exists() {
            return Ok(Some(full_path.to_string_lossy().to_string()));
        }
    }
    
    Ok(None)
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
            check_directory_exists,
            export_profile,
            share_profile,
            get_settings,
            save_settings,
            get_game_path,
            get_game_path,
            confirm_dialog,
            read_image
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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

    // Create directories
    fs::create_dir_all(&mod_dir).map_err(|e| e.to_string())?;

    // Download
    let response = reqwest::get(&download_url).await.map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let cursor = std::io::Cursor::new(bytes);

    // Unzip
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    
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
                if let Ok(packages) = serde_json::from_reader::<_, Vec<serde_json::Value>>(reader) {
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
        let packages: Vec<serde_json::Value> = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;
        
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
    search: String
) -> Result<Vec<serde_json::Value>, String> {
    let packages_lock = state.packages.lock().map_err(|_| "Failed to lock state".to_string())?;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        let filtered: Vec<&serde_json::Value> = if search.is_empty() {
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
async fn delete_profile_folder(app: AppHandle, profile_id: String) -> Result<bool, String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
    
    if profile_dir.exists() {
        fs::remove_dir_all(profile_dir).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
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
