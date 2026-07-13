use crate::commands::game_commands::{ensure_macos_steam_launch_options, get_game_path};
use crate::commands::game_commands::{restore_outerwilds_vanilla, restore_mscorlib_vanilla};
use crate::models::shared::{
    get_balatro_mods_dir, is_balatro_game_path, is_balatro_identifier,
    is_outerwilds_identifier, is_outerwilds_game_path, AppState,
};
use crate::utils::file_ops::*;
use std::fs;
use std::io::Write;
use std::sync::OnceLock;
use tauri::{command, AppHandle, State};

static PROFILE_SAVE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn is_bepinex_shell_script(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".sh") && lower.contains("bepinex")
}

#[command]
pub fn get_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let profile_path = crate::utils::paths::app_data_dir(&app).unwrap().join("profiles.json");
    let backup_path = profile_path.with_extension("json.bak");
    if !profile_path.exists() {
        if backup_path.exists() {
            fs::copy(&backup_path, &profile_path).map_err(|e| e.to_string())?;
        } else {
            return Ok(vec![]);
        }
    }
    let data = fs::read_to_string(&profile_path).map_err(|e| e.to_string())?;
    match serde_json::from_str(&data) {
        Ok(profiles) => Ok(profiles),
        Err(primary_error) if backup_path.exists() => {
            let backup = fs::read_to_string(&backup_path).map_err(|e| e.to_string())?;
            serde_json::from_str(&backup).map_err(|backup_error| {
                format!(
                    "Profiles file and recovery backup are invalid: {}; {}",
                    primary_error, backup_error
                )
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

#[command]
pub async fn save_profiles(
    app: AppHandle,
    profiles: Vec<serde_json::Value>,
) -> Result<bool, String> {
    let save_lock = PROFILE_SAVE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _save_guard = save_lock.lock().await;
    let profile_path = crate::utils::paths::app_data_dir(&app).unwrap().join("profiles.json");

    // Serialize first (fast operation)
    let data = serde_json::to_string_pretty(&profiles).map_err(|e| e.to_string())?;

    // Write to disk in blocking thread pool to avoid blocking async runtime
    tokio::task::spawn_blocking(move || {
        // Ensure dir exists
        if let Some(parent) = profile_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let temp_path = profile_path.with_extension("json.tmp");
        let backup_path = profile_path.with_extension("json.bak");
        let mut temp_file = fs::File::create(&temp_path)?;
        temp_file.write_all(data.as_bytes())?;
        temp_file.sync_all()?;
        drop(temp_file);
        #[cfg(target_os = "windows")]
        if profile_path.exists() {
            if backup_path.exists() {
                fs::remove_file(&backup_path)?;
            }
            fs::rename(&profile_path, &backup_path)?;
        }
        let rename_result = fs::rename(&temp_path, &profile_path);
        #[cfg(target_os = "windows")]
        if rename_result.is_err() && backup_path.exists() && !profile_path.exists() {
            let _ = fs::rename(&backup_path, &profile_path);
        }
        rename_result?;
        if backup_path.exists() {
            let _ = fs::remove_file(backup_path);
        }
        Ok::<(), std::io::Error>(())
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
        if let Ok(Some(game_path_str)) = get_game_path(app.clone(), game_id.clone(), platform.clone()).await
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

            // --- Outer Wilds (OWML) cleanup: restore full vanilla state ---
            let is_outerwilds = is_outerwilds_identifier(&game_id)
                || is_outerwilds_game_path(game_path);
            if is_outerwilds {
                // 1. Restore vanilla Assembly-CSharp.dll BEFORE removing OWML,
                //    so the backup lookup still works if it was inside the game root.
                if let Err(e) = restore_outerwilds_vanilla(game_path) {
                    eprintln!("[delete_profile] Could not restore vanilla DLL (non-fatal): {}", e);
                } else {
                    eprintln!("[delete_profile] Restored vanilla Assembly-CSharp.dll");
                }

                // 1a. Restore vanilla mscorlib.dll and delete its backup.
                if let Err(e) = restore_mscorlib_vanilla(game_path, true) {
                    eprintln!("[delete_profile] Could not restore vanilla mscorlib.dll (non-fatal): {}", e);
                } else {
                    eprintln!("[delete_profile] Restored vanilla mscorlib.dll");
                }

                // 1b. Clean up boot.config modifications.
                let boot_config_path = game_path.join("OuterWilds_Data").join("boot.config");
                if boot_config_path.exists() {
                    if let Ok(content) = fs::read_to_string(&boot_config_path) {
                        let updated = content.replace("gc-max-time-slice=3", "").replace("\r\n\r\n", "\r\n").replace("\n\n", "\n");
                        let _ = fs::write(&boot_config_path, updated);
                        eprintln!("[delete_profile] Restored boot.config");
                    }
                }

                // 2. Remove OWML folder (the whole mod runtime, not just Mods/).
                let owml_dir = game_path.join("OWML");
                if owml_dir.exists() {
                    eprintln!("[delete_profile] Removing OWML folder from game");
                    let _ = fs::remove_dir_all(&owml_dir);
                    removed_runtime_artifact = true;
                }

                // 3. Also remove OWML_DISABLED if it was left from a vanilla launch.
                let owml_disabled_dir = game_path.join("OWML_DISABLED");
                if owml_disabled_dir.exists() {
                    eprintln!("[delete_profile] Removing OWML_DISABLED folder from game");
                    let _ = fs::remove_dir_all(&owml_disabled_dir);
                    removed_runtime_artifact = true;
                }

                // 4. Remove the OWMLPatcher.exe we dropped in the game root.
                let patcher_exe = game_path.join("OWMLPatcher.exe");
                if patcher_exe.exists() {
                    let _ = fs::remove_file(&patcher_exe);
                    eprintln!("[delete_profile] Removed OWMLPatcher.exe");
                }
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
                    let cache_dirs = ["BepInEx", "Balatro", "MelonLoader"];
                    for dir_name in cache_dirs.iter() {
                        let cache_dir = entry.path().join(dir_name);
                        if cache_dir.exists() {
                            // Calculate size before deleting
                            if let Ok(size) = calculate_dir_size(&cache_dir) {
                                size_freed += size;
                            }
                            eprintln!("[clear_profile_cache] Removing: {:?}", cache_dir);
                            let _ = fs::remove_dir_all(&cache_dir);
                            cleared += 1;
                        }
                    }
                }
            }
        }
    }

    if let Ok(cache_dir) = crate::utils::paths::app_cache_dir(&app) {
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

        let downloads_dir = cache_dir.join("mod-downloads");
        if downloads_dir.exists() {
            if let Ok(size) = calculate_dir_size(&downloads_dir) {
                size_freed += size;
            }
            let _ = fs::remove_dir_all(&downloads_dir);
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
                        if game_id == "outerwilds" {
                            // The OWML folder may be at OWML (modded mode) or OWML_DISABLED
                            // (vanilla mode, where mods stay on disk but the runtime is hidden).
                            // Resolve whichever actually exists so configs are always listed.
                            if let Some(owml_dir) =
                                crate::models::shared::get_owml_dir(game_path)
                            {
                                // 1. Flat scan OWML root for global configs
                                collect_config_files_flat(&owml_dir, game_path, &mut files);
                                // 2. Recursive scan OWML/Mods for mod configs
                                let owml_mods = owml_dir.join("Mods");
                                if owml_mods.is_dir() {
                                    collect_config_files(&owml_mods, game_path, &mut files);
                                }
                            }
                        } else {
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

    let content = fs::read_to_string(&canonical_target)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    // Strip UTF-8 BOM (Byte Order Mark) if present.
    let stripped = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
    Ok(stripped.to_string())
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
