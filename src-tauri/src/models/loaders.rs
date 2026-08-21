//! Which mod loader a Thunderstore community actually uses.
//!
//! Not every game is a BepInEx game. Hades II runs on ReturnOfModding (through
//! the Hell2Modding pack), Balatro on Lovely, Outer Wilds on OWML, and a long
//! tail of communities on loaders this app does not install at all. Assuming
//! BepInEx everywhere is what produced "BepInEx runtime missing" followed by
//! "No compatible bepinex loader was found for this community" on Hades II
//! (issue #38) - a repair that could never succeed, because the community has
//! no BepInEx pack to find.
//!
//! The verdict comes from the Thunderstore ecosystem schema, which states the
//! loader per community (`games[*].r2modman[*].packageLoader`) and the packages
//! that ship each loader (`modloaderPackages`). A snapshot of it is embedded at
//! build time (regenerate with `node scripts/generate-loader-map.mjs`) and is
//! refreshed at runtime into the app data directory, so a community added after
//! release resolves correctly without shipping a new build.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;

const EMBEDDED_LOADER_MAP: &str = include_str!("loader_map.json");
const LOADER_MAP_URL: &str = "https://thunderstore.io/api/experimental/schema/dev/latest/";
pub const LOADER_MAP_CACHE_FILE: &str = "loader_map.json";

/// The proxy DLLs a ReturnOfModding pack can install next to the game exe.
///
/// The name is not fixed across packs: ReturnOfModding ships
/// `ReturnOfModdingPack/version.dll` while Hell2Modding (Hades II) ships
/// `ReturnOfModdingPack/d3d12.dll`. Anything installed from the pack root that
/// is a DLL counts as the loader, so this list only has to catch installs whose
/// ownership manifest is gone (pre-existing installs, manual copies).
pub const RETURN_OF_MODDING_PROXY_NAMES: [&str; 6] = [
    "version.dll",
    "d3d12.dll",
    "d3d11.dll",
    "dinput8.dll",
    "winmm.dll",
    "winhttp.dll",
];

#[derive(Debug, Clone, Deserialize)]
struct LoaderPackage {
    #[serde(rename = "packageId")]
    package_id: String,
    #[allow(dead_code)]
    #[serde(default, rename = "rootFolder")]
    root_folder: String,
}

/// What a shimloader community adds to the loader facts.
///
/// `data_folder` is the game's Unreal data folder (`Pal`, `VotV`, ...), which
/// is where the loader's proxy DLL has to sit; `pak_extensions` is the
/// `shimloader/pak` rule's `defaultFileExtensions`, which only Astroneer
/// declares and which decides where a loose `.pak` at a package root lands.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShimloaderGame {
    #[serde(default, rename = "dataFolder")]
    pub data_folder: String,
    #[serde(default, rename = "pakExtensions")]
    pub pak_extensions: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LoaderMap {
    #[serde(default)]
    communities: HashMap<String, String>,
    #[serde(default, rename = "loaderPackages")]
    loader_packages: HashMap<String, Vec<LoaderPackage>>,
    #[serde(default, rename = "shimloaderGames")]
    shimloader_games: HashMap<String, ShimloaderGame>,
}

/// The loaders the ecosystem schema can name.
///
/// `BepInEx` covers the BepInEx family (including BepisLoader, which installs a
/// BepInEx layout). Everything this app cannot install is kept as
/// `Unsupported(slug)` so the UI can say which loader the game needs instead of
/// silently falling back to a BepInEx repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageLoader {
    BepInEx,
    ReturnOfModding,
    Lovely,
    Owml,
    Shimloader,
    Unsupported(String),
}

impl PackageLoader {
    fn from_slug(slug: &str) -> Self {
        // Normalised, so `return-of-modding` and `return_of_modding` land on
        // the same loader instead of one of them reading as unsupported.
        match normalize_slug(slug).as_str() {
            "bepinex" | "bepisloader" => PackageLoader::BepInEx,
            "returnofmodding" => PackageLoader::ReturnOfModding,
            "lovely" => PackageLoader::Lovely,
            "owml" => PackageLoader::Owml,
            "shimloader" => PackageLoader::Shimloader,
            _ => PackageLoader::Unsupported(slug.to_string()),
        }
    }

    /// The runtime name reported to the frontend.
    pub fn runtime_name(&self) -> &str {
        match self {
            PackageLoader::BepInEx => "bepinex",
            PackageLoader::ReturnOfModding => "returnofmodding",
            PackageLoader::Lovely => "lovely",
            PackageLoader::Owml => "owml",
            PackageLoader::Shimloader => "shimloader",
            PackageLoader::Unsupported(slug) => slug,
        }
    }

    /// Whether r2modmac knows how to install and repair this loader.
    pub fn is_supported(&self) -> bool {
        !matches!(self, PackageLoader::Unsupported(_))
    }
}

static RUNTIME_MAP: OnceLock<RwLock<Option<LoaderMap>>> = OnceLock::new();

fn embedded_map() -> &'static LoaderMap {
    static EMBEDDED: OnceLock<LoaderMap> = OnceLock::new();
    EMBEDDED.get_or_init(|| {
        serde_json::from_str(EMBEDDED_LOADER_MAP).unwrap_or_else(|error| {
            log::error!("[loaders] Embedded loader map is unreadable: {error}");
            LoaderMap::default()
        })
    })
}

fn runtime_map() -> &'static RwLock<Option<LoaderMap>> {
    RUNTIME_MAP.get_or_init(|| RwLock::new(None))
}

/// Look a value up in the refreshed map first, then the embedded snapshot.
fn with_maps<T>(lookup: impl Fn(&LoaderMap) -> Option<T>) -> Option<T> {
    if let Ok(guard) = runtime_map().read() {
        if let Some(map) = guard.as_ref() {
            if let Some(value) = lookup(map) {
                return Some(value);
            }
        }
    }
    lookup(embedded_map())
}

/// The community slug as the ecosystem schema spells it.
///
/// Profiles store the community identifier they were created with, which is the
/// same slug the package index uses (`hades-ii`), but older profiles and manual
/// entries can differ in case or separators.
fn community_key(game_identifier: &str) -> String {
    game_identifier.trim().to_lowercase()
}

fn normalized_community_key(game_identifier: &str) -> String {
    crate::models::shared::normalize_for_matching(game_identifier)
}

/// The loader the ecosystem declares for a community, if it declares one.
///
/// Returns `None` for communities the schema does not describe (a handful have
/// no `r2modman` entry) - callers fall back to what is on disk.
pub fn loader_for_community(game_identifier: &str) -> Option<PackageLoader> {
    let key = community_key(game_identifier);
    let normalized = normalized_community_key(game_identifier);
    with_maps(|map| {
        map.communities
            .get(&key)
            .or_else(|| {
                map.communities
                    .iter()
                    .find(|(slug, _)| normalize_slug(slug) == normalized)
                    .map(|(_, loader)| loader)
            })
            .map(|slug| PackageLoader::from_slug(slug))
    })
}

fn normalize_slug(slug: &str) -> String {
    crate::models::shared::normalize_for_matching(slug)
}

/// The `Author-Package` ids that ship a given loader, newest schema first.
pub fn loader_package_ids(loader: &PackageLoader) -> Vec<String> {
    let slugs: &[&str] = match loader {
        PackageLoader::BepInEx => &["bepinex", "bepisloader"],
        PackageLoader::ReturnOfModding => &["return-of-modding"],
        PackageLoader::Lovely => &["lovely"],
        PackageLoader::Owml => &["owml"],
        PackageLoader::Shimloader => &["shimloader"],
        PackageLoader::Unsupported(slug) => return collect_package_ids(&[slug.as_str()]),
    };
    collect_package_ids(slugs)
}

fn collect_package_ids(slugs: &[&str]) -> Vec<String> {
    let mut ids = Vec::new();
    for slug in slugs {
        if let Some(packages) = with_maps(|map| map.loader_packages.get(*slug).cloned()) {
            ids.extend(packages.into_iter().map(|package| package.package_id));
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Whether `full_name` is the loader package itself rather than a mod.
///
/// `full_name` is a Thunderstore full name, optionally version-suffixed
/// (`Hell2Modding-Hell2Modding-1.0.110`), so the comparison is on the
/// `Author-Package` prefix.
pub fn is_loader_package(loader: &PackageLoader, full_name: &str) -> bool {
    let key = package_key(full_name);
    loader_package_ids(loader)
        .iter()
        .any(|id| id.to_lowercase() == key)
}

fn package_key(full_name: &str) -> String {
    let parts: Vec<&str> = full_name.split('-').collect();
    if parts.len() >= 2 {
        format!("{}-{}", parts[0], parts[1]).to_lowercase()
    } else {
        full_name.to_lowercase()
    }
}

/// The loader in use for a profile, resolved from the community and then from
/// what the game folder actually holds.
///
/// The disk check is what keeps hand-configured game paths and communities the
/// schema does not cover working: a `ReturnOfModding` folder or an installed
/// pack proxy is proof of a ReturnOfModding game regardless of the identifier.
pub fn resolve_loader(game_identifier: &str, game_path: &Path) -> PackageLoader {
    if crate::models::shared::is_outerwilds_identifier(game_identifier)
        || crate::models::shared::is_outerwilds_game_path(game_path)
    {
        return PackageLoader::Owml;
    }
    if let Some(loader) = loader_for_community(game_identifier) {
        return loader;
    }
    if has_return_of_modding_layout(game_path) {
        log::debug!(
            "[loaders] {} is not in the loader map; game folder {:?} holds a ReturnOfModding layout",
            game_identifier,
            game_path
        );
        return PackageLoader::ReturnOfModding;
    }
    log::debug!(
        "[loaders] No loader declared for {}; assuming BepInEx",
        game_identifier
    );
    PackageLoader::BepInEx
}

/// Whether the game folder holds a ReturnOfModding install.
pub fn has_return_of_modding_layout(game_path: &Path) -> bool {
    game_path.join("ReturnOfModding").is_dir() || !return_of_modding_proxies(game_path).is_empty()
}

/// The ReturnOfModding proxy DLLs present in the game folder, active or
/// disabled, in the order they were found.
pub fn return_of_modding_proxies(game_path: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for name in RETURN_OF_MODDING_PROXY_NAMES {
        let active = game_path.join(name);
        if active.is_file() {
            found.push(active);
            continue;
        }
        let disabled = game_path.join(format!("{name}_DISABLED"));
        if disabled.is_file() {
            found.push(disabled);
        }
    }
    found
}

/// The `WINEDLLOVERRIDES` value a Wine/CrossOver launch needs, if any.
///
/// A loader that hooks the game through a proxy DLL only gets loaded when Wine
/// is told to prefer the native file over its own builtin. The proxy is named
/// per pack - `version.dll` for ReturnOfModding, `d3d12.dll` for Hell2Modding on
/// Hades II, `winhttp.dll` for a BepInEx doorstop - so the value is built from
/// the proxies actually sitting next to the game.
pub fn wine_dll_override_value(game_path: &Path) -> Option<String> {
    let overrides = RETURN_OF_MODDING_PROXY_NAMES
        .iter()
        .filter(|name| game_path.join(name).is_file())
        .map(|name| format!("{}=n,b", name.trim_end_matches(".dll")))
        .collect::<Vec<_>>();
    (!overrides.is_empty()).then(|| overrides.join(";"))
}

/// The files the shimloader package leaves in the profile root.
///
/// They are the only part of a shimloader install that has to reach the game
/// folder: `dwmapi.dll` is the shim the game loads as a proxy, and it in turn
/// loads `ue4ss.dll` beside it, configured by `UE4SS-settings.ini`. Everything
/// else (mods, paks, configs) stays in the profile and is handed to the shim on
/// the command line.
pub const SHIMLOADER_RUNTIME_FILES: [&str; 3] = ["dwmapi.dll", "ue4ss.dll", "UE4SS-settings.ini"];

/// The shimloader facts the schema states for a community, if it is one.
pub fn shimloader_game(game_identifier: &str) -> Option<ShimloaderGame> {
    let key = community_key(game_identifier);
    let normalized = normalized_community_key(game_identifier);
    with_maps(|map| {
        map.shimloader_games
            .get(&key)
            .or_else(|| {
                map.shimloader_games
                    .iter()
                    .find(|(slug, _)| normalize_slug(slug) == normalized)
                    .map(|(_, game)| game)
            })
            .cloned()
    })
}

/// True when the community runs on shimloader.
///
/// Unlike ReturnOfModding there is no folder to fall back on: a shimloader
/// install leaves nothing in the game folder except the proxy DLL, which
/// several other loaders also use.
pub fn uses_shimloader(game_identifier: &str) -> bool {
    loader_for_community(game_identifier) == Some(PackageLoader::Shimloader)
}

/// Where the game loads its proxy DLL from: `<game>/<data folder>/Binaries/Win64`.
///
/// The data folder comes from the schema, but the configured game path is
/// whatever the user pointed at, so a folder that does not hold the expected
/// data folder is searched one level deep for the `Binaries/Win64` layout
/// rather than being given up on.
pub fn shimloader_binaries_dir(game_path: &Path, data_folder: &str) -> Option<std::path::PathBuf> {
    let expected = game_path.join(data_folder).join("Binaries").join("Win64");
    if !data_folder.is_empty() && expected.is_dir() {
        return Some(expected);
    }
    let entries = std::fs::read_dir(game_path).ok()?;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let candidate = entry.path().join("Binaries").join("Win64");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    (!data_folder.is_empty()).then_some(expected)
}

/// True when the community (or the folder) runs on ReturnOfModding.
pub fn uses_return_of_modding(game_identifier: &str, game_path: &Path) -> bool {
    resolve_loader(game_identifier, game_path) == PackageLoader::ReturnOfModding
}

/// Replace the in-memory map with a freshly fetched ecosystem schema.
pub fn install_refreshed_map(raw_schema: &str) -> Result<usize, String> {
    let map = parse_ecosystem_schema(raw_schema)?;
    let count = map.communities.len();
    if let Ok(mut guard) = runtime_map().write() {
        *guard = Some(map);
    }
    Ok(count)
}

/// Reduce the full ecosystem schema (over a megabyte) to the loader facts.
fn parse_ecosystem_schema(raw: &str) -> Result<LoaderMap, String> {
    let schema: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| format!("Malformed ecosystem schema: {error}"))?;

    let mut communities = HashMap::new();
    let mut shimloader_games: HashMap<String, ShimloaderGame> = HashMap::new();
    if let Some(games) = schema.get("games").and_then(|value| value.as_object()) {
        for (game_key, game) in games {
            let Some(entries) = game.get("r2modman").and_then(|value| value.as_array()) else {
                continue;
            };
            for entry in entries {
                let Some(loader) = entry.get("packageLoader").and_then(|value| value.as_str())
                else {
                    continue;
                };
                let community = entry
                    .get("packageIndex")
                    .and_then(|value| value.as_str())
                    .and_then(community_from_package_index)
                    .unwrap_or_else(|| game_key.clone());
                if loader == "shimloader" {
                    shimloader_games.insert(community.clone(), shimloader_game_from_entry(entry));
                }
                communities.insert(community, loader.to_string());
            }
        }
    }

    let mut loader_packages: HashMap<String, Vec<LoaderPackage>> = HashMap::new();
    if let Some(packages) = schema
        .get("modloaderPackages")
        .and_then(|value| value.as_array())
    {
        for package in packages {
            let (Some(loader), Some(package_id)) = (
                package.get("loader").and_then(|value| value.as_str()),
                package.get("packageId").and_then(|value| value.as_str()),
            ) else {
                continue;
            };
            loader_packages
                .entry(loader.to_string())
                .or_default()
                .push(LoaderPackage {
                    package_id: package_id.to_string(),
                    root_folder: package
                        .get("rootFolder")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                });
        }
    }

    if communities.is_empty() {
        return Err("Ecosystem schema declared no communities".to_string());
    }

    Ok(LoaderMap {
        communities,
        loader_packages,
        shimloader_games,
    })
}

fn shimloader_game_from_entry(entry: &serde_json::Value) -> ShimloaderGame {
    let pak_extensions = entry
        .get("installRules")
        .and_then(|value| value.as_array())
        .and_then(|rules| {
            rules.iter().find(|rule| {
                rule.get("route").and_then(|value| value.as_str()) == Some("shimloader/pak")
            })
        })
        .and_then(|rule| rule.get("defaultFileExtensions"))
        .and_then(|value| value.as_array())
        .map(|extensions| {
            extensions
                .iter()
                .filter_map(|value| value.as_str().map(|value| value.to_string()))
                .collect()
        })
        .unwrap_or_default();

    ShimloaderGame {
        data_folder: entry
            .get("dataFolderName")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        pak_extensions,
    }
}

fn community_from_package_index(package_index: &str) -> Option<String> {
    let (_, rest) = package_index.split_once("/c/")?;
    let (community, _) = rest.split_once('/')?;
    (!community.is_empty()).then(|| community.to_lowercase())
}

/// Load the cached schema written by a previous refresh, if any.
pub fn load_cached_map(app: &tauri::AppHandle) -> bool {
    let Ok(dir) = crate::utils::paths::app_data_dir(app) else {
        return false;
    };
    let Ok(raw) = std::fs::read_to_string(dir.join(LOADER_MAP_CACHE_FILE)) else {
        return false;
    };
    match install_refreshed_map(&raw) {
        Ok(count) => {
            log::debug!("[loaders] Loaded cached loader map ({count} communities)");
            true
        }
        Err(error) => {
            log::warn!("[loaders] Ignoring cached loader map: {error}");
            false
        }
    }
}

/// Fetch the ecosystem schema and cache it for the next start.
///
/// Failure is not an error the user needs to see: the embedded snapshot still
/// answers for every community that existed at build time.
pub async fn refresh_loader_map(app: tauri::AppHandle) {
    let response = match reqwest::Client::new().get(LOADER_MAP_URL).send().await {
        Ok(response) => response,
        Err(error) => {
            log::debug!("[loaders] Loader map refresh skipped: {error}");
            return;
        }
    };
    if !response.status().is_success() {
        log::debug!(
            "[loaders] Loader map refresh skipped: HTTP {}",
            response.status()
        );
        return;
    }
    let raw = match response.text().await {
        Ok(raw) => raw,
        Err(error) => {
            log::debug!("[loaders] Loader map refresh unreadable: {error}");
            return;
        }
    };
    match install_refreshed_map(&raw) {
        Ok(count) => {
            log::info!("[loaders] Refreshed loader map ({count} communities)");
            if let Ok(dir) = crate::utils::paths::app_data_dir(&app) {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(dir.join(LOADER_MAP_CACHE_FILE), &raw);
            }
        }
        Err(error) => log::warn!("[loaders] Loader map refresh rejected: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_map_names_the_loader_for_known_communities() {
        assert_eq!(
            loader_for_community("hades-ii"),
            Some(PackageLoader::ReturnOfModding)
        );
        assert_eq!(
            loader_for_community("risk-of-rain-returns"),
            Some(PackageLoader::ReturnOfModding)
        );
        assert_eq!(loader_for_community("balatro"), Some(PackageLoader::Lovely));
        assert_eq!(
            loader_for_community("lethal-company"),
            Some(PackageLoader::BepInEx)
        );
    }

    #[test]
    fn community_lookup_tolerates_identifier_spelling() {
        assert_eq!(
            loader_for_community("Hades-II"),
            Some(PackageLoader::ReturnOfModding)
        );
        assert_eq!(
            loader_for_community("hadesii"),
            Some(PackageLoader::ReturnOfModding)
        );
    }

    #[test]
    fn hades_ii_loader_package_is_recognised() {
        let loader = PackageLoader::ReturnOfModding;
        let ids = loader_package_ids(&loader);
        assert!(ids.iter().any(|id| id == "Hell2Modding-Hell2Modding"));
        assert!(ids.iter().any(|id| id == "ReturnOfModding-ReturnOfModding"));
        assert!(is_loader_package(
            &loader,
            "Hell2Modding-Hell2Modding-1.0.110"
        ));
        assert!(is_loader_package(
            &loader,
            "ReturnOfModding-ReturnOfModding-1.1.30"
        ));
        assert!(!is_loader_package(&loader, "LuaENVY-ENVY-1.0.0"));
    }

    #[test]
    fn shimloader_communities_carry_their_data_folder() {
        assert_eq!(
            loader_for_community("palworld"),
            Some(PackageLoader::Shimloader)
        );
        assert!(uses_shimloader("voices-of-the-void"));
        assert!(!uses_shimloader("hades-ii"));

        let palworld = shimloader_game("palworld").unwrap();
        assert_eq!(palworld.data_folder, "Pal");
        assert!(palworld.pak_extensions.is_empty());

        // Astroneer is the one community that routes loose .pak files by
        // extension; everything else leaves them in the mod folder.
        let astroneer = shimloader_game("astroneer").unwrap();
        assert_eq!(astroneer.data_folder, "Astro");
        assert_eq!(astroneer.pak_extensions, vec![".pak".to_string()]);

        assert!(shimloader_game("hades-ii").is_none());
        assert!(loader_package_ids(&PackageLoader::Shimloader)
            .iter()
            .any(|id| id == "Thunderstore-unreal_shimloader"));
        assert!(is_loader_package(
            &PackageLoader::Shimloader,
            "Thunderstore-unreal_shimloader-1.1.7"
        ));
    }

    #[test]
    fn the_binaries_dir_is_found_even_when_the_data_folder_is_not_where_it_should_be() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-shimloader-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let expected = root.join("Pal").join("Binaries").join("Win64");
        std::fs::create_dir_all(&expected).unwrap();
        assert_eq!(shimloader_binaries_dir(&root, "Pal"), Some(expected));

        // A game folder whose data folder is spelled differently than the
        // schema says still has exactly one Binaries/Win64 to find.
        let other = root.join("other");
        let actual = other.join("Panicore").join("Binaries").join("Win64");
        std::fs::create_dir_all(&actual).unwrap();
        assert_eq!(shimloader_binaries_dir(&other, "Panicore2"), Some(actual));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unsupported_loaders_keep_their_name() {
        let loader = PackageLoader::from_slug("melonloader");
        assert!(!loader.is_supported());
        assert_eq!(loader.runtime_name(), "melonloader");
    }

    #[test]
    fn hell2modding_proxy_counts_as_a_return_of_modding_install() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-loaders-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(!has_return_of_modding_layout(&root));
        // Hell2Modding ships d3d12.dll, not the version.dll ReturnOfModding uses.
        std::fs::write(root.join("d3d12.dll"), b"loader").unwrap();
        assert!(has_return_of_modding_layout(&root));
        assert_eq!(return_of_modding_proxies(&root).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn wine_override_names_the_proxy_that_is_actually_installed() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-loaders-wine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(wine_dll_override_value(&root), None);
        std::fs::write(root.join("d3d12.dll"), b"loader").unwrap();
        assert_eq!(wine_dll_override_value(&root).as_deref(), Some("d3d12=n,b"));
        // A parked loader is not loaded: vanilla mode must not override it.
        std::fs::rename(root.join("d3d12.dll"), root.join("d3d12.dll_DISABLED")).unwrap();
        assert_eq!(wine_dll_override_value(&root), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn schema_parsing_maps_community_from_package_index() {
        let raw = r#"{
            "games": {
                "some-game": {
                    "r2modman": [{
                        "packageIndex": "https://thunderstore.io/c/some-community/api/v1/package-listing-index/",
                        "packageLoader": "return-of-modding"
                    }]
                }
            },
            "modloaderPackages": [
                {"packageId": "Author-Loader", "rootFolder": "Pack", "loader": "return-of-modding"}
            ]
        }"#;
        let map = parse_ecosystem_schema(raw).unwrap();
        assert_eq!(
            map.communities.get("some-community").map(String::as_str),
            Some("return-of-modding")
        );
        assert_eq!(map.loader_packages["return-of-modding"].len(), 1);
    }
}

#[cfg(test)]
mod refreshed_map_tests {
    use super::*;

    /// A community added to Thunderstore after this build still resolves once
    /// the schema is refreshed.
    #[test]
    fn a_community_missing_from_the_build_resolves_after_a_refresh() {
        let community = "a-game-released-after-this-build";
        assert!(loader_for_community(community).is_none());

        let schema = format!(
            r#"{{"games":{{"newgame":{{"r2modman":[{{"packageLoader":"return_of_modding",
               "packageIndex":"https://thunderstore.io/c/{community}/api/v1/package/"}}]}}}}}}"#
        );
        install_refreshed_map(&schema).unwrap();

        assert_eq!(
            loader_for_community(community).map(|loader| loader.runtime_name().to_string()),
            Some("returnofmodding".to_string())
        );

        // The build's own entries survive the refresh.
        assert_eq!(
            loader_for_community("valheim").map(|loader| loader.runtime_name().to_string()),
            Some("bepinex".to_string())
        );

        if let Ok(mut guard) = runtime_map().write() {
            *guard = None;
        }
    }
}
