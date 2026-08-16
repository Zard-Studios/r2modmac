use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
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
                        log::debug!(
                            "[packages-cache] evicted {} (limit {})",
                            id,
                            MAX_CACHED_GAMES
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

/// Field order here is the field order on disk, so new fields go at the end.
/// Maps are `BTreeMap` — a `HashMap` reshuffles `settings.json` on every save.
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
    pub game_paths: BTreeMap<String, String>,
    #[serde(default)]
    pub steam_launch_option_backups: BTreeMap<String, String>,
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
    pub hide_verbose_logs_warning: bool,
    #[serde(default)]
    pub thunderstore_chunk_cache_migrated: bool,
    #[serde(default)]
    pub stream_mode: bool,
    #[serde(default = "default_true")]
    pub sponsored_messages_enabled: bool,
    #[serde(default = "default_sponsor_scale")]
    pub sponsored_messages_scale: u8,
    #[serde(default = "default_sponsor_opacity")]
    pub sponsored_messages_background_opacity: u8,
    /// Emit the per-file/per-mod tracing that is otherwise suppressed.
    ///
    /// Off by default so the rotating log stays short enough to be useful in a
    /// bug report; users turn it on to capture detail for one reproduction.
    #[serde(default = "default_false")]
    pub verbose_logging: bool,
    /// Thunderstore community identifier (e.g. "lethal-company") to jump
    /// straight to on startup, skipping the game selection screen.
    #[serde(default)]
    pub default_game: Option<String>,
    /// Name of the profile under `default_game` to open automatically,
    /// skipping profile selection too. Meaningless without `default_game` set.
    #[serde(default)]
    pub default_profile: Option<String>,
    /// File name of the theme in `<app-data>/themes/` to paint the UI with.
    /// `None` means the stock palette, which is why it is also the default:
    /// an untouched install looks exactly as it did before themes existed.
    #[serde(default)]
    pub active_theme: Option<String>,
    /// Keyboard shortcuts the user changed, as action id → combination.
    ///
    /// Only the overrides are stored; the defaults live in the frontend
    /// (`src/utils/keybinds.ts`). A map rather than a field per action, so a
    /// shortcut added in a later version needs no migration here — and so an
    /// action this build does not know about is carried through untouched
    /// rather than dropped on the next save.
    #[serde(default)]
    pub keybinds: BTreeMap<String, String>,
}

impl Settings {
    pub fn default() -> Self {
        Self {
            steam_path: None,
            windows_steam_path: None,
            mac_steam_path: None,
            favorite_games: Vec::new(),
            game_paths: BTreeMap::new(),
            steam_launch_option_backups: BTreeMap::new(),
            legacy_install_mode: false,
            ask_version_before_install: false,
            install_in_parallel: true,
            confirm_before_apply_to_game: false,
            write_debug_logs_to_game: false,
            default_mod_view_mode: default_mod_view_mode(),
            show_deprecated_warnings: true,
            hide_crossover_guide: false,
            hide_macos_guide: false,
            hide_verbose_logs_warning: false,
            thunderstore_chunk_cache_migrated: false,
            stream_mode: false,
            sponsored_messages_enabled: true,
            sponsored_messages_scale: default_sponsor_scale(),
            sponsored_messages_background_opacity: default_sponsor_opacity(),
            verbose_logging: false,
            default_game: None,
            default_profile: None,
            active_theme: None,
            keybinds: BTreeMap::new(),
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

fn default_sponsor_scale() -> u8 {
    80
}

fn default_sponsor_opacity() -> u8 {
    80
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
    crate::utils::stable_json::write_file(&path, settings)
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
    dirs::data_dir().map(|dir| dir.join("Balatro").join("Mods"))
}

pub fn is_outerwilds_identifier(game_identifier: &str) -> bool {
    normalize_for_matching(game_identifier) == "outerwilds"
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

/// Walks up from `path` to the Wine/CrossOver prefix root that contains it
/// (the directory holding `drive_c`), if any.
pub fn find_wine_prefix_root(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors() {
        let is_drive_c = ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("drive_c"))
            .unwrap_or(false);
        if is_drive_c {
            return ancestor.parent().map(|parent| parent.to_path_buf());
        }
    }
    None
}

/// Translates a Windows path as seen from inside a prefix (`Z:\Users\me\Games`)
/// into the host path it actually points at.
///
/// A Steam client running inside a bottle records its library folders in
/// Windows form, and those libraries very often live outside the bottle — on a
/// drive letter that `dosdevices` symlinks back to a normal macOS directory.
/// Without this translation those libraries are invisible to us, and a game
/// installed in one looks like it has no Steam behind it at all.
pub fn map_wine_path_to_native_path(prefix_root: &Path, windows_path: &str) -> Option<PathBuf> {
    let trimmed = windows_path.trim();
    let mut chars = trimmed.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }

    let dosdevices = prefix_root.join("dosdevices");
    let mut base = [
        dosdevices.join(format!("{}:", drive.to_ascii_lowercase())),
        dosdevices.join(format!("{}:", drive.to_ascii_uppercase())),
    ]
    .into_iter()
    .find_map(|link| std::fs::canonicalize(&link).ok());

    if base.is_none() && drive.eq_ignore_ascii_case(&'c') {
        let drive_c = prefix_root.join("drive_c");
        if drive_c.is_dir() {
            base = Some(drive_c);
        }
    }

    let mut resolved = base?;
    // The .vdf escapes separators, so a single segment can arrive as `\\`.
    // Splitting on both separators and dropping empties normalises that.
    for segment in trimmed[2..].split(['\\', '/']) {
        match segment {
            "" | "." => continue,
            ".." => return None,
            segment => resolved.push(segment),
        }
    }
    Some(resolved)
}

pub fn get_steam_library_folders(steam_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut folders = vec![steam_path.to_path_buf()];
    let library_folders_path = steam_path.join("steamapps").join("libraryfolders.vdf");
    if library_folders_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&library_folders_path) {
            let prefix_root = find_wine_prefix_root(steam_path);
            let re = regex::Regex::new(r#""path"\s+"([^"]+)""#).unwrap();
            for cap in re.captures_iter(&content) {
                let raw = &cap[1];
                let folder = std::path::PathBuf::from(raw);
                let resolved = if folder.exists() {
                    Some(folder)
                } else {
                    prefix_root
                        .as_deref()
                        .and_then(|prefix| map_wine_path_to_native_path(prefix, raw))
                        .filter(|path| path.exists())
                };

                if let Some(resolved) = resolved {
                    if !folders.contains(&resolved) {
                        folders.push(resolved);
                    }
                }
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

#[cfg(test)]
mod bottle_steam_library_tests {
    use super::{get_steam_library_folders, map_wine_path_to_native_path};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A CrossOver-shaped bottle whose `Z:` maps to the host filesystem root.
    fn fake_bottle(root: &std::path::Path) -> std::path::PathBuf {
        let prefix_root = root.join("Bottles").join("Steam");
        std::fs::create_dir_all(prefix_root.join("drive_c")).unwrap();
        std::fs::create_dir_all(prefix_root.join("dosdevices")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("../drive_c", prefix_root.join("dosdevices/c:")).unwrap();
            std::os::unix::fs::symlink("/", prefix_root.join("dosdevices/z:")).unwrap();
        }
        prefix_root
    }

    #[cfg(unix)]
    #[test]
    fn z_drive_paths_resolve_back_to_the_host_filesystem() {
        let root = temp_root("winepath");
        let prefix_root = fake_bottle(&root);
        let library = root.join("WindowsSteam");
        std::fs::create_dir_all(&library).unwrap();

        let windows_path = format!("Z:{}", library.to_string_lossy().replace('/', "\\"));
        let resolved = map_wine_path_to_native_path(&prefix_root, &windows_path)
            .expect("Z: path must resolve through dosdevices");

        assert_eq!(
            std::fs::canonicalize(resolved).unwrap(),
            std::fs::canonicalize(&library).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn c_drive_paths_resolve_inside_the_prefix() {
        let root = temp_root("winepathc");
        let prefix_root = fake_bottle(&root);
        let steam_dir = prefix_root.join("drive_c/Program Files (x86)/Steam");
        std::fs::create_dir_all(&steam_dir).unwrap();

        let resolved =
            map_wine_path_to_native_path(&prefix_root, "C:\\Program Files (x86)\\Steam").unwrap();

        assert_eq!(
            std::fs::canonicalize(resolved).unwrap(),
            std::fs::canonicalize(&steam_dir).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn secondary_library_outside_the_bottle_is_discovered() {
        // The layout from issue #25: Steam lives in a CrossOver bottle while the
        // games live in a plain folder in the user's home, which the bottle sees
        // as a Z: path. Without translation that library is invisible and the
        // game looks like it has no Steam behind it.
        let root = temp_root("libfolders");
        let prefix_root = fake_bottle(&root);
        let steam_root = prefix_root.join("drive_c/Program Files (x86)/Steam");
        std::fs::create_dir_all(steam_root.join("steamapps")).unwrap();

        let library = root.join("WindowsSteam");
        std::fs::create_dir_all(library.join("steamapps/common")).unwrap();

        let windows_library = library.to_string_lossy().replace('/', "\\");
        std::fs::write(
            steam_root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"Z:{}\"\n\t}}\n}}",
                windows_library.replace('\\', "\\\\")
            ),
        )
        .unwrap();

        let folders = get_steam_library_folders(&steam_root);
        let canonical_library = std::fs::canonicalize(&library).unwrap();
        assert!(
            folders
                .iter()
                .any(|folder| std::fs::canonicalize(folder).ok() == Some(canonical_library.clone())),
            "expected the Z: library in {:?}",
            folders
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod default_game_and_profile_settings_tests {
    use super::Settings;

    #[test]
    fn default_game_and_default_profile_round_trip_through_json() {
        let mut settings = Settings::default();
        settings.default_game = Some("lethal-company".to_string());
        settings.default_profile = Some("MyModProfile".to_string());

        let json = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.default_game.as_deref(), Some("lethal-company"));
        assert_eq!(restored.default_profile.as_deref(), Some("MyModProfile"));
    }

    #[test]
    fn settings_missing_both_fields_default_to_none() {
        // Simulates a settings.json written before this feature existed.
        let json = r#"{"steam_path": null, "favorite_games": [], "game_paths": {}}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.default_game, None);
        assert_eq!(settings.default_profile, None);
    }
}

#[cfg(test)]
mod settings_keybind_tests {
    use super::Settings;

    #[test]
    fn settings_written_before_shortcuts_existed_still_load() {
        // Every field is optional on read, so an older settings.json must not
        // fail to parse and reset the user's whole configuration.
        let older = r#"{"steam_path":null,"game_paths":{}}"#;
        let settings: Settings = serde_json::from_str(older).unwrap();
        assert!(settings.keybinds.is_empty());
    }

    #[test]
    fn a_shortcut_for_an_action_this_build_does_not_know_is_kept() {
        // The map is carried verbatim, so downgrading and saving again does not
        // quietly discard a shortcut a newer version had bound.
        let stored = r#"{"steam_path":null,"game_paths":{},
            "keybinds":{"launch-modded":"Mod+Shift+L","from-the-future":"Mod+K"}}"#;
        let settings: Settings = serde_json::from_str(stored).unwrap();

        let round_tripped: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();

        assert_eq!(
            round_tripped
                .keybinds
                .get("launch-modded")
                .map(String::as_str),
            Some("Mod+Shift+L")
        );
        assert_eq!(
            round_tripped
                .keybinds
                .get("from-the-future")
                .map(String::as_str),
            Some("Mod+K")
        );
    }
}

#[cfg(test)]
mod settings_stable_order_tests {
    use super::Settings;
    use crate::utils::stable_json::to_pretty_string;

    fn populated(games: &[(&str, &str)], binds: &[(&str, &str)]) -> Settings {
        let mut settings = Settings::default();
        for (game, path) in games {
            settings
                .game_paths
                .insert((*game).to_string(), (*path).to_string());
            settings
                .steam_launch_option_backups
                .insert((*game).to_string(), format!("-applaunch {}", path));
        }
        for (action, combination) in binds {
            settings
                .keybinds
                .insert((*action).to_string(), (*combination).to_string());
        }
        settings
    }

    #[test]
    fn the_same_settings_serialize_to_the_same_bytes_whatever_order_they_were_built_in() {
        let games = [
            ("lethal-company", "/Games/Lethal Company"),
            ("balatro", "/Games/Balatro"),
            ("outer-wilds", "/Games/Outer Wilds"),
        ];
        let binds = [
            ("launch-modded", "Mod+Shift+L"),
            ("open-preferences", "Mod+,"),
            ("focus-search", "Mod+F"),
        ];

        let mut reversed_games = games;
        reversed_games.reverse();
        let mut reversed_binds = binds;
        reversed_binds.reverse();

        assert_eq!(
            to_pretty_string(&populated(&games, &binds)).unwrap(),
            to_pretty_string(&populated(&reversed_games, &reversed_binds)).unwrap()
        );
    }

    #[test]
    fn changing_one_value_changes_one_line() {
        let before = to_pretty_string(&Settings::default()).unwrap();
        let mut settings = Settings::default();
        settings.stream_mode = true;
        let after = to_pretty_string(&settings).unwrap();

        let changed = before
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(before.lines().count(), after.lines().count());
        assert_eq!(
            changed, 1,
            "expected a one-line diff, got {} lines",
            changed
        );
    }

    #[test]
    fn the_file_ends_with_a_newline() {
        assert!(to_pretty_string(&Settings::default())
            .unwrap()
            .ends_with("\n"));
    }
}
