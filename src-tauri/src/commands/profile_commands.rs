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
    let profile_path = crate::utils::paths::app_data_dir(&app).unwrap().join("profiles.json");
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
    let profile_path = crate::utils::paths::app_data_dir(&app).unwrap().join("profiles.json");

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
    let profile_dir = crate::utils::paths::app_data_dir(&app)
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
    let profile_dir = crate::utils::paths::app_data_dir(&app)
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
    let profiles_dir = crate::utils::paths::app_data_dir(&app)
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

        // Clean up gzipped package index caches
        if let Ok(entries) = fs::read_dir(&cache_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with("_packages.json.gz") || name.ends_with("_packages_v2.json.gz") {
                    if let Ok(meta) = entry.metadata() {
                        size_freed += meta.len();
                    }
                    eprintln!("[clear_profile_cache] Removing package cache file: {}", name);
                    let _ = fs::remove_file(entry.path());
                }
            }
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
    let profile_path = crate::utils::paths::app_data_dir(&app).unwrap().join("profiles.json");
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

// ── Config Editor Commands ────────────────────────────────────────────────────

/// Metadata about a config file inside a profile directory.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ConfigFileInfo {
    /// Display name of the file (e.g. "BepInEx.cfg")
    pub name: String,
    /// Path relative to the root (e.g. "BepInEx/config/BepInEx.cfg")
    pub relative_path: String,
    /// Absolute path to the root directory that `relative_path` is relative to.
    /// Needed by read/write commands to reconstruct the full path.
    pub root: String,
}

/// Extensions we surface in the config editor.
const CONFIG_EXTENSIONS: &[&str] = &["cfg", "ini", "json", "yml", "yaml", "txt"];

/// Top-level profile sub-directories that we skip entirely (they contain DLLs /
/// game binaries, not user-editable config files).
const SKIP_DIRS: &[&str] = &[
    "dotnet",
    "_state",
    "patchers",
    "cache",
    "unity-libs",
    "interop",
    "bin",
    ".r2modmac",
];

/// Recursively walk `dir`, collecting files whose extension is in
/// `CONFIG_EXTENSIONS`, while skipping `SKIP_DIRS` and `manifest.json` files.
fn collect_config_files(
    dir: &std::path::Path,
    profile_root: &std::path::Path,
    out: &mut Vec<ConfigFileInfo>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            // Skip known binary/state directories.
            if SKIP_DIRS.iter().any(|s| name.eq_ignore_ascii_case(s)) {
                continue;
            }
            collect_config_files(&path, profile_root, out);
        } else if path.is_file() {
            // Skip manifest.json / README / icon files / mods.yml.
            let lower = name.to_lowercase();
            if lower == "manifest.json"
                || lower == "readme.md"
                || lower == "readme.txt"
                || lower == "icon.png"
                || lower == "mods.yml"
            {
                continue;
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if CONFIG_EXTENSIONS.contains(&ext.as_str()) {
                if let Ok(rel) = path.strip_prefix(profile_root) {
                    // Use forward slashes for cross-platform consistency on the
                    // JS side.
                    let relative_path = rel
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("/");
                    out.push(ConfigFileInfo {
                        name: name.clone(),
                        relative_path,
                        root: profile_root.to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
}

/// Like `collect_config_files` but only scans `dir` non-recursively (one level),
/// producing paths relative to `root`.  Used for flat config directories like
/// `BepInEx/config/` inside the game folder.
fn collect_config_files_flat(
    dir: &std::path::Path,
    root: &std::path::Path,
    out: &mut Vec<ConfigFileInfo>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if lower == "manifest.json" || lower == "mods.yml" {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if CONFIG_EXTENSIONS.contains(&ext.as_str()) {
            if let Ok(rel) = path.strip_prefix(root) {
                let relative_path = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(ConfigFileInfo {
                    name: name.clone(),
                    relative_path,
                    root: root.to_string_lossy().to_string(),
                });
            }
        }
    }
}

#[command]
pub fn list_profile_config_files(
    app: AppHandle,
    profile_id: String,
    game_identifier: Option<String>,
    platform: Option<String>,
) -> Result<Vec<ConfigFileInfo>, String> {
    let mut files: Vec<ConfigFileInfo> = Vec::new();

    // ── 1. Profile directory (covers Windows profiles where BepInEx lives here)
    let profile_dir = crate::utils::paths::app_data_dir(&app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(&profile_id);

    if profile_dir.exists() {
        collect_config_files(&profile_dir, &profile_dir, &mut files);
    }

    // ── 2. Game directory (covers Mac profiles where BepInEx is installed into
    //       the game folder).  Only scan BepInEx/config/ and skip DLL-heavy
    //       subdirs so we don't surface binary files.
    if let (Some(game_id), Some(plat)) = (game_identifier, platform) {
        let settings_path = crate::utils::paths::app_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("settings.json");

        if settings_path.exists() {
            if let Ok(raw) = fs::read_to_string(&settings_path) {
                if let Ok(settings) = serde_json::from_str::<serde_json::Value>(&raw) {
                    // Try  "<id>::<platform>"  then bare  "<id>"
                    let keys = [
                        format!("{}::{}", game_id, plat),
                        game_id.clone(),
                    ];
                    let mut game_path_str: Option<String> = None;
                    if let Some(paths) = settings.get("game_paths").and_then(|v| v.as_object()) {
                        for key in &keys {
                            if let Some(p) = paths.get(key).and_then(|v| v.as_str()) {
                                game_path_str = Some(p.to_string());
                                break;
                            }
                        }
                    }

                    if let Some(gp) = game_path_str {
                        let game_path = std::path::Path::new(&gp);
                        // BepInEx/config  — flat scan
                        let bep_config = game_path.join("BepInEx").join("config");
                        if bep_config.is_dir() {
                            collect_config_files_flat(&bep_config, game_path, &mut files);
                        }
                        // BepInEx/plugins — recursive, but only config-extension files
                        let bep_plugins = game_path.join("BepInEx").join("plugins");
                        if bep_plugins.is_dir() {
                            collect_config_files(&bep_plugins, game_path, &mut files);
                        }
                    }
                }
            }
        }
    }

    // De-duplicate by relative_path (same file found through both paths).
    // If a file exists in both the profile and game directories, prioritize the game directory (which has root != profile_dir).
    let profile_dir_str = profile_dir.to_string_lossy().to_string();
    files.sort_by(|a, b| {
        let cmp = a.relative_path.cmp(&b.relative_path);
        if cmp == std::cmp::Ordering::Equal {
            let a_is_profile = a.root == profile_dir_str;
            let b_is_profile = b.root == profile_dir_str;
            a_is_profile.cmp(&b_is_profile)
        } else {
            cmp
        }
    });
    files.dedup_by(|a, b| a.relative_path == b.relative_path);
    Ok(files)
}

#[command]
pub fn read_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    root: Option<String>,
) -> Result<String, String> {
    // If a root override is provided (e.g. game folder), use that instead of
    // the profile directory.
    let base_dir = if let Some(r) = root {
        std::path::PathBuf::from(r)
    } else {
        crate::utils::paths::app_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("profiles")
            .join(&profile_id)
    };

    // Prevent path traversal attacks.
    let target = base_dir.join(&relative_path);
    let canonical_target =
        target.canonicalize().map_err(|e| format!("Cannot resolve path: {}", e))?;
    let canonical_root = base_dir
        .canonicalize()
        .map_err(|e| format!("Cannot resolve root dir: {}", e))?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err("Path traversal detected".to_string());
    }

    fs::read_to_string(&canonical_target).map_err(|e| format!("Failed to read file: {}", e))
}

#[command]
pub fn write_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    content: String,
    root: Option<String>,
) -> Result<bool, String> {
    let base_dir = if let Some(r) = root {
        std::path::PathBuf::from(r)
    } else {
        crate::utils::paths::app_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("profiles")
            .join(&profile_id)
    };

    // Build target path and validate it stays inside the base directory.
    let target = base_dir.join(&relative_path);

    // Normalise without canonicalize (file may not exist yet for new files).
    let normalised = target
        .components()
        .fold(std::path::PathBuf::new(), |mut acc, c| {
            match c {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                other => acc.push(other),
            }
            acc
        });

    if !normalised.starts_with(&base_dir) {
        return Err("Path traversal detected".to_string());
    }

    // Ensure parent directories exist.
    if let Some(parent) = normalised.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directories: {}", e))?;
    }

    fs::write(&normalised, content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(true)
}

#[command]
pub fn reveal_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    root: Option<String>,
) -> Result<(), String> {
    let base_dir = if let Some(r) = root {
        std::path::PathBuf::from(r)
    } else {
        crate::utils::paths::app_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("profiles")
            .join(&profile_id)
    };

    let target = base_dir.join(&relative_path);
    let normalised = target
        .components()
        .fold(std::path::PathBuf::new(), |mut acc, c| {
            match c {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                other => acc.push(other),
            }
            acc
        });

    if !normalised.starts_with(&base_dir) {
        return Err("Path traversal detected".to_string());
    }

    if !normalised.exists() {
        return Err("File does not exist".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&normalised)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&normalised)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(parent) = normalised.parent() {
            open::that(parent).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[command]
pub fn open_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    root: Option<String>,
) -> Result<(), String> {
    let base_dir = if let Some(r) = root {
        std::path::PathBuf::from(r)
    } else {
        crate::utils::paths::app_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("profiles")
            .join(&profile_id)
    };

    let target = base_dir.join(&relative_path);
    let normalised = target
        .components()
        .fold(std::path::PathBuf::new(), |mut acc, c| {
            match c {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                other => acc.push(other),
            }
            acc
        });

    if !normalised.starts_with(&base_dir) {
        return Err("Path traversal detected".to_string());
    }

    if !normalised.exists() {
        return Err("File does not exist".to_string());
    }

    open::that(&normalised).map_err(|e| e.to_string())?;
    Ok(())
}

