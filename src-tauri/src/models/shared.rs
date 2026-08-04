use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;

/// Maximum number of game package-lists kept in memory at once. Each game's
/// Thunderstore listing can be tens of MB of JSON; without eviction the cache
/// grows unbounded as the user browses different games.
const MAX_CACHED_GAMES: usize = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackageVersion {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    pub version_number: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub download_url: String,
    #[serde(default)]
    pub downloads: i64,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub file_size: u64,
    // Fields below are skipped from deserialization to save memory.
    // date_created and is_active are truly unused in the install/UI flow.
    pub uuid4: String,
    pub full_name: String,
    #[serde(skip_deserializing, default)]
    pub date_created: String,
    #[serde(skip_deserializing, default)]
    pub is_active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Package {
    pub name: String,
    pub full_name: String,
    // owner is never accessed directly — derived from full_name.split('-')[0] in the frontend.
    #[serde(skip_deserializing, default)]
    pub owner: String,
    // package_url is never read from real data (only from mocks in ProfileSidebar).
    #[serde(skip_deserializing, default)]
    pub package_url: String,
    // date_created on Package is unused; only date_updated is used for sorting.
    #[serde(skip_deserializing, default)]
    pub date_created: String,
    pub date_updated: String,
    pub uuid4: String,
    #[serde(default)]
    pub rating_score: i64,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub is_deprecated: bool,
    #[serde(default)]
    pub has_nsfw_content: bool,
    #[serde(default)]
    pub categories: Vec<String>,
    pub versions: Vec<PackageVersion>,
}

pub struct AppState {
    pub packages: Arc<RwLock<HashMap<String, Vec<Package>>>>,
    pub platform_cache: Arc<RwLock<HashMap<String, CachedPlatform>>>,
    /// Insertion order of cached game ids (most-recent at the end) so we can
    /// evict the oldest entry when the cache grows past MAX_CACHED_GAMES.
    packages_order: Arc<Mutex<Vec<String>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            packages: Arc::new(RwLock::new(HashMap::new())),
            platform_cache: Arc::new(RwLock::new(HashMap::new())),
            packages_order: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AppState {
    /// Record that a game's packages were (re)cached, then evict the oldest
    /// entries beyond MAX_CACHED_GAMES to keep memory bounded.
    pub async fn touch_packages_cache(&self, game_id: &str) {
        let mut order = self.packages_order.lock().await;
        order.retain(|id| id != game_id);
        order.push(game_id.to_string());

        if order.len() > MAX_CACHED_GAMES {
            let drop_count = order.len() - MAX_CACHED_GAMES;
            let to_drop: Vec<String> = order.drain(..drop_count).collect();
            drop(order);
            if !to_drop.is_empty() {
                let mut packages_lock = self.packages.write().await;
                let mut order = self.packages_order.lock().await;
                for id in &to_drop {
                    if packages_lock.remove(id).is_some() {
                        eprintln!(
                            "[packages-cache] evicted {} (limit {})",
                            id, MAX_CACHED_GAMES
                        );
                    }
                }
                order.retain(|id| !to_drop.contains(id));
            }
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
    #[serde(default = "default_false")]
    pub write_debug_logs_to_game: bool,
    #[serde(default = "default_mod_view_mode")]
    pub default_mod_view_mode: String,
    #[serde(default = "default_true")]
    pub show_deprecated_warnings: bool,
    #[serde(default)]
    pub hide_crossover_guide: bool,
    #[serde(default)]
    pub hide_macos_guide: bool,
    #[serde(default)]
    pub thunderstore_chunk_cache_migrated: bool,
    #[serde(default)]
    pub stream_mode: bool,
    #[serde(default = "default_true")]
    pub sponsored_messages_enabled: bool,
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
            ask_version_before_install: false,
            install_in_parallel: true,
            confirm_before_apply_to_game: false,
            write_debug_logs_to_game: false,
            default_mod_view_mode: default_mod_view_mode(),
            show_deprecated_warnings: true,
            hide_crossover_guide: false,
            hide_macos_guide: false,
            thunderstore_chunk_cache_migrated: false,
            stream_mode: false,
            sponsored_messages_enabled: true,
        }
    }
}

fn default_mod_view_mode() -> String {
    "grid".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

pub fn get_settings_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::utils::paths::app_data_dir(app)
        .unwrap()
        .join("settings.json")
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
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
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

    if path.join("Contents").join("MacOS").join("love").exists() {
        return true;
    }

    false
}

pub fn get_balatro_mods_dir() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|dir| {
        dir.join("Balatro").join("Mods")
    })
}

pub fn is_outerwilds_identifier(game_identifier: &str) -> bool {
    normalize_for_matching(game_identifier) == "outerwilds"
}

pub fn is_risk_of_rain_returns_identifier(game_identifier: &str) -> bool {
    normalize_for_matching(game_identifier) == "riskofrainreturns"
}

pub fn is_risk_of_rain_returns_game_path(path: &std::path::Path) -> bool {
    path.join("Risk of Rain Returns.exe").is_file()
        || path.join("Risk of Rain Returns").is_file()
}

pub fn is_outerwilds_game_path(path: &std::path::Path) -> bool {
    // Detect by OuterWilds.exe in the game directory or inside a bottle prefix
    if path.join("OuterWilds.exe").exists() {
        return true;
    }
    // If we're given the bottle/prefix root, look inside drive_c
    if path
        .join("drive_c")
        .join("Program Files (x86)")
        .join("Steam")
        .join("steamapps")
        .join("common")
        .join("Outer Wilds")
        .join("OuterWilds.exe")
        .exists()
    {
        return true;
    }
    false
}

/// Returns the path to the OWML runtime folder inside the game directory.
///
/// The folder is physically at `<game_path>/OWML/` while the profile is in
/// modded mode, but it gets renamed to `<game_path>/OWML_DISABLED/` when the
/// profile is in vanilla mode (mods stay on disk, only the runtime is hidden
/// from the game). Callers that need to find mods/configs must therefore look
/// in whichever of the two actually exists, preferring the active `OWML`.
///
/// Priority: <game_path>/OWML/ (active) -> <game_path>/OWML_DISABLED/ (vanilla).
pub fn get_owml_dir(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let normal = game_path.join("OWML");
    if normal.exists() {
        return Some(normal);
    }
    let disabled = game_path.join("OWML_DISABLED");
    if disabled.exists() {
        return Some(disabled);
    }
    None
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
    let trimmed = normalized
        .trim()
        .trim_start_matches("./")
        .trim_start_matches('/');

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

pub fn detect_bepinex_structure<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> (bool, Option<String>) {
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
