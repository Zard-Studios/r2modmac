use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AppState {
    pub packages: Arc<RwLock<HashMap<String, Vec<serde_json::Value>>>>,
    pub platform_cache: Arc<RwLock<HashMap<String, CachedPlatform>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            packages: Arc::new(RwLock::new(HashMap::new())),
            platform_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlatformInfo {
    pub windows: bool,
    pub mac: bool,
    pub linux: bool,
    pub confidence: f32,
    pub source: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CachedPlatform {
    pub info: PlatformInfo,
    pub fetched_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub steam_path: Option<String>,
    #[serde(default)]
    pub windows_steam_path: Option<String>,
    #[serde(default)]
    pub mac_steam_path: Option<String>,
    #[serde(default)]
    pub favorite_games: Vec<String>,
    #[serde(default)]
    pub game_paths: HashMap<String, String>,
    #[serde(default)]
    pub steam_launch_option_backups: HashMap<String, String>,
    #[serde(default)]
    pub legacy_install_mode: bool,
    #[serde(default = "default_true")]
    pub ask_version_before_install: bool,
    #[serde(default = "default_true")]
    pub install_in_parallel: bool,
    #[serde(default)]
    pub confirm_before_apply_to_game: bool,
    #[serde(default = "default_true")]
    pub write_debug_logs_to_game: bool,
    #[serde(default = "default_mod_view_mode")]
    pub default_mod_view_mode: String,
    #[serde(default)]
    pub hide_crossover_guide: bool,
    #[serde(default)]
    pub hide_macos_guide: bool,
    #[serde(default)]
    pub thunderstore_chunk_cache_migrated: bool,
}

impl Settings {
    pub fn default() -> Self {
        Self {
            steam_path: None,
            windows_steam_path: None,
            mac_steam_path: None,
            favorite_games: Vec::new(),
            game_paths: HashMap::new(),
            steam_launch_option_backups: HashMap::new(),
            legacy_install_mode: false,
            ask_version_before_install: true,
            install_in_parallel: true,
            confirm_before_apply_to_game: false,
            write_debug_logs_to_game: true,
            default_mod_view_mode: default_mod_view_mode(),
            hide_crossover_guide: false,
            hide_macos_guide: false,
            thunderstore_chunk_cache_migrated: false,
        }
    }
}

fn default_mod_view_mode() -> String {
    "grid".to_string()
}

fn default_true() -> bool {
    true
}

pub fn get_settings_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager;
    app.path().app_data_dir().unwrap().join("settings.json")
}

pub fn load_settings_impl(app: &tauri::AppHandle) -> Settings {
    let path = get_settings_path(app);
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str(&data) {
                return settings;
            }
        }
    }
    Settings::default()
}

pub fn save_settings_impl(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let path = get_settings_path(app);
    let data = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn normalize_for_matching(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

pub fn is_balatro_identifier(game_identifier: &str) -> bool {
    normalize_for_matching(game_identifier) == "balatro"
}

pub fn is_balatro_game_path(path: &std::path::Path) -> bool {
    if path
        .join("Balatro.app")
        .join("Contents")
        .join("MacOS")
        .join("love")
        .exists()
    {
        return true;
    }

    if path
        .join("Contents")
        .join("MacOS")
        .join("love")
        .exists()
    {
        return true;
    }

    false
}

pub fn get_balatro_mods_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Balatro")
            .join("Mods")
    })
}

pub fn get_steam_library_folders(steam_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut folders = vec![steam_path.to_path_buf()];
    let library_folders_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    if library_folders_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&library_folders_path) {
            let re = regex::Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
            for cap in re.captures_iter(&content) {
                folders.push(std::path::PathBuf::from(&cap[1]));
            }
        }
    }
    folders
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub notes: String,
    pub download_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub assets: Vec<GithubAsset>,
    pub body: String,
}

#[derive(Serialize, Deserialize)]
pub struct GithubAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Serialize, Deserialize)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

pub fn clean_mod_name(name: &str, version: &str) -> String {
    let suffix = format!("-{}", version);
    if name.ends_with(&suffix) {
        name[0..name.len() - suffix.len()].to_string()
    } else {
        name.to_string()
    }
}

pub fn normalize_zip_entry_path(name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    let trimmed = normalized.trim().trim_start_matches("./").trim_start_matches('/');

    if trimmed.is_empty() {
        return None;
    }

    let mut path = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(value) => path.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

pub fn normalize_zip_entry_name(name: &str) -> Option<String> {
    let path = normalize_zip_entry_path(name)?;
    let normalized = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub fn zip_entry_is_dir(name: &str) -> bool {
    name.replace('\\', "/").ends_with('/')
}

pub fn detect_bepinex_structure<R: std::io::Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> (bool, Option<String>) {
    let mut found_bepinex_core = false;
    let mut found_root_level_dll = false;
    let mut root_prefix: Option<String> = None;
    
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index_raw(i) {
            let Some(name) = normalize_zip_entry_name(file.name()) else {
                continue;
            };
            
            if name.contains("BepInEx/core/") || name.ends_with("BepInEx/core") {
                found_bepinex_core = true;
                if let Some(idx) = name.find("BepInEx/") {
                    let prefix = &name[..idx];
                    if !prefix.is_empty() && root_prefix.is_none() {
                        root_prefix = Some(prefix.to_string());
                    }
                }
            }
            
            if name.ends_with("winhttp.dll") || name.ends_with("doorstop_config.ini") {
                found_root_level_dll = true;
                if let Some(idx) = name.rfind('/') {
                    let prefix = &name[..idx + 1];
                    if root_prefix.is_none() {
                        root_prefix = Some(prefix.to_string());
                    }
                }
            }
        }
    }
    
    let is_bepinex = found_bepinex_core || found_root_level_dll;
    if is_bepinex && root_prefix.is_none() {
        root_prefix = Some(String::new());
    }
    
    (is_bepinex, root_prefix)
}
