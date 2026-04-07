use std::{
    collections::{HashSet, VecDeque},
    fs,
    sync::{Mutex, OnceLock},
};
use tauri::{command, AppHandle, Emitter, Manager};
use crate::models::shared::*;
use crate::utils::file_ops::*;
use crate::utils::mod_manifest::{
    cleanup_owned_mod_manifests,
    load_owned_mod_manifests,
    manifest_matches_target_root,
    GAME_MANIFEST_SCOPE,
};
use crate::commands::mod_commands::{
    detect_unity_runtime_kind,
    download_official_macos_bepinex_runtime,
    extract_bepinex_pack_to_root,
    extract_version_number_from_full_name,
    normalize_macos_doorstop_config_file,
};

const CANONICAL_MAC_BEPINEX_SCRIPT: &str = "run_bepinex.sh";
const BALATRO_LOVELY_SCRIPT: &str = "run_lovely_macos.sh";
const STEAM_LAUNCH_OPTIONS_RESTART_EVENT: &str = "steam-launch-options-restart";
static LOGGED_GAME_PATH_OVERRIDES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn is_bepinex_shell_script_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".sh") && lower.contains("bepinex")
}

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

fn log_manual_override_once(key: &str, path: &str) {
    let dedupe_key = format!("{}::{}", key, path);
    let seen = LOGGED_GAME_PATH_OVERRIDES.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut seen) = seen.lock() else {
        eprintln!("[get_game_path] Found manual override (key={}): {}", key, path);
        return;
    };

    if seen.insert(dedupe_key) {
        eprintln!("[get_game_path] Found manual override (key={}): {}", key, path);
    }
}

fn normalize_mod_match_value(input: &str) -> String {
    let stem = std::path::Path::new(input)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());

    stem.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn derive_mod_match_terms(input: &str) -> Vec<String> {
    let stem = std::path::Path::new(input)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());
    let parts: Vec<&str> = stem.split('-').filter(|part| !part.is_empty()).collect();
    let mut terms = Vec::new();

    if parts.len() >= 2 {
        terms.push(normalize_mod_match_value(parts[1]));
        terms.push(normalize_mod_match_value(&format!("{}-{}", parts[0], parts[1])));
    }

    terms.push(normalize_mod_match_value(&stem));
    terms.retain(|term| term.len() >= 3);
    terms.sort();
    terms.dedup();
    terms
}

fn is_metadata_only_plugin_file_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        ".ds_store"
            | "manifest.json"
            | "icon.png"
            | "readme"
            | "readme.md"
            | "readme.txt"
            | "changelog"
            | "changelog.md"
            | "changelog.txt"
            | "license"
            | "license.md"
            | "license.txt"
    )
}

fn path_has_plugin_payload(path: &std::path::Path, depth: usize) -> bool {
    if depth == 0 || !path.exists() {
        return false;
    }

    if path.is_file() {
        return path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| !is_metadata_only_plugin_file_name(name))
            .unwrap_or(false);
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry in entries.filter_map(|entry| entry.ok()) {
        let child_path = entry.path();
        if child_path.is_file() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !is_metadata_only_plugin_file_name(&file_name) {
                return true;
            }
        } else if child_path.is_dir() && path_has_plugin_payload(&child_path, depth - 1) {
            return true;
        }
    }

    false
}

fn entry_name_matches_mod_payload(entry_name: &str, folder_name: &str, mod_key: &str) -> bool {
    let entry_norm = normalize_mod_match_value(entry_name);
    if entry_norm.is_empty() {
        return false;
    }

    let mut terms = derive_mod_match_terms(folder_name);
    terms.extend(derive_mod_match_terms(mod_key));
    terms.sort();
    terms.dedup();

    terms
        .iter()
        .any(|term| entry_norm.contains(term) || term.contains(&entry_norm))
}

fn game_mod_folder_has_payload(
    plugins_root: &std::path::Path,
    folder_path: &std::path::Path,
    folder_name: &str,
    mod_key: &str,
) -> bool {
    if path_has_plugin_payload(folder_path, 6) {
        return true;
    }

    let entries = match fs::read_dir(plugins_root) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry in entries.filter_map(|entry| entry.ok()) {
        let sibling_path = entry.path();
        if sibling_path == folder_path {
            continue;
        }

        let sibling_name = entry.file_name().to_string_lossy().to_string();
        if !entry_name_matches_mod_payload(&sibling_name, folder_name, mod_key) {
            continue;
        }

        if path_has_plugin_payload(&sibling_path, 6) {
            return true;
        }
    }

    false
}

fn game_mod_folder_is_auxiliary_payload(
    folder_name: &str,
    folder_key: &str,
    desired_full_by_key: &std::collections::HashMap<String, String>,
) -> bool {
    desired_full_by_key.iter().any(|(desired_key, desired_full)| {
        desired_key != folder_key
            && entry_name_matches_mod_payload(folder_name, desired_full, desired_key)
    })
}

fn remove_plugin_entry(path: &std::path::Path) -> std::io::Result<()> {
    if path.is_symlink() || path.is_file() {
        fs::remove_file(path)
    } else if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        Ok(())
    }
}

fn is_stale_generated_entry_name(name: &str) -> bool {
    !matches!(
        name.to_lowercase().as_str(),
        ".ds_store" | "bepinex.cfg"
    )
}

fn cleanup_stale_generated_mod_artifacts(
    game_path: &std::path::Path,
    desired_mod_labels: &[String],
) -> Result<usize, String> {
    let keep_terms = desired_mod_labels
        .iter()
        .flat_map(|label| derive_mod_match_terms(label))
        .collect::<std::collections::HashSet<_>>();

    let known_roots = [
        game_path.join("BepInEx").join("config"),
        game_path.join("BepInEx").join("cache"),
        game_path.join("BepInEx").join("Translation"),
        game_path.join("config"),
        game_path.join("cache"),
        game_path.join("Translation"),
    ];

    let mut removed = 0usize;
    for root in known_roots {
        if !root.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_stale_generated_entry_name(&name) {
                continue;
            }

            let entry_norm = normalize_mod_match_value(&name);
            if entry_norm.is_empty() {
                continue;
            }

            let belongs_to_desired_mod = keep_terms
                .iter()
                .any(|term| entry_norm.contains(term) || term.contains(&entry_norm));
            if belongs_to_desired_mod {
                continue;
            }

            remove_plugin_entry(&entry.path()).map_err(|e| {
                format!(
                    "Failed to remove stale generated artifact {}: {}",
                    entry.path().display(),
                    e
                )
            })?;
            removed += 1;
        }
    }

    Ok(removed)
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

fn get_profile_distribution(app: &AppHandle, profile_id: &str) -> String {
    let profiles_path = app.path().app_data_dir().unwrap_or_default().join("profiles.json");
    if profiles_path.exists() {
        if let Ok(data) = fs::read_to_string(&profiles_path) {
            if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                if let Some(profile) = profiles.iter().find(|p| p["id"].as_str() == Some(profile_id)) {
                    if let Some(distribution) = profile["distribution"].as_str() {
                        if distribution == "steam" || distribution == "manual" {
                            return distribution.to_string();
                        }
                    }
                }
            }
        }
    }
    "steam".to_string()
}

fn get_profile_launch_mode(app: &AppHandle, profile_id: &str) -> String {
    let profiles_path = app.path().app_data_dir().unwrap_or_default().join("profiles.json");
    if profiles_path.exists() {
        if let Ok(data) = fs::read_to_string(&profiles_path) {
            if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                if let Some(profile) = profiles.iter().find(|p| p["id"].as_str() == Some(profile_id)) {
                    if let Some(launch_mode) = profile["launchMode"].as_str() {
                        if launch_mode == "auto" || launch_mode == "steam" || launch_mode == "direct" {
                            return launch_mode.to_string();
                        }
                    }
                }
            }
        }
    }
    "auto".to_string()
}

fn should_manage_steam_launch_options(distribution: &str, launch_mode: &str) -> bool {
    distribution == "steam" && launch_mode != "direct"
}

fn profile_prefers_direct_launch(launch_mode: &str) -> bool {
    launch_mode == "direct"
}

fn macos_steam_launch_option_is_managed(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<bool, String> {
    let app_id = find_steam_app_id_for_game_path_any(app, game_path, false)
        .ok_or_else(|| "Couldn't determine the Steam app ID for this macOS game".to_string())?;

    let steam_roots = get_steam_roots_for_platform(app, false);
    let steam_root_for_config = find_matching_steam_root_for_game_path(app, game_path, false)
        .or_else(|| {
            steam_roots
                .iter()
                .find(|root| get_latest_localconfig_path(root).is_some())
                .cloned()
        })
        .or_else(|| steam_roots.first().cloned())
        .ok_or_else(|| "No Steam installation found to inspect macOS launch options".to_string())?;

    let localconfig_paths = get_all_localconfig_paths(&steam_root_for_config);
    if localconfig_paths.is_empty() {
        return Err(
        "Couldn't locate Steam's localconfig.vdf for macOS launch option inspection.".to_string()
        );
    }

    for localconfig_path in localconfig_paths {
        let Ok(localconfig) = fs::read_to_string(&localconfig_path) else {
            continue;
        };

        if get_launch_options_for_app(&localconfig, &app_id)
            .as_deref()
            .map(|value| is_managed_macos_launch_option_for_game(value, game_path))
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn managed_macos_launch_option_for_game(
    game_path: &std::path::Path,
) -> Result<String, String> {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let script_path = canonicalize_macos_bepinex_script(&runtime_root)?;
    if should_use_native_macos_bepinex_launcher(&runtime_root) {
        Ok(format!(
            "/usr/bin/arch -arm64 /bin/bash \"{}\" %command%",
            script_path.display()
        ))
    } else {
        Ok(format!(
            "/usr/bin/arch -x86_64 /bin/bash \"{}\" %command%",
            script_path.display()
        ))
    }
}

fn macos_steam_launch_option_matches_desired(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<bool, String> {
    let app_id = find_steam_app_id_for_game_path_any(app, game_path, false)
        .ok_or_else(|| "Couldn't determine the Steam app ID for this macOS game".to_string())?;
    let desired = managed_macos_launch_option_for_game(game_path)?;

    let steam_roots = get_steam_roots_for_platform(app, false);
    let steam_root_for_config = find_matching_steam_root_for_game_path(app, game_path, false)
        .or_else(|| {
            steam_roots
                .iter()
                .find(|root| get_latest_localconfig_path(root).is_some())
                .cloned()
        })
        .or_else(|| steam_roots.first().cloned())
        .ok_or_else(|| "No Steam installation found to inspect macOS launch options".to_string())?;

    let localconfig_paths = get_all_localconfig_paths(&steam_root_for_config);
    if localconfig_paths.is_empty() {
        return Err(
            "Couldn't locate Steam's localconfig.vdf for macOS launch option inspection.".to_string()
        );
    }

    for localconfig_path in localconfig_paths {
        let Ok(localconfig) = fs::read_to_string(&localconfig_path) else {
            continue;
        };

        let Some(current) = get_launch_options_for_app(&localconfig, &app_id) else {
            continue;
        };

        if current == desired {
            return Ok(true);
        }

        if is_managed_macos_launch_option_for_game(&current, game_path) {
            eprintln!(
                "[macos_steam_launch_option_matches_desired] accepting managed non-exact launch option for app_id={} localconfig={} current={:?} desired={:?}",
                app_id,
                localconfig_path.display(),
                current,
                desired
            );
            return Ok(true);
        }
    }

    eprintln!(
        "[macos_steam_launch_option_matches_desired] no matching launch option for app_id={} game_path={} desired={:?}",
        app_id,
        game_path.display(),
        desired
    );
    Ok(false)
}

fn canonicalize_or_original(path: &std::path::Path) -> std::path::PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn game_path_matches_install_root(game_path: &std::path::Path, install_root: &std::path::Path) -> bool {
    let canonical_game = canonicalize_or_original(game_path);
    let canonical_install = canonicalize_or_original(install_root);
    canonical_game == canonical_install
        || canonical_game.starts_with(&canonical_install)
        || canonical_install.starts_with(&canonical_game)
}

fn find_steam_app_id_for_library_root(
    library_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Option<String> {
    let steamapps_dir = library_root.join("steamapps");
    if !steamapps_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(&steamapps_dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let app_id = match parse_manifest_value(&content, "appid") {
            Some(value) => value,
            None => continue,
        };
        let install_dir = match parse_manifest_value(&content, "installdir") {
            Some(value) => value,
            None => continue,
        };

        let manifest_game_path = library_root.join("steamapps").join("common").join(install_dir);
        if game_path_matches_install_root(game_path, &manifest_game_path) {
            return Some(app_id);
        }
    }

    None
}

fn find_embedded_steam_library_root_for_game_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let canonical_game = canonicalize_or_original(game_path);

    for ancestor in canonical_game.ancestors() {
        let is_common = ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("common"))
            .unwrap_or(false);
        if !is_common {
            continue;
        }

        let Some(steamapps_dir) = ancestor.parent() else {
            continue;
        };
        let is_steamapps = steamapps_dir
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("steamapps"))
            .unwrap_or(false);
        if !is_steamapps {
            continue;
        }

        let Some(library_root) = steamapps_dir.parent() else {
            continue;
        };
        if find_steam_app_id_for_library_root(library_root, &canonical_game).is_some() {
            return Some(library_root.to_path_buf());
        }
    }

    None
}

fn can_launch_via_steam_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> bool {
    let Some(steam_root) = find_matching_steam_root_for_game_path(app, game_path, is_windows_profile) else {
        return false;
    };

    find_steam_app_id_for_game_path(&steam_root, game_path).is_some()
}

fn infer_distribution_from_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> String {
    let game_path = canonicalize_or_original(game_path);

    for steam_root in get_steam_roots_for_platform(app, is_windows_profile) {
        for library_root in get_steam_library_folders(&steam_root) {
            if find_steam_app_id_for_library_root(&library_root, &game_path).is_some() {
                return "steam".to_string();
            }
        }
    }

    if find_embedded_steam_library_root_for_game_path(&game_path).is_some() {
        return "steam".to_string();
    }

    "manual".to_string()
}

fn get_steam_roots_for_platform(app: &AppHandle, is_windows_profile: bool) -> Vec<std::path::PathBuf> {
    let settings = load_settings_impl(app);
    let mut steam_paths_to_check = Vec::new();

    let expand_user_path = |raw: &str| -> std::path::PathBuf {
        if raw == "~" {
            return dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(raw));
        }

        if let Some(stripped) = raw.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }

        std::path::PathBuf::from(raw)
    };

    if !is_windows_profile {
        if let Some(home) = dirs::home_dir() {
            let mac_steam = home.join("Library/Application Support/Steam");
            if mac_steam.exists() {
                steam_paths_to_check.push(mac_steam);
            }
        }
    }

    let legacy_mac_steam_path = settings.steam_path.as_ref().filter(|path| {
        let lower = path.to_lowercase();
        !lower.contains("drive_c") && !lower.contains("crossover") && !lower.contains("wine")
    });

    let configured_steam_path = if is_windows_profile {
        settings
            .windows_steam_path
            .as_ref()
            .or(settings.steam_path.as_ref())
    } else {
        settings
            .mac_steam_path
            .as_ref()
            .or(legacy_mac_steam_path)
    };

    if let Some(steam_path_str) = configured_steam_path {
        let configured_steam = expand_user_path(steam_path_str);
        if configured_steam.exists() && !steam_paths_to_check.contains(&configured_steam) {
            steam_paths_to_check.push(configured_steam);
        }
    }

    steam_paths_to_check
}

fn parse_manifest_value(content: &str, key: &str) -> Option<String> {
    let pattern = format!(r#""{}"\s+"([^"]+)""#, regex::escape(key));
    let re = regex::Regex::new(&pattern).ok()?;
    re.captures(content)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
}

fn find_steam_app_id_for_game_path(
    steam_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Option<String> {
    for library_root in get_steam_library_folders(steam_root) {
        if let Some(app_id) = find_steam_app_id_for_library_root(&library_root, game_path) {
            return Some(app_id);
        }
    }

    None
}

fn find_steam_app_id_for_game_path_any(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> Option<String> {
    for steam_root in get_steam_roots_for_platform(app, is_windows_profile) {
        if let Some(app_id) = find_steam_app_id_for_game_path(&steam_root, game_path) {
            return Some(app_id);
        }
    }

    if let Some(library_root) = find_embedded_steam_library_root_for_game_path(game_path) {
        if let Some(app_id) = find_steam_app_id_for_library_root(&library_root, game_path) {
            return Some(app_id);
        }
    }

    None
}

fn find_matching_steam_root_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> Option<std::path::PathBuf> {
    let canonical_game = canonicalize_or_original(game_path);

    for steam_root in get_steam_roots_for_platform(app, is_windows_profile) {
        for library_root in get_steam_library_folders(&steam_root) {
            if find_steam_app_id_for_library_root(&library_root, &canonical_game).is_some() {
                return Some(steam_root);
            }
        }
    }

    None
}

fn get_latest_localconfig_path(steam_root: &std::path::Path) -> Option<std::path::PathBuf> {
    get_all_localconfig_paths(steam_root).into_iter().next()
}

fn get_all_localconfig_paths(steam_root: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn steamid64_to_accountid(user_id: &str) -> Option<String> {
        const STEAMID64_BASE: u64 = 76561197960265728;
        let parsed = user_id.parse::<u64>().ok()?;
        parsed
            .checked_sub(STEAMID64_BASE)
            .map(|account_id| account_id.to_string())
    }

    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut push_path = |candidate: std::path::PathBuf| {
        if !candidate.exists() {
            return;
        }

        let canonical = canonicalize_or_original(&candidate);
        if seen.insert(canonical) {
            paths.push(candidate);
        }
    };

    let loginusers_path = steam_root.join("config").join("loginusers.vdf");
    if loginusers_path.exists() {
        if let Ok(content) = fs::read_to_string(&loginusers_path) {
            if let Ok(block_re) = regex::Regex::new(r#"(?s)"(?P<id>\d+)"\s*\{(?P<body>.*?)\n\s*\}"#) {
                if let Ok(most_recent_re) = regex::Regex::new(r#""MostRecent"\s+"1""#) {
                    if let Ok(timestamp_re) = regex::Regex::new(r#""Timestamp"\s+"(\d+)""#) {
                        let mut candidates: Vec<(String, bool, u64)> = block_re
                            .captures_iter(&content)
                            .filter_map(|captures| {
                                let user_id = captures.name("id")?.as_str().to_string();
                                let body = captures.name("body")?.as_str();
                                let most_recent = most_recent_re.is_match(body);
                                let timestamp = timestamp_re
                                    .captures(body)
                                    .and_then(|m| m.get(1))
                                    .and_then(|m| m.as_str().parse::<u64>().ok())
                                    .unwrap_or(0);
                                Some((user_id, most_recent, timestamp))
                            })
                            .collect();

                        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));

                        for (user_id, _, _) in candidates {
                            let account_id = steamid64_to_accountid(&user_id).unwrap_or(user_id);
                            push_path(
                                steam_root
                                    .join("userdata")
                                    .join(account_id)
                                    .join("config")
                                    .join("localconfig.vdf"),
                            );
                        }
                    }
                }
            }
        }
    }

    let userdata_dir = steam_root.join("userdata");
    if !userdata_dir.exists() {
        return paths;
    }

    let mut fallback_paths = fs::read_dir(userdata_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("config").join("localconfig.vdf"))
        .filter(|path| path.exists())
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .ok();
            (path, modified)
        })
        .collect::<Vec<_>>();

    fallback_paths.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in fallback_paths {
        push_path(path);
    }

    paths
}

fn is_macos_app_bundle_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".app"))
        .unwrap_or(false)
}

fn macos_app_bundle_score(app_bundle: &std::path::Path) -> i32 {
    let mut score = 0;
    let contents = app_bundle.join("Contents");
    if contents.join("MacOS").is_dir() {
        score += 100;
    }
    if contents.join("Resources").join("Data").is_dir() {
        score += 200;
    }
    if contents.join("Info.plist").is_file() || contents.join("Info").is_file() {
        score += 20;
    }
    score
}

fn find_macos_app_bundles_in_dir(
    root: &std::path::Path,
    max_depth: usize,
) -> Vec<(std::path::PathBuf, usize)> {
    let mut found = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0usize));

    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }

            if is_macos_app_bundle_path(&path) {
                found.push((path, depth + 1));
                continue;
            }

            if depth < max_depth {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if matches!(
                    name.as_str(),
                    "bepinex" | "doorstop_libs" | "plugins" | "__macosx"
                ) {
                    continue;
                }
                queue.push_back((path, depth + 1));
            }
        }
    }

    found
}

fn find_macos_app_bundle(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<(std::path::PathBuf, usize, i32)> = Vec::new();
    let mut push_candidate = |candidate: std::path::PathBuf, depth: usize| {
        if candidates.iter().any(|(existing, _, _)| *existing == candidate) {
            return;
        }
        let score = macos_app_bundle_score(&candidate);
        candidates.push((candidate, depth, score));
    };

    if is_macos_app_bundle_path(game_path) {
        push_candidate(game_path.to_path_buf(), 0);
    }

    let is_contents_dir = game_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("Contents"))
        .unwrap_or(false);
    if is_contents_dir {
        if let Some(parent) = game_path.parent() {
            if is_macos_app_bundle_path(parent) {
                push_candidate(parent.to_path_buf(), 0);
            }
        }
    }

    if game_path.is_dir() {
        for (bundle, depth) in find_macos_app_bundles_in_dir(game_path, 4) {
            push_candidate(bundle, depth);
        }
    }

    candidates.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    candidates.first().map(|(bundle, _, _)| bundle.clone())
}

fn find_macos_launch_bundle(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    // If the stored game path already lives inside an app bundle (for example
    // `/Applications/Foo.app/Contents`), launch that enclosing bundle instead
    // of a nested child app. Some standalone macOS wrappers prepare runtime
    // state before handing off to the real game executable.
    find_enclosing_app_bundle(game_path).or_else(|| find_macos_app_bundle(game_path))
}

fn is_steam_bundle_path(path: &std::path::Path) -> bool {
    let lower = canonicalize_or_original(path).to_string_lossy().to_lowercase();
    lower.ends_with("/steam.app")
        || lower.contains("/steam.app/")
        || lower.ends_with("/steam.appbundle/steam")
        || lower.contains("/steam.appbundle/steam/")
}

fn find_macos_wrapper_launcher_path(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let launch_bundle = find_macos_launch_bundle(game_path)?;
    let macos_dir = launch_bundle.join("Contents").join("MacOS");
    let load_script = macos_dir.join("load");
    let steam_appid = macos_dir.join("steam_appid.txt");
    let ipcserver = macos_dir.join("ipcserver");

    if load_script.is_file() && steam_appid.is_file() && ipcserver.exists() {
        Some(load_script)
    } else {
        None
    }
}

fn find_macos_executable_path(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let app_bundle = find_macos_app_bundle(game_path)?;
    let macos_dir = app_bundle.join("Contents").join("MacOS");
    if !macos_dir.is_dir() {
        return None;
    }

    let info_plist = app_bundle.join("Contents").join("Info.plist");
    let defaults_target = if info_plist.exists() {
        info_plist
    } else {
        app_bundle.join("Contents").join("Info")
    };

    let defaults_output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", &defaults_target.to_string_lossy(), "CFBundleExecutable"])
        .output()
        .ok()?;

    if defaults_output.status.success() {
        let executable_name = String::from_utf8_lossy(&defaults_output.stdout).trim().to_string();
        if !executable_name.is_empty() {
            let candidate = macos_dir.join(&executable_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    fs::read_dir(&macos_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|entry| entry.path())
}

pub(crate) fn resolve_macos_runtime_root(game_path: &std::path::Path) -> std::path::PathBuf {
    let Some(app_bundle) = find_macos_app_bundle(game_path) else {
        return game_path.to_path_buf();
    };
    let Some(parent_dir) = app_bundle.parent() else {
        return game_path.to_path_buf();
    };

    if parent_dir != game_path && parent_dir.starts_with(game_path) {
        parent_dir.to_path_buf()
    } else {
        game_path.to_path_buf()
    }
}

fn macos_executable_supports_x86_64(executable_path: &std::path::Path) -> Result<bool, String> {
    let output = std::process::Command::new("/usr/bin/lipo")
        .args(["-archs", &executable_path.to_string_lossy()])
        .output()
        .map_err(|e| format!("Failed to inspect macOS executable architectures: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to inspect macOS executable architectures: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let archs = String::from_utf8_lossy(&output.stdout).to_lowercase();
    Ok(archs.split_whitespace().any(|arch| arch == "x86_64"))
}

fn validate_macos_bepinex_support(game_path: &std::path::Path) -> Result<(), String> {
    let Some(app_bundle) = find_macos_app_bundle(game_path) else {
        if find_windows_executable_path(game_path).is_some() {
            return Err(
                "This looks like a Windows game build. Use a Windows/CrossOver profile with Windows BepInEx instead of macOS BepInEx."
                    .to_string(),
            );
        }

        return Err(
            "macOS mod support currently requires a native macOS .app bundle inside the game directory."
                .to_string(),
        );
    };

    let data_dir = app_bundle.join("Contents").join("Resources").join("Data");
    if !data_dir.is_dir() {
        return Err(
            "Could not find a Unity Data folder inside the macOS app bundle. Native macOS BepInEx currently supports Unity .app builds only."
                .to_string(),
        );
    }

    let executable_path = find_macos_executable_path(game_path)
        .ok_or_else(|| "Could not find the macOS game executable inside the app bundle.".to_string())?;

    if !macos_executable_supports_x86_64(&executable_path)? {
        return Err(
            "This macOS build is arm64-only. Current BepInEx macOS runtimes are x64-only, so this game cannot be launched modded natively on macOS right now. If the Windows build is moddable, use a Windows/CrossOver profile."
                .to_string(),
        );
    }

    Ok(())
}

fn is_windows_game_executable_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe")
        && !lower.contains("unitycrashhandler")
        && !lower.contains("crashhandler")
        && !lower.starts_with("unins")
        && !lower.contains("setup")
}

fn windows_executable_score(game_path: &std::path::Path, executable_path: &std::path::Path) -> i32 {
    let stem = executable_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let folder_name = game_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let normalized_stem = normalize_for_matching(stem);
    let normalized_folder = normalize_for_matching(folder_name);
    let lower_stem = stem.to_lowercase();

    let mut score = 0;
    if !normalized_stem.is_empty() && !normalized_folder.is_empty() {
        if normalized_stem == normalized_folder {
            score += 120;
        } else if normalized_stem.contains(&normalized_folder) || normalized_folder.contains(&normalized_stem) {
            score += 70;
        }
    }

    if lower_stem.contains("launcher") || lower_stem.contains("bootstrap") {
        score -= 25;
    }
    if lower_stem.contains("eac") || lower_stem.contains("battleye") {
        score -= 20;
    }

    score - (stem.len() as i32)
}

fn find_windows_executable_path(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    if game_path.is_file() {
        let name = game_path.file_name()?.to_string_lossy().to_string();
        if is_windows_game_executable_name(&name) {
            return Some(game_path.to_path_buf());
        }
        return None;
    }

    let entries = fs::read_dir(game_path).ok()?;
    let mut candidates: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(is_windows_game_executable_name)
                .unwrap_or(false)
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by_key(|path| std::cmp::Reverse(windows_executable_score(game_path, path)));
    candidates.into_iter().next()
}

fn build_process_match_pattern(path: &std::path::Path) -> String {
    regex::escape(&path.to_string_lossy())
}

fn push_unique_pattern(patterns: &mut Vec<String>, pattern: String) {
    if !pattern.is_empty() && !patterns.contains(&pattern) {
        patterns.push(pattern);
    }
}

fn map_native_path_to_wine_path(
    prefix_root: &std::path::Path,
    native_path: &std::path::Path,
) -> Option<String> {
    let canonical_native = canonicalize_or_original(native_path);
    let dosdevices_dir = prefix_root.join("dosdevices");
    if dosdevices_dir.is_dir() {
        let entries = fs::read_dir(&dosdevices_dir).ok()?;
        for entry in entries.filter_map(|entry| entry.ok()) {
            let device_name = entry.file_name().to_string_lossy().to_string();
            if device_name.len() != 2 || !device_name.ends_with(':') {
                continue;
            }

            let mapped_target = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            if !canonical_native.starts_with(&mapped_target) {
                continue;
            }

            let relative = canonical_native.strip_prefix(&mapped_target).ok()?;
            let mut wine_path = format!("{}\\", device_name.to_uppercase());
            let relative_path = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("\\");
            wine_path.push_str(&relative_path);
            return Some(wine_path.trim_end_matches('\\').to_string());
        }
    }

    let drive_c_root = prefix_root.join("drive_c");
    let canonical_drive_c = canonicalize_or_original(&drive_c_root);
    if canonical_native.starts_with(&canonical_drive_c) {
        let relative = canonical_native.strip_prefix(&canonical_drive_c).ok()?;
        let relative_path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\\");
        return Some(format!("C:\\{}", relative_path));
    }

    None
}

fn build_windows_process_match_patterns(executable_path: &std::path::Path) -> Vec<String> {
    let mut patterns = Vec::new();
    push_unique_pattern(&mut patterns, build_process_match_pattern(executable_path));

    if let Some(prefix_root) = find_wine_prefix_root(executable_path) {
        if let Some(windows_path) = map_native_path_to_wine_path(&prefix_root, executable_path) {
            push_unique_pattern(&mut patterns, regex::escape(&windows_path));
        }
    }

    if let Some(file_name) = executable_path.file_name().and_then(|value| value.to_str()) {
        push_unique_pattern(&mut patterns, regex::escape(file_name));
    }

    patterns
}

fn is_process_running_for_pattern(pattern: &str) -> bool {
	#[cfg(unix)] {
		std::process::Command::new("/usr/bin/pgrep")
			.args(["-f", pattern])
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::null())
			.status()
			.map(|status| status.success())
			.unwrap_or(false)
	}

	#[cfg(windows)] {
		use std::os::windows::process::CommandExt;
		let gamefile_name = pattern.replace("\\", "");

		std::process::Command::new("tasklist")
		.creation_flags(0x08000000)
		.args(["/FI", &format!("IMAGENAME eq {}", gamefile_name), "/NH", "/FO", "CSV"])
		.output()
		.map(|out| {
			// tasklist fa schifo quindi va per forza controllato l'output
			let text = String::from_utf8_lossy(&out.stdout);
			text.to_lowercase().contains(&gamefile_name.to_lowercase())
		})
		.unwrap_or(false)
	}
}

fn is_process_running_for_patterns(patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| is_process_running_for_pattern(pattern))
}

fn is_process_running_for_executable(executable_path: &std::path::Path) -> bool {
    is_process_running_for_pattern(&build_process_match_pattern(executable_path))
}

fn wait_for_process_start_pattern(pattern: &str, timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);

    for _ in 0..attempts {
        if is_process_running_for_pattern(pattern) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }

    is_process_running_for_pattern(pattern)
}

fn wait_for_process_start_patterns(patterns: &[String], timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);

    for _ in 0..attempts {
        if is_process_running_for_patterns(patterns) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }

    is_process_running_for_patterns(patterns)
}

fn wait_for_process_start(executable_path: &std::path::Path, timeout_ms: u64) -> bool {
    wait_for_process_start_pattern(&build_process_match_pattern(executable_path), timeout_ms)
}

const MACOS_LAUNCH_OBSERVE_TIMEOUT_MS: u64 = 1_500;

fn wait_for_process_exit_pattern(pattern: &str, timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);

    for _ in 0..attempts {
        if !is_process_running_for_pattern(pattern) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }

    !is_process_running_for_pattern(pattern)
}

fn wait_for_process_exit_patterns(patterns: &[String], timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);

    for _ in 0..attempts {
        if !is_process_running_for_patterns(patterns) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }

    !is_process_running_for_patterns(patterns)
}

fn find_wine_prefix_root(path: &std::path::Path) -> Option<std::path::PathBuf> {
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

fn find_enclosing_app_bundle(path: &std::path::Path) -> Option<std::path::PathBuf> {
    path.ancestors().find_map(|ancestor| {
        let is_app_bundle = ancestor
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("app"))
            .unwrap_or(false);
        if is_app_bundle {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
}

fn push_unique_existing_path(paths: &mut Vec<std::path::PathBuf>, candidate: std::path::PathBuf) {
    if candidate.exists() && candidate.is_file() && !paths.contains(&candidate) {
        paths.push(candidate);
    }
}

fn find_named_file_under(
    root: &std::path::Path,
    names: &[&str],
    max_depth: usize,
) -> Option<std::path::PathBuf> {
    if !root.exists() || !root.is_dir() {
        return None;
    }

    let wanted: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_lowercase();

            if wanted.iter().any(|name| name == &file_name) && path.is_file() {
                return Some(path);
            }

            if depth < max_depth && entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push((path, depth + 1));
            }
        }
    }

    None
}

fn find_executables_in_path(names: &[&str]) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Some(path_var) = std::env::var_os("PATH") else {
        return found;
    };

    for dir in std::env::split_paths(&path_var) {
        for name in names {
            push_unique_existing_path(&mut found, dir.join(name));
        }
    }

    found
}

fn find_wine_runner_binary(
    prefix_root: Option<&std::path::Path>,
    executable_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();

    if let Some(prefix_root) = prefix_root {
        for relative in [
            "bin/wine",
            "wine/bin/wine",
            "bin/wine64",
            "wine/bin/wine64",
        ] {
            push_unique_existing_path(&mut candidates, prefix_root.join(relative));
        }
    }

    if let Some(bundle_path) = find_enclosing_app_bundle(executable_path)
        .or_else(|| prefix_root.and_then(find_enclosing_app_bundle))
    {
        for relative in [
            "Contents/Frameworks/wswine.bundle/bin/wine",
            "Contents/Resources/wine/bin/wine",
            "Contents/SharedSupport/wine/bin/wine",
            "Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine",
            "Contents/Frameworks/wswine.bundle/bin/wine64",
            "Contents/Resources/wine/bin/wine64",
            "Contents/SharedSupport/wine/bin/wine64",
            "Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine64",
        ] {
            push_unique_existing_path(&mut candidates, bundle_path.join(relative));
        }

        if let Some(found) = find_named_file_under(&bundle_path.join("Contents"), &["wine", "wine64"], 6) {
            push_unique_existing_path(&mut candidates, found);
        }
    }

    if let Some(home) = dirs::home_dir() {
        for app_path in [
            std::path::PathBuf::from("/Applications/CrossOver.app"),
            home.join("Applications").join("CrossOver.app"),
        ] {
            push_unique_existing_path(
                &mut candidates,
                app_path
                    .join("Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine"),
            );
            push_unique_existing_path(
                &mut candidates,
                app_path
                    .join("Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine64"),
            );
        }

        for root in [
            home.join("Library/Application Support/heroic/tools"),
            home.join("Library/Application Support/Whisky"),
        ] {
            if let Some(found) = find_named_file_under(&root, &["wine", "wine64"], 6) {
                push_unique_existing_path(&mut candidates, found);
            }
        }

        for app_root in [std::path::PathBuf::from("/Applications"), home.join("Applications")] {
            let entries = match fs::read_dir(&app_root) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.filter_map(|entry| entry.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if !path.is_dir() || !name.ends_with(".app") {
                    continue;
                }
                if !["crossover", "wine", "wineskin", "whisky", "heroic"]
                    .iter()
                    .any(|keyword| name.contains(keyword))
                {
                    continue;
                }
                if let Some(found) = find_named_file_under(&path.join("Contents"), &["wine", "wine64"], 6) {
                    push_unique_existing_path(&mut candidates, found);
                }
            }
        }
    }

    for path in find_executables_in_path(&["wine", "wine64"]) {
        push_unique_existing_path(&mut candidates, path);
    }

    for candidate in [
        "/opt/homebrew/bin/wine",
        "/usr/local/bin/wine",
        "/opt/homebrew/bin/wine64",
        "/usr/local/bin/wine64",
    ] {
        push_unique_existing_path(&mut candidates, std::path::PathBuf::from(candidate));
    }

    candidates.into_iter().next()
}

fn is_crossover_bottle(prefix_root: &std::path::Path) -> bool {
    prefix_root.join("cxbottle.conf").exists()
        || prefix_root
            .to_string_lossy()
            .to_lowercase()
            .contains("/crossover/bottles/")
}

fn is_crossover_runner(runner_path: &std::path::Path) -> bool {
    runner_path
        .to_string_lossy()
        .to_lowercase()
        .contains("crossover")
}

fn configure_windows_runner_command(
    command: &mut std::process::Command,
    runner_path: &std::path::Path,
    prefix_root: Option<&std::path::Path>,
) -> Result<(), String> {
    let use_crossover_bottle_mode = prefix_root
        .map(|prefix_root| is_crossover_bottle(prefix_root) && is_crossover_runner(runner_path))
        .unwrap_or(false);

    if use_crossover_bottle_mode {
        let bottle_path = prefix_root
            .ok_or_else(|| "CrossOver bottle path could not be determined.".to_string())?;
        command.arg("--bottle").arg(bottle_path);
        eprintln!(
            "[windows_runner] Using CrossOver bottle {:?} with runner {:?}",
            bottle_path, runner_path
        );
    } else if let Some(prefix_root) = prefix_root {
        command.env("WINEPREFIX", prefix_root);
        eprintln!(
            "[windows_runner] Using Wine prefix {:?} with runner {:?}",
            prefix_root, runner_path
        );
    } else {
        eprintln!(
            "[windows_runner] Using runner {:?} without explicit prefix",
            runner_path
        );
    }

    Ok(())
}

fn launch_windows_direct_game(game_path: &std::path::Path) -> Result<(), String> {
    let executable_path = find_windows_executable_path(game_path)
        .ok_or_else(|| "Could not find a Windows game executable in the selected folder.".to_string())?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if is_process_running_for_patterns(&process_patterns) {
        return Err("Game is already running.".to_string());
    }

    let executable_dir = executable_path.parent().unwrap_or(game_path);

    #[cfg(unix)] {
	    let prefix_root = find_wine_prefix_root(&executable_path).or_else(|| find_wine_prefix_root(game_path));

	    if let Some(runner_path) = find_wine_runner_binary(prefix_root.as_deref(), &executable_path) {
	        let mut command = std::process::Command::new(&runner_path);
	        configure_windows_runner_command(&mut command, &runner_path, prefix_root.as_deref())?;
	        eprintln!(
	            "[launch_windows_direct_game] Launching Windows executable directly: {:?}",
	            executable_path
	        );
	        command
	            .arg(&executable_path)
	            .current_dir(executable_dir)
	            .spawn()
	            .map_err(|e| format!("Failed to launch the Windows game via {}: {}", runner_path.display(), e))?;
	    } else if let Some(app_bundle) = find_enclosing_app_bundle(game_path) {
	        open::that(&app_bundle)
	            .map_err(|e| format!("Failed to launch the Windows wrapper app: {}", e))?;
	    } else {
	        return Err(
	            "No compatible Wine launcher was found. Install a Wine-compatible launcher or point the game path inside a Wine/CrossOver/Wineskin prefix."
	                .to_string(),
	        );
	    }
    }

    #[cfg(windows)] {
		eprintln!(
			"[launch_windows_direct_game] Launching Windows executable directly: {:?}",
			executable_path
		);
		std::process::Command::new(&executable_path)
		//.arg("-applaunch")
		.arg(&executable_path)
		.current_dir(executable_dir)
		.spawn()
		.map_err(|e| format!("Failed to launch the Windows game via {}: {}", executable_path.display(), e))?;
	}

    if !wait_for_process_start_patterns(&process_patterns, 20_000) {
        return Err("Game did not start in time.".to_string());
    }

    Ok(())
}

fn launch_windows_steam_game(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let executable_path = find_windows_executable_path(game_path)
        .ok_or_else(|| "Could not find a Windows game executable in the selected folder.".to_string())?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if is_process_running_for_patterns(&process_patterns) {
        return Err("Game is already running.".to_string());
    }

    let steam_root = find_matching_steam_root_for_game_path(app, game_path, true)
        .ok_or_else(|| "Could not match this Windows game to a Steam installation.".to_string())?;
    let app_id = find_steam_app_id_for_game_path(&steam_root, game_path)
        .ok_or_else(|| "Could not determine the Steam app ID for this Windows game.".to_string())?;
    let steam_executable = steam_root.join("steam.exe");
    if !steam_executable.exists() {
        return Err(format!(
            "Steam executable not found at {}. Check the Windows Steam directory in Settings.",
            steam_executable.display()
        ));
    }

    #[cfg(unix)] {
		let prefix_root = find_wine_prefix_root(&steam_executable)
			.or_else(|| find_wine_prefix_root(&executable_path))
			.or_else(|| find_wine_prefix_root(game_path));
		let runner_path = find_wine_runner_binary(prefix_root.as_deref(), &steam_executable)
			.ok_or_else(|| {
				"No compatible Wine launcher was found for this Steam installation. Set the game path inside Wine/CrossOver/Wineskin/Whisky and try again."
					.to_string()
			})?;

		let mut command = std::process::Command::new(&runner_path);
		configure_windows_runner_command(&mut command, &runner_path, prefix_root.as_deref())?;
		eprintln!(
			"[launch_windows_steam_game] Launching Steam app {} via {:?} using steam executable {:?}",
			app_id, runner_path, steam_executable
		);
		command
			.arg(&steam_executable)
			.arg("-applaunch")
			.arg(&app_id)
			.current_dir(&steam_root)
			.spawn()
			.map_err(|e| format!("Failed to launch Steam app {}: {}", app_id, e))?;
	}

	#[cfg(windows)] {
		eprintln!(
			"[launch_windows_steam_game] Launching Steam app {} via {:?}",
			app_id, steam_executable
		);
		std::process::Command::new(&steam_executable)
		.arg("-applaunch")
		.arg(&app_id)
		.current_dir(&steam_root)
		.spawn()
		.map_err(|e| format!("Failed to launch Steam app {}: {}", app_id, e))?;
	}

    if !wait_for_process_start_patterns(&process_patterns, 60_000) {
        eprintln!(
            "[launch_windows_steam_game] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
            app_id
        );
    }

    Ok(())
}

fn launch_windows_game(app: &AppHandle, game_path: &std::path::Path) -> Result<(), String> {
    let distribution = infer_distribution_from_game_path(app, game_path, true);
    if distribution == "steam" && can_launch_via_steam_for_game_path(app, game_path, true) {
        return launch_windows_steam_game(app, game_path);
    }

    launch_windows_direct_game(game_path)
}

fn remove_r2modmac_debug_logs(game_path: &std::path::Path) {
    let runtime_root = resolve_macos_runtime_root(game_path);
    for log_name in [
        "r2modmac_bootstrap.log",
        "r2modmac_dyld.log",
        "r2modmac_exec.log",
    ] {
        let log_path = runtime_root.join(log_name);
        if log_path.exists() {
            let _ = fs::remove_file(&log_path);
        }
    }

    let legacy_bootstrap_log = runtime_root.join("BepInEx").join("r2modmac_bootstrap.log");
    if legacy_bootstrap_log.exists() {
        let _ = fs::remove_file(&legacy_bootstrap_log);
    }
}

fn launch_macos_bepinex_wrapper(
    app: &AppHandle,
    game_path: &std::path::Path,
    executable_path: Option<&std::path::PathBuf>,
    context: &str,
) -> Result<bool, String> {
    let runtime_root = resolve_macos_runtime_root(game_path);

    if find_bepinex_script_in_dir(&runtime_root).is_none() {
        return Ok(false);
    }

    let run_script = canonicalize_macos_bepinex_script(&runtime_root)?;

    if let Some(executable_path) = executable_path.as_ref() {
        if is_process_running_for_executable(executable_path) {
            return Err("Game is already running.".to_string());
        }
    }

    let write_debug_logs_to_game = load_settings_impl(app).write_debug_logs_to_game;
    if !write_debug_logs_to_game {
        remove_r2modmac_debug_logs(&runtime_root);
    }

    configure_macos_bepinex_script(&run_script, &runtime_root, write_debug_logs_to_game)?;
    dequarantine_recursive(&runtime_root);

    eprintln!("[{}] Launching via run_bepinex.sh at {:?}", context, run_script);

    std::process::Command::new("/usr/bin/arch")
        .arg("-x86_64")
        .arg("/bin/bash")
        .arg(&run_script)
        .current_dir(&runtime_root)
        .spawn()
        .map_err(|e| format!("Failed to launch run_bepinex.sh: {}", e))?;

    if let Some(executable_path) = executable_path.as_ref() {
        if !wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
            eprintln!(
                "[{}] run_bepinex.sh launch request succeeded, but the game process was not observed in time. Continuing optimistically.",
                context
            );
        }
    }

    Ok(true)
}

#[cfg(target_os = "macos")]
fn macos_steam_binary_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from(
        "/Applications/Steam.app/Contents/MacOS/steam_osx",
    )];

    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("Steam")
                .join("Steam.AppBundle")
                .join("Steam")
                .join("Contents")
                .join("MacOS")
                .join("steam_osx"),
        );
    }

    candidates
}

#[cfg(target_os = "macos")]
fn ensure_macos_steam_running_for_launch() {
    if is_steam_app_running_on_macos() {
        return;
    }

    let mut started = false;
    for args in [
        vec!["-b", "com.valvesoftware.steam"],
        vec!["-a", "/Applications/Steam.app"],
        vec!["-a", "Steam"],
    ] {
        match std::process::Command::new("/usr/bin/open")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                eprintln!(
                    "[launch_via_steam_for_game_path] Steam startup requested via `/usr/bin/open {}` pid={}",
                    args.join(" "),
                    child.id()
                );
                started = true;
                break;
            }
            Err(error) => {
                eprintln!(
                    "[launch_via_steam_for_game_path] Steam startup attempt failed via `/usr/bin/open {}` error={}",
                    args.join(" "),
                    error
                );
            }
        }
    }

    if !started {
        for steam_binary in macos_steam_binary_candidates() {
            if !steam_binary.is_file() {
                continue;
            }
            match std::process::Command::new(&steam_binary)
                .arg("-silent")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    eprintln!(
                        "[launch_via_steam_for_game_path] Steam startup requested via `{}` pid={}",
                        steam_binary.display(),
                        child.id()
                    );
                    started = true;
                    break;
                }
                Err(error) => {
                    eprintln!(
                        "[launch_via_steam_for_game_path] Steam binary launch failed path={} error={}",
                        steam_binary.display(),
                        error
                    );
                }
            }
        }
    }

    if !started {
        eprintln!("[launch_via_steam_for_game_path] Failed to start Steam.app before steam://run.");
        return;
    }

    let started = std::time::Instant::now();
    while started.elapsed().as_millis() < 10_000 {
        if is_steam_app_running_on_macos() {
            eprintln!(
                "[launch_via_steam_for_game_path] Steam.app startup observed elapsed_ms={}",
                started.elapsed().as_millis()
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    eprintln!(
        "[launch_via_steam_for_game_path] Steam.app startup not observed within timeout; continuing with steam://run dispatch."
    );
}

#[cfg(not(target_os = "macos"))]
fn ensure_macos_steam_running_for_launch() {}

fn dispatch_macos_steam_run_url(app_id: &str) -> Result<std::process::Child, String> {
    let steam_url = format!("steam://run/{}", app_id);

    #[cfg(target_os = "macos")]
    {
        if let Ok(child) = std::process::Command::new("/usr/bin/open")
            .arg(&steam_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            return Ok(child);
        }

        if let Ok(child) = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.valvesoftware.steam"])
            .arg(&steam_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            return Ok(child);
        }

        for steam_binary in macos_steam_binary_candidates() {
            if !steam_binary.is_file() {
                continue;
            }
            if let Ok(child) = std::process::Command::new(&steam_binary)
                .arg(&steam_url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                return Ok(child);
            }
        }
    }

    std::process::Command::new("/usr/bin/open")
        .arg(&steam_url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to ask Steam to launch the game: {}", e))
}

fn launch_via_steam_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let launch_start = std::time::Instant::now();
    let app_id = find_steam_app_id_for_game_path_any(app, game_path, false)
        .ok_or_else(|| "Couldn't determine the Steam app ID for this game".to_string())?;
    let executable_path = find_macos_executable_path(game_path);
    eprintln!(
        "[launch_via_steam_for_game_path] start app_id={} game_path={}",
        app_id,
        game_path.display()
    );

    ensure_macos_steam_running_for_launch();
    let child = dispatch_macos_steam_run_url(&app_id)?;
    eprintln!(
        "[launch_via_steam_for_game_path] open_dispatched pid={} elapsed_ms={}",
        child.id(),
        launch_start.elapsed().as_millis()
    );

    if let Some(executable_path) = executable_path.as_ref() {
        if !wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
            eprintln!(
                "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                app_id
            );
        }
    }

    eprintln!(
        "[launch_via_steam_for_game_path] done app_id={} total_elapsed_ms={}",
        app_id,
        launch_start.elapsed().as_millis()
    );

    Ok(())
}

fn find_bepinex_script_in_dir(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if !dir.exists() {
        return None;
    }

    let canonical = dir.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    if canonical.exists() {
        return Some(canonical);
    }

    fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                && is_bepinex_shell_script_name(&entry.file_name().to_string_lossy())
        })
        .map(|entry| entry.path())
}

fn canonicalize_macos_bepinex_script(dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let canonical = dir.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    if canonical.exists() {
        return Ok(canonical);
    }

    let Some(found) = find_bepinex_script_in_dir(dir) else {
        return Err("No macOS BepInEx startup script found".to_string());
    };

    if found == canonical {
        return Ok(canonical);
    }

    if let Err(rename_err) = fs::rename(&found, &canonical) {
        fs::copy(&found, &canonical).map_err(|copy_err| {
            format!(
                "Failed to normalize macOS BepInEx startup script: rename failed ({}), copy failed ({})",
                rename_err, copy_err
            )
        })?;
        let _ = fs::remove_file(&found);
    }

    Ok(canonical)
}

fn has_complete_macos_bepinex_runtime(game_path: &std::path::Path) -> bool {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let has_core = runtime_root.join("BepInEx").join("core").is_dir();
    let has_doorstop_payload = runtime_root.join("doorstop_libs").is_dir()
        || runtime_root.join("libdoorstop.dylib").exists();
    let has_script = find_bepinex_script_in_dir(&runtime_root).is_some();

    has_core && has_doorstop_payload && has_script
}

fn has_complete_disabled_macos_bepinex_runtime(game_path: &std::path::Path) -> bool {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let has_core = runtime_root.join("BepInEx_DISABLED").join("core").is_dir();
    let has_doorstop_payload = runtime_root.join("doorstop_libs_DISABLED").is_dir()
        || runtime_root.join("libdoorstop.dylib_DISABLED").exists();
    let has_script = find_bepinex_script_in_dir(&runtime_root).is_some();

    has_core && has_doorstop_payload && has_script
}

fn ensure_windows_bepinex_console_enabled(game_path: &std::path::Path) -> Result<(), String> {
    let config_path = game_path.join("BepInEx").join("config").join("BepInEx.cfg");
    if !config_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read BepInEx.cfg: {}", e))?;

    if content.contains("[Logging.Console]") && content.contains("Enabled = true") {
        return Ok(());
    }

    let updated = if content.contains("[Logging.Console]") && content.contains("Enabled = false") {
        content.replacen("Enabled = false", "Enabled = true", 1)
    } else if content.contains("[Logging.Console]") {
        content
    } else {
        format!(
            "{}\n[Logging.Console]\nEnabled = true\n",
            content.trim_end()
        )
    };

    fs::write(&config_path, updated)
        .map_err(|e| format!("Failed to update BepInEx.cfg: {}", e))?;
    Ok(())
}

fn rename_path_if_present(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    if !src.exists() && !src.is_symlink() {
        return Ok(());
    }

    if dst.exists() || dst.is_symlink() {
        if dst.is_dir() && !dst.is_symlink() {
            let _ = fs::remove_dir_all(dst);
        } else {
            let _ = fs::remove_file(dst);
        }
    }

    fs::rename(src, dst).map_err(|e| format!("Failed to move {} -> {}: {}", src.display(), dst.display(), e))
}

fn sync_macos_runtime_disabled_state(
    game_path: &std::path::Path,
    disable: bool,
) -> Result<(), String> {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let dir_items = ["BepInEx", "doorstop_libs"];
    let file_items = [
        "doorstop_config.ini",
        "libdoorstop.dylib",
        ".doorstop_version",
    ];

    if disable {
        let stale_bepinex = runtime_root.join("BepInEx");
        if stale_bepinex.is_dir() {
            let mut can_remove = true;
            if let Ok(entries) = fs::read_dir(&stale_bepinex) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name != "r2modmac_bootstrap.log" && name != ".DS_Store" {
                        can_remove = false;
                        break;
                    }
                }
            } else {
                can_remove = false;
            }

            if can_remove {
                let _ = fs::remove_dir_all(&stale_bepinex);
            }
        }
    }

    for item in dir_items {
        let active = runtime_root.join(item);
        let disabled = runtime_root.join(format!("{}_DISABLED", item));
        if disable {
            if active.is_dir() && disabled.is_dir() {
                let _ = fs::remove_dir_all(&active);
                continue;
            }
            rename_path_if_present(&active, &disabled)?;
        } else if disabled.exists() {
            if !active.exists() {
                rename_path_if_present(&disabled, &active)?;
            } else if disabled.is_dir() && !disabled.is_symlink() {
                let _ = fs::remove_dir_all(&disabled);
            } else {
                let _ = fs::remove_file(&disabled);
            }
        }
    }

    for item in file_items {
        let active = runtime_root.join(item);
        let disabled = runtime_root.join(format!("{}_DISABLED", item));
        if disable {
            if (active.exists() || active.is_symlink()) && (disabled.exists() || disabled.is_symlink()) {
                let _ = fs::remove_file(&active);
                continue;
            }
            rename_path_if_present(&active, &disabled)?;
        } else if disabled.exists() {
            if !active.exists() {
                rename_path_if_present(&disabled, &active)?;
            } else {
                let _ = fs::remove_file(&disabled);
            }
        }
    }

    let active_script = runtime_root.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    let disabled_script = runtime_root.join(format!("{}_DISABLED", CANONICAL_MAC_BEPINEX_SCRIPT));
    if !active_script.exists() && disabled_script.exists() {
        rename_path_if_present(&disabled_script, &active_script)?;
    } else if active_script.exists() && disabled_script.exists() {
        if disabled_script.is_dir() && !disabled_script.is_symlink() {
            let _ = fs::remove_dir_all(&disabled_script);
        } else {
            let _ = fs::remove_file(&disabled_script);
        }
    }

    if let Ok(entries) = fs::read_dir(&runtime_root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if disable {
                if path.is_file() && is_bepinex_shell_script_name(&name) && name != CANONICAL_MAC_BEPINEX_SCRIPT {
                    let disabled = runtime_root.join(format!("{}_DISABLED", name));
                    rename_path_if_present(&path, &disabled)?;
                }
            } else if name.ends_with("_DISABLED") {
                let active_name = name.trim_end_matches("_DISABLED").to_string();
                if is_bepinex_shell_script_name(&active_name) {
                    let active_path = runtime_root.join(&active_name);
                    if !active_path.exists() {
                        rename_path_if_present(&path, &active_path)?;
                    } else if path.is_dir() && !path.is_symlink() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    Ok(())
}

fn has_balatro_lovely_runtime(game_path: &std::path::Path) -> bool {
    game_path.join(BALATRO_LOVELY_SCRIPT).exists() && game_path.join("liblovely.dylib").exists()
}

fn balatro_mods_disabled_dir() -> Result<std::path::PathBuf, String> {
    let mods_dir = get_balatro_mods_dir()
        .ok_or_else(|| "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string())?;
    let parent = mods_dir
        .parent()
        .ok_or_else(|| "Could not resolve Balatro application support directory".to_string())?;
    Ok(parent.join("Mods_DISABLED"))
}

fn set_executable_if_present(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("Failed to inspect file permissions: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("Failed to update executable permissions: {}", e))?;
    }

    Ok(())
}

fn read_manifest_version(dir: &std::path::Path) -> Option<String> {
    let manifest_path = dir.join("manifest.json");
    let data = fs::read_to_string(manifest_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;
    json.get("version_number")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("versionNumber").and_then(|v| v.as_str()))
        .map(|v| v.to_string())
}

fn is_managed_macos_launch_option(value: &str) -> bool {
    let lower = value.to_lowercase();
    let has_script = lower.contains("run_bepinex.sh") || lower.contains("bepinex.sh");
    has_script && lower.contains("%command%")
}

fn extract_macos_launch_script_path(value: &str) -> Option<std::path::PathBuf> {
    let re = regex::Regex::new(r#""([^"]*(?:run_bepinex\.sh|bepinex\.sh))""#).ok()?;
    re.captures(value)
        .and_then(|captures| captures.get(1).map(|path| std::path::PathBuf::from(path.as_str())))
}

fn is_managed_macos_launch_option_for_game(value: &str, game_path: &std::path::Path) -> bool {
    if !is_managed_macos_launch_option(value) {
        return false;
    }

    let Some(script_path) = extract_macos_launch_script_path(value) else {
        return false;
    };

    if script_path.is_relative() {
        return true;
    }

    script_path
        .parent()
        .map(|script_dir| game_path_matches_install_root(script_dir, game_path))
        .unwrap_or(false)
}

const MACOS_STEAM_APP_PROCESS_NAME: &str = "steam_osx";
const MACOS_STEAM_HELPER_PROCESS_NAMES: &[&str] = &[
    "Steam Helper",
    "steamwebhelper",
    "ipcserver",
];
const MACOS_STEAM_QUIT_PROCESS_NAMES: &[&str] = &[
    MACOS_STEAM_APP_PROCESS_NAME,
    "Steam Helper",
    "steamwebhelper",
    "ipcserver",
];
const MACOS_STEAM_KILL_FALLBACK_PATTERNS: &[&str] = &[
    "steam_osx",
    "steamwebhelper",
    "Steam Helper",
    "ipcserver",
    "Steam.AppBundle",
    "steam.sh",
    "steam_monitor.sh",
];

fn is_named_process_running_on_macos(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/pgrep")
            .args(["-x", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn is_steam_app_running_on_macos() -> bool {
    is_named_process_running_on_macos(MACOS_STEAM_APP_PROCESS_NAME)
}

fn is_steam_running_on_macos() -> bool {
    is_steam_app_running_on_macos()
        || MACOS_STEAM_HELPER_PROCESS_NAMES
        .iter()
        .any(|name| is_named_process_running_on_macos(name))
}

#[cfg(target_os = "macos")]
fn collect_macos_steam_process_ids(steam_roots: &[std::path::PathBuf]) -> HashSet<u32> {
    let mut pids = HashSet::new();

    for steam_root in steam_roots {
        let steam_app_root = steam_root.join("Steam.AppBundle").join("Steam");
        if !steam_app_root.exists() {
            continue;
        }

        let pattern = regex::escape(&steam_app_root.to_string_lossy());
        let Ok(output) = std::process::Command::new("/usr/bin/pgrep")
            .args(["-f", &pattern])
            .output()
        else {
            continue;
        };

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                pids.insert(pid);
            }
        }
    }

    pids
}

#[cfg(target_os = "macos")]
fn has_macos_steam_processes(steam_roots: &[std::path::PathBuf]) -> bool {
    is_steam_running_on_macos() || !collect_macos_steam_process_ids(steam_roots).is_empty()
}

#[cfg(target_os = "macos")]
fn collect_macos_steam_process_snapshot() -> Vec<String> {
    let Ok(output) = std::process::Command::new("/usr/bin/pgrep")
        .args([
            "-af",
            "steam_osx|steamwebhelper|Steam Helper|ipcserver|Steam.AppBundle|steam.sh|steam_monitor.sh",
        ])
        .output()
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn has_macos_steam_processes(_steam_roots: &[std::path::PathBuf]) -> bool {
    false
}

fn quit_steam_if_running(steam_roots: &[std::path::PathBuf]) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let steam_app_was_running = is_steam_app_running_on_macos();
        if !has_macos_steam_processes(steam_roots) {
            return Ok(false);
        }

        if steam_app_was_running {
            eprintln!(
                "[quit_steam_if_running] Steam is running — force killing Steam processes to apply launch option changes immediately..."
            );
        } else {
            eprintln!(
                "[quit_steam_if_running] Steam.app is not running, but helper processes are still alive — clearing stale Steam helpers before launch option update..."
            );
        }

        // Force kill every process under Steam.AppBundle first, then fallback names.
        for pid in collect_macos_steam_process_ids(steam_roots) {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-9", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        for process_name in MACOS_STEAM_QUIT_PROCESS_NAMES {
            let _ = std::process::Command::new("/usr/bin/killall")
                .args(["-9", process_name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        for pattern in MACOS_STEAM_KILL_FALLBACK_PATTERNS {
            let _ = std::process::Command::new("/usr/bin/pkill")
                .args(["-9", "-f", pattern])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        // Second sweep to catch orphan helpers that may survive the first kill.
        for pid in collect_macos_steam_process_ids(steam_roots) {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-9", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        if has_macos_steam_processes(steam_roots) {
            let leftovers = collect_macos_steam_process_snapshot();
            eprintln!(
                "[quit_steam_if_running] Some Steam processes are still present after force-kill; proceeding anyway."
            );
            if !leftovers.is_empty() {
                eprintln!(
                    "[quit_steam_if_running] leftover_processes={}",
                    leftovers.join(" | ")
                );
            }
        } else {
            eprintln!("[quit_steam_if_running] Steam processes terminated.");
        }

        return Ok(steam_app_was_running);
    }

    #[allow(unreachable_code)]
    Ok(false)
}

fn emit_steam_launch_options_restart_event(app: &AppHandle) {
    let _ = app.emit(STEAM_LAUNCH_OPTIONS_RESTART_EVENT, true);
}

#[cfg(target_os = "macos")]
fn relaunch_macos_steam_if_needed(_steam_root: &std::path::Path) {
    let mut command = std::process::Command::new("/usr/bin/open");
    command.args(["-a", "Steam"]);

    let Ok(_) = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        eprintln!(
            "[relaunch_macos_steam_if_needed] Failed to issue Steam relaunch request."
        );
        return;
    };
    eprintln!("[relaunch_macos_steam_if_needed] Steam relaunch requested.");
}

#[cfg(not(target_os = "macos"))]
fn relaunch_macos_steam_if_needed(_steam_root: &std::path::Path) {}

fn find_next_non_whitespace(text: &str, mut index: usize, end: usize) -> Option<usize> {
    while index < end {
        match text.as_bytes()[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            _ => return Some(index),
        }
    }
    None
}

fn find_matching_brace(text: &str, open_index: usize, end: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = open_index;
    let mut in_string = false;
    let mut escaped = false;

    while index < end {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }

    None
}

fn find_block_by_key(
    text: &str,
    key: &str,
    start: usize,
    end: usize,
) -> Option<(usize, usize, usize, String)> {
    let pattern = format!("\"{}\"", key);
    let mut search_start = start;

    while search_start < end {
        let relative_match = text[search_start..end].find(&pattern)?;
        let key_index = search_start + relative_match;
        let line_start = text[..key_index].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let indentation = text[line_start..key_index].to_string();

        let block_search_start = key_index + pattern.len();
        let brace_index = find_next_non_whitespace(text, block_search_start, end)?;
        if text.as_bytes()[brace_index] == b'{' {
            let close_index = find_matching_brace(text, brace_index, end)?;
            return Some((key_index, brace_index, close_index, indentation));
        }

        search_start = key_index + pattern.len();
    }

    None
}

fn find_all_blocks_by_key(
    text: &str,
    key: &str,
    start: usize,
    end: usize,
) -> Vec<(usize, usize, usize, String)> {
    let pattern = format!("\"{}\"", key);
    let mut search_start = start;
    let mut matches = Vec::new();

    while search_start < end {
        let Some(relative_match) = text[search_start..end].find(&pattern) else {
            break;
        };
        let key_index = search_start + relative_match;
        let line_start = text[..key_index].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let indentation = text[line_start..key_index].to_string();

        let block_search_start = key_index + pattern.len();
        if let Some(brace_index) = find_next_non_whitespace(text, block_search_start, end) {
            if text.as_bytes()[brace_index] == b'{' {
                if let Some(close_index) = find_matching_brace(text, brace_index, end) {
                    matches.push((key_index, brace_index, close_index, indentation));
                    search_start = close_index + 1;
                    continue;
                }
            }
        }

        search_start = key_index + pattern.len();
    }

    matches
}

fn escape_vdf_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn unescape_vdf_value(value: &str) -> String {
    value
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

fn extract_launch_options_from_app_block(block: &str) -> Option<String> {
    let re = regex::Regex::new(r#"(?m)^[ \t]*"LaunchOptions"[ \t]*"((?:\\.|[^"])*)"[ \t]*$"#).ok()?;
    re.captures(block)
        .and_then(|captures| captures.get(1).map(|value| unescape_vdf_value(value.as_str())))
}

fn find_steam_apps_block(text: &str, app_id: Option<&str>) -> Option<(usize, usize, usize, String)> {
    let (_, software_open, software_close, _) = find_block_by_key(text, "Software", 0, text.len())?;
    let (_, valve_open, valve_close, _) =
        find_block_by_key(text, "Valve", software_open + 1, software_close)?;
    let (_, steam_open, steam_close, _) =
        find_block_by_key(text, "Steam", valve_open + 1, valve_close)?;

    let candidates = find_all_blocks_by_key(text, "apps", steam_open + 1, steam_close);
    if candidates.is_empty() {
        return None;
    }

    let mut scored = candidates
        .into_iter()
        .map(|candidate| {
            let (_, apps_open, apps_close, _) = &candidate;
            let content = &text[*apps_open + 1..*apps_close];
            let mut score = 0i32;

            if let Some(app_id) = app_id {
                if let Some((_, app_open, app_close, _)) =
                    find_block_by_key(text, app_id, *apps_open + 1, *apps_close)
                {
                    score += 2000;
                    let app_content = &text[app_open + 1..app_close];
                    if app_content.contains("\"LastPlayed\"")
                        || app_content.contains("\"Playtime\"")
                        || app_content.contains("\"BadgeData\"")
                        || app_content.contains("\"cloud\"")
                        || app_content.contains("\"autocloud\"")
                        || app_content.contains("\"LaunchOptions\"")
                    {
                        score += 500;
                    }
                    if app_content.contains("\"UseSteamControllerConfig\"")
                        || app_content.contains("\"SteamControllerRumble\"")
                    {
                        score -= 1000;
                    }
                } else {
                    score -= 1000;
                }
            }

            if content.contains("\"LastPlayed\"") || content.contains("\"Playtime\"") {
                score += 100;
            }
            if content.contains("\"LaunchOptions\"") {
                score += 50;
            }
            if content.contains("\"UseSteamControllerConfig\"") {
                score -= 250;
            }
            if content.contains("\"SteamControllerRumble\"")
                || content.contains("\"SteamControllerRumbleIntensity\"")
            {
                score -= 250;
            }

            (score, candidate)
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, candidate)| candidate).next()
}

fn get_launch_options_for_app(text: &str, app_id: &str) -> Option<String> {
    let (_, apps_open, apps_close, _) = find_steam_apps_block(text, Some(app_id))?;
    let (_, app_open, app_close, _) = find_block_by_key(text, app_id, apps_open + 1, apps_close)?;
    extract_launch_options_from_app_block(&text[app_open + 1..app_close])
}

fn update_launch_options_in_localconfig(
    text: &str,
    app_id: &str,
    desired: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let (_, apps_open, apps_close, apps_indent) = find_steam_apps_block(text, Some(app_id))
        .ok_or_else(|| "Steam localconfig.vdf does not contain an apps block".to_string())?;

    let app_block = find_block_by_key(text, app_id, apps_open + 1, apps_close);
    let launch_options_re = regex::Regex::new(
        r#"(?m)^[ \t]*"LaunchOptions"[ \t]*"((?:\\.|[^"])*)"[ \t]*\r?\n?"#,
    )
    .map_err(|e| format!("Invalid launch options regex: {}", e))?;

    if let Some((_, app_open, app_close, app_indent)) = app_block {
        let block_content = &text[app_open + 1..app_close];
        let current = extract_launch_options_from_app_block(block_content);

        let property_indent = format!("{}\t", app_indent);
        let mut updated_block = if let Some(value) = desired {
            let replacement_line = format!(
                "{}\"LaunchOptions\"\t\t\"{}\"\n",
                property_indent,
                escape_vdf_value(value)
            );
            if launch_options_re.is_match(block_content) {
                launch_options_re.replace(block_content, replacement_line.as_str()).to_string()
            } else {
                let mut block = block_content.to_string();
                if !block.ends_with('\n') {
                    block.push('\n');
                }
                block.push_str(&replacement_line);
                block
            }
        } else {
            launch_options_re.replace(block_content, "").to_string()
        };

        if !updated_block.ends_with('\n') {
            updated_block.push('\n');
        }

        let updated_text = format!(
            "{}{}{}",
            &text[..app_open + 1],
            updated_block,
            &text[app_close..]
        );

        return Ok((updated_text, current));
    }

    if let Some(value) = desired {
        let app_indent = format!("{}\t", apps_indent);
        let property_indent = format!("{}\t", app_indent);
        let insertion = format!(
            "\n{}\"{}\"\n{}{{\n{}\"LaunchOptions\"\t\t\"{}\"\n{}}}\n",
            app_indent,
            app_id,
            app_indent,
            property_indent,
            escape_vdf_value(value),
            app_indent
        );
        let updated_text = format!(
            "{}{}{}",
            &text[..apps_close],
            insertion,
            &text[apps_close..]
        );
        return Ok((updated_text, None));
    }

    Ok((text.to_string(), None))
}

fn copy_macos_bepinex_runtime_root(
    source_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let root_dirs = ["BepInEx", "doorstop_libs"];
    for item in root_dirs {
        let src = source_root.join(item);
        let dst = game_path.join(item);
        if !src.exists() {
            continue;
        }
        if dst.exists() {
            let _ = fs::remove_dir_all(&dst);
        }
        copy_dir_recursive(&src, &dst)
            .map_err(|e| format!("Failed to copy {}: {}", item, e))?;
    }

    let root_files = ["doorstop_config.ini", "libdoorstop.dylib", CANONICAL_MAC_BEPINEX_SCRIPT];
    for item in root_files {
        let src = source_root.join(item);
        let dst = game_path.join(item);
        if !src.exists() {
            continue;
        }
        if dst.exists() {
            let _ = fs::remove_file(&dst);
        }
        fs::copy(&src, &dst).map_err(|e| format!("Failed to copy {}: {}", item, e))?;
        if item == "doorstop_config.ini" {
            normalize_macos_doorstop_config_file(&dst)?;
            configure_macos_doorstop_target_assembly(&dst, game_path)?;
        }
    }

    Ok(())
}

fn configure_macos_doorstop_target_assembly(
    config_path: &std::path::Path,
    game_path: &std::path::Path,
) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }

    let preloader_path = game_path
        .join("BepInEx")
        .join("core")
        .join("BepInEx.Preloader.dll")
        .to_string_lossy()
        .replace('\\', "/");
    let core_path = game_path
        .join("BepInEx")
        .join("core")
        .to_string_lossy()
        .replace('\\', "/");
    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let target_line = format!("targetAssembly={}", preloader_path);
    let dll_search_path_line = format!("dllSearchPathOverride={}", core_path);

    let mut updated = if let Ok(target_re) = regex::Regex::new(r"(?m)^targetAssembly=.*$") {
        target_re.replace(&content, target_line.as_str()).into_owned()
    } else {
        content.clone()
    };

    if let Ok(dll_search_re) = regex::Regex::new(r"(?m)^dllSearchPathOverride=.*$") {
        updated = dll_search_re
            .replace(&updated, dll_search_path_line.as_str())
            .into_owned();
    }

    if updated != content {
        fs::write(config_path, updated).map_err(|e| e.to_string())?;
    }

    Ok(())
}

async fn ensure_macos_bepinex_runtime_present(
    app: &AppHandle,
    profile_id: &str,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let runtime_root = resolve_macos_runtime_root(game_path);

    if has_complete_macos_bepinex_runtime(&runtime_root) {
        normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
        configure_macos_doorstop_target_assembly(
            &runtime_root.join("doorstop_config.ini"),
            &runtime_root,
        )?;
        return Ok(());
    }

    let profile_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id);

    if has_complete_macos_bepinex_runtime(&profile_dir) {
        normalize_macos_doorstop_config_file(&profile_dir.join("doorstop_config.ini"))?;
        copy_macos_bepinex_runtime_root(&profile_dir, &runtime_root)?;
        dequarantine_recursive(&runtime_root);
        if has_complete_macos_bepinex_runtime(&runtime_root) {
            return Ok(());
        }
    }

    let profiles_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles.json");
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or_else(|| "Profile not found while restoring macOS BepInEx runtime".to_string())?;

    let bepinex_full_name = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|m| m["enabled"].as_bool().unwrap_or(true))
        .filter_map(|m| m["fullName"].as_str())
        .find(|full_name| full_name.to_lowercase().contains("bepinexpack"))
        .map(|s| s.to_string());

    let Some(bepinex_full_name) = bepinex_full_name else {
        return Ok(());
    };

    let version_number = extract_version_number_from_full_name(&bepinex_full_name)
        .ok_or_else(|| format!("Could not parse BepInEx version from {}", bepinex_full_name))?;
    let runtime_kind = detect_unity_runtime_kind(&runtime_root);
    let runtime_bytes = download_official_macos_bepinex_runtime(&version_number, runtime_kind).await?;

    fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut game_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    extract_bepinex_pack_to_root(&mut game_archive, &runtime_root, true, false)?;
    normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
    configure_macos_doorstop_target_assembly(
        &runtime_root.join("doorstop_config.ini"),
        &runtime_root,
    )?;

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut profile_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    extract_bepinex_pack_to_root(&mut profile_archive, &profile_dir, true, false)?;
    normalize_macos_doorstop_config_file(&profile_dir.join("doorstop_config.ini"))?;

    dequarantine_recursive(&runtime_root);

    if has_complete_macos_bepinex_runtime(&runtime_root) {
        Ok(())
    } else {
        Err("No macOS BepInEx startup script found".to_string())
    }
}

fn configure_macos_bepinex_script(
    script_path: &std::path::Path,
    game_path: &std::path::Path,
    write_debug_logs_to_game: bool,
) -> Result<(), String> {
    fn resolve_macos_executable_path(
        game_path: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        find_macos_executable_path(game_path).ok_or_else(|| {
            "No macOS executable found (supported locations include nested .app bundles such as Contents/game/*.app).".to_string()
        })
    }

    fn resolve_macos_launch_entry_path(
        game_path: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        find_macos_wrapper_launcher_path(game_path)
            .or_else(|| find_macos_executable_path(game_path))
            .ok_or_else(|| {
                "No macOS launch entry found (supported locations include wrapper launchers such as Contents/MacOS/load and nested .app bundles such as Contents/game/*.app).".to_string()
            })
    }

    fn has_macos_doorstop_support(script: &str) -> bool {
        let lower = script.to_lowercase();
        lower.contains("dyld_insert_libraries")
            && lower.contains("dylib")
            && (lower.contains("doorstop_enable") || lower.contains("doorstop_enabled"))
    }

    fn build_generated_macos_bepinex_script(
        relative_exec: &str,
        relative_launch_entry: &str,
        launch_entry_uses_wrapper: bool,
        write_debug_logs_to_game: bool,
    ) -> String {
        let write_debug_logs = if write_debug_logs_to_game { 1 } else { 0 };
        let launch_entry_uses_wrapper = if launch_entry_uses_wrapper { 1 } else { 0 };
        format!(
            r#"#!/bin/sh
# r2modmac generated macOS BepInEx launcher
executable_name="{relative_exec}"
launch_entry_name="{relative_launch_entry}"
launch_entry_uses_wrapper={launch_entry_uses_wrapper}
write_debug_logs={write_debug_logs}

a="/$0"; a=${{a%/*}}; a=${{a#/}}; a=${{a:-.}}; BASEDIR=$(cd "$a"; pwd -P)
cd "$BASEDIR"

if [ "$write_debug_logs" = "1" ]; then
    bootstrap_log="$BASEDIR/r2modmac_bootstrap.log"
    if [ -z "${{R2MODMAC_BOOTSTRAP_LOG_READY:-}}" ]; then
        : > "$bootstrap_log"
        export R2MODMAC_BOOTSTRAP_LOG_READY=1
    fi

    dyld_log="$BASEDIR/r2modmac_dyld.log"
    if [ -z "${{R2MODMAC_DYLD_LOG_READY:-}}" ]; then
        : > "$dyld_log"
        export R2MODMAC_DYLD_LOG_READY=1
    fi

    exec_log="$BASEDIR/r2modmac_exec.log"
    if [ -z "${{R2MODMAC_EXEC_LOG_READY:-}}" ]; then
        : > "$exec_log"
        export R2MODMAC_EXEC_LOG_READY=1
    fi

    log_bootstrap() {{
        printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >> "$bootstrap_log"
    }}
else
    bootstrap_log="/dev/null"
    dyld_log="/dev/null"
    exec_log="/dev/null"
    log_bootstrap() {{
        :
    }}
fi

log_bootstrap "wrapper_start pid=$$ ppid=$PPID argv=$*"
wrapper_arch=$(/usr/bin/arch 2>/dev/null || printf unknown)
wrapper_translated=$(/usr/sbin/sysctl -in sysctl.proc_translated 2>/dev/null || printf 0)
log_bootstrap "wrapper_arch=$wrapper_arch translated=$wrapper_translated"

# r2modmac: if the runtime is marked disabled, launch the game without Doorstop.
runtime_disabled=false
if [ -e "$BASEDIR/BepInEx_DISABLED" ] || [ -e "$BASEDIR/doorstop_libs_DISABLED" ] || [ -e "$BASEDIR/libdoorstop.dylib_DISABLED" ] || [ -e "$BASEDIR/doorstop_config.ini_DISABLED" ]; then
    runtime_disabled=true
    log_bootstrap "runtime_disabled=true"
else
    log_bootstrap "runtime_disabled=false"
fi

if command -v xattr >/dev/null 2>&1; then
  /usr/bin/xattr -d com.apple.quarantine "$BASEDIR/run_bepinex.sh" "$BASEDIR/doorstop_libs" "$BASEDIR/BepInEx" "$BASEDIR"/*.dylib 2>/dev/null || true
fi

export DOORSTOP_ENABLE=1
export DOORSTOP_ENABLED=1
export DOORSTOP_INVOKE_DLL_PATH="$BASEDIR/BepInEx/core/BepInEx.Preloader.dll"
export DOORSTOP_TARGET_ASSEMBLY="$DOORSTOP_INVOKE_DLL_PATH"
export DOORSTOP_BOOT_CONFIG_OVERRIDE=""
export DOORSTOP_IGNORE_DISABLED_ENV=0
export DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="$BASEDIR/BepInEx/core"
export DOORSTOP_MONO_DEBUG_ENABLED=0
export DOORSTOP_MONO_DEBUG_START_SERVER=0
export DOORSTOP_MONO_DEBUG_ADDRESS="127.0.0.1:10000"
export DOORSTOP_MONO_DEBUG_SUSPEND=0
export DOORSTOP_CLR_RUNTIME_CORECLR_PATH=""
export DOORSTOP_CLR_CORLIB_DIR=""
export DOORSTOP_CORLIB_OVERRIDE_PATH=""
export DOORSTOP_REDIRECT_OUTPUT_LOG=1

steam_arg_helper() {{
    if [ "$executable_name" != "" ] && [ "$1" != "${{1%"$executable_name"}}" ]; then
        return 0
    elif [ "$executable_name" = "" ] && [ "$1" != "${{1%.x86_64}}" ]; then
        return 0
    elif [ "$executable_name" = "" ] && [ "$1" != "${{1%.x86}}" ]; then
        return 0
    else
        return 1
    fi
}}

steam_launch_args_ready=false
for a in "$@"; do
    if [ "$a" = "SteamLaunch" ]; then
        log_bootstrap "steam_launch_branch_entered"
        rotated=0
        max=$#
        while [ $rotated -lt $max ]; do
            if steam_arg_helper "$1"; then
                to_rotate=$(($# - rotated))
                set -- "$@" "$0"
                while [ $((to_rotate-=1)) -ge 0 ]; do
                    set -- "$@" "$1"
                    shift
                done
                steam_launch_args_ready=true
                log_bootstrap "steam_launch_args_ready argv=$*"
                break
            else
                set -- "$@" "$1"
                shift
                rotated=$((rotated+1))
            fi
        done
        if [ "$steam_launch_args_ready" != true ]; then
            log_bootstrap "steam_launch_branch_failed_to_match_executable"
            echo "Please set executable_name to a valid name in a text editor"
            exit 1
        fi
        break
    fi
done

case "$executable_name" in
    *.app|/*.app)
        real_executable_name="$executable_name"
        case "$real_executable_name" in
            /*) ;;
            *) real_executable_name="$BASEDIR/$real_executable_name" ;;
        esac
        inner_executable_name=$(defaults read "${{real_executable_name}}/Contents/Info" CFBundleExecutable 2>/dev/null || defaults read "${{real_executable_name}}/Contents/Info.plist" CFBundleExecutable 2>/dev/null)
        executable_path="${{real_executable_name}}/Contents/MacOS/${{inner_executable_name}}"
        ;;
    *.app/Contents/MacOS/*|/*.app/Contents/MacOS/*)
        case "$executable_name" in
            /*) executable_path="$executable_name" ;;
            *) executable_path="$BASEDIR/$executable_name" ;;
        esac
        ;;
    /*) executable_path="$executable_name" ;;
    *) executable_path="$BASEDIR/$executable_name" ;;
esac

case "$launch_entry_name" in
    /*) launch_entry_path="$launch_entry_name" ;;
    *) launch_entry_path="$BASEDIR/$launch_entry_name" ;;
esac

if [ -n "$1" ]; then
    case "$1" in
        *.app)
            real_executable_name=$(defaults read "$1/Contents/Info" CFBundleExecutable)
            executable_path="$1/Contents/MacOS/${{real_executable_name}}"
            ;;
        *.app/Contents/MacOS/*)
            executable_path="$1"
            ;;
    esac
fi

abs_path() {{
    echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
}}

_readlink() {{
    ab_path="$(abs_path "$1")"
    link="$(readlink "${{ab_path}}")"
    case $link in
        /*) ;;
        *) link="$(dirname "$ab_path")/$link" ;;
    esac
    echo "$link"
}}

resolve_executable_path() {{
    e_path="$(abs_path "$1")"
    while [ -L "${{e_path}}" ]; do
        e_path=$(_readlink "${{e_path}}")
    done
    echo "${{e_path}}"
}}

executable_path=$(resolve_executable_path "${{executable_path}}")
launch_entry_path=$(resolve_executable_path "${{launch_entry_path}}")
log_bootstrap "resolved_executable_path=$executable_path"
log_bootstrap "resolved_launch_entry_path=$launch_entry_path wrapper=$launch_entry_uses_wrapper"

app_path="${{executable_path%/Contents/MacOS*}}"
app_path_lower=$(printf "%s" "$app_path" | tr '[:upper:]' '[:lower:]')
if echo "$app_path_lower" | grep -Eq '/steam\.app(/|$)|/steam\.appbundle/steam(/|$)'; then
    log_bootstrap "codesign_remove_signature_skipped_steam app_path=$app_path"
elif command -v codesign >/dev/null 2>&1 && [ -d "$app_path" ] && codesign -d "$app_path" >/dev/null 2>&1; then
    log_bootstrap "codesign_remove_signature_attempt app_path=$app_path"
    if codesign --remove-signature "$app_path" >/dev/null 2>&1; then
        log_bootstrap "codesign_remove_signature_ok"
    else
        log_bootstrap "codesign_remove_signature_failed"
    fi
fi

executable_type=$(LD_PRELOAD="" file -b "${{executable_path}}")
log_bootstrap "executable_type=$executable_type"
if [ "$wrapper_translated" = "1" ]; then
    native_macos_arch="arm64"
else
    native_macos_arch=$(/usr/bin/uname -m 2>/dev/null || printf unknown)
fi
log_bootstrap "native_macos_arch=$native_macos_arch"

root_doorstop_dylib="$BASEDIR/libdoorstop.dylib"
root_doorstop_type=""
if [ -f "$root_doorstop_dylib" ]; then
    root_doorstop_type=$(LD_PRELOAD="" file -b "$root_doorstop_dylib")
    log_bootstrap "root_doorstop_type=$root_doorstop_type"
fi

root_loader_mode=false
if [ -f "$root_doorstop_dylib" ] && [ ! -d "$BASEDIR/doorstop_libs" ]; then
    root_loader_mode=true
    export DOORSTOP_ENABLE=1
    export DOORSTOP_ENABLED=1
    export DOORSTOP_TARGET_ASSEMBLY="$DOORSTOP_INVOKE_DLL_PATH"
    export DOORSTOP_CLR_RUNTIME_CORECLR_PATH=""
    export DOORSTOP_CLR_CORLIB_DIR=""
    export DOORSTOP_REDIRECT_OUTPUT_LOG=1
fi
log_bootstrap "doorstop_env_prepared root_loader_mode=$root_loader_mode DOORSTOP_ENABLE=$DOORSTOP_ENABLE DOORSTOP_ENABLED=$DOORSTOP_ENABLED DOORSTOP_INVOKE_DLL_PATH=$DOORSTOP_INVOKE_DLL_PATH DOORSTOP_TARGET_ASSEMBLY=$DOORSTOP_TARGET_ASSEMBLY DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=$DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE DOORSTOP_REDIRECT_OUTPUT_LOG=$DOORSTOP_REDIRECT_OUTPUT_LOG"

can_retry_x64=false
if echo "$executable_type" | grep -q "x86_64" && echo "$root_doorstop_type" | grep -q "x86_64"; then
    can_retry_x64=true
fi

case $executable_type in
    *arm64*)
        if [ "$native_macos_arch" = "arm64" ] && [ -f "$root_doorstop_dylib" ] && echo "$root_doorstop_type" | grep -q "arm64"; then
            arch="arm64"
        else
            arch="x64"
        fi
        ;;
    *64-bit*)
        arch="x64"
        ;;
    *32-bit*|*i386*)
        arch="x86"
        ;;
    *)
        log_bootstrap "unsupported_executable_type=$executable_type"
        echo "Cannot identify executable type: $executable_type"
        exit 1
        ;;
esac

if [ "$launch_entry_uses_wrapper" = "1" ] && [ "$native_macos_arch" = "arm64" ]; then
    arch="x64"
    log_bootstrap "forcing_x64_for_wrapper_game launch_entry=$launch_entry_path"
fi

doorstop_libname="libdoorstop_${{arch}}.dylib"
doorstop_dylib="$BASEDIR/doorstop_libs/${{doorstop_libname}}"
doorstop_libs="$BASEDIR/doorstop_libs"

if [ "$arch" = "arm64" ] && [ -f "$root_doorstop_dylib" ]; then
    doorstop_dylib="$root_doorstop_dylib"
    doorstop_libs="$BASEDIR"
elif [ ! -f "$doorstop_dylib" ] && [ -f "$root_doorstop_dylib" ]; then
    doorstop_dylib="$root_doorstop_dylib"
    doorstop_libs="$BASEDIR"
fi

log_bootstrap "selected_runtime_arch=$arch doorstop_dylib=$doorstop_dylib"

if [ ! -f "$doorstop_dylib" ]; then
    log_bootstrap "doorstop_dylib_missing"
    echo "Cannot find Doorstop library: $doorstop_dylib"
    exit 1
fi

if [ "$runtime_disabled" = true ]; then
    if [ "$steam_launch_args_ready" = true ]; then
        log_bootstrap "steam_launch_exec_vanilla argv=$*"
        exec "$@"
    fi
    if [ "$launch_entry_uses_wrapper" = "1" ]; then
        log_bootstrap "exec_vanilla_wrapper=$launch_entry_path"
        exec /bin/bash "${{launch_entry_path}}"
    fi
    log_bootstrap "exec_vanilla=$executable_path"
    exec "${{executable_path}}"
fi

if [ "$root_loader_mode" = true ]; then
    export LD_LIBRARY_PATH="$BASEDIR:${{LD_LIBRARY_PATH}}"
    if [ -z "${{LD_PRELOAD:-}}" ]; then
        export LD_PRELOAD="libdoorstop.dylib"
    else
        export LD_PRELOAD="libdoorstop.dylib:${{LD_PRELOAD}}"
    fi

    if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
        export DYLD_LIBRARY_PATH="$BASEDIR:${{DYLD_LIBRARY_PATH}}"
    else
        export DYLD_LIBRARY_PATH="$BASEDIR"
    fi

    if [ -n "${{DYLD_INSERT_LIBRARIES:-}}" ]; then
        export DYLD_INSERT_LIBRARIES="libdoorstop.dylib:${{DYLD_INSERT_LIBRARIES}}"
    else
        export DYLD_INSERT_LIBRARIES="libdoorstop.dylib"
    fi
else
    export LD_LIBRARY_PATH="${{doorstop_libs}}:${{LD_LIBRARY_PATH}}"
    export LD_PRELOAD="${{doorstop_dylib}}:${{LD_PRELOAD}}"

    # r2modmac: preserve Steam-provided DYLD hooks so the Steam Overlay keeps working.
    if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
        export DYLD_LIBRARY_PATH="${{doorstop_libs}}:${{DYLD_LIBRARY_PATH}}"
    else
        export DYLD_LIBRARY_PATH="${{doorstop_libs}}"
    fi

    if [ -n "${{DYLD_INSERT_LIBRARIES:-}}" ]; then
        export DYLD_INSERT_LIBRARIES="${{doorstop_dylib}}:${{DYLD_INSERT_LIBRARIES}}"
    else
        export DYLD_INSERT_LIBRARIES="${{doorstop_dylib}}"
    fi
fi

if [ "$write_debug_logs" = "1" ]; then
    export DYLD_PRINT_LIBRARIES=1
    export DYLD_PRINT_TO_FILE="$dyld_log"
fi

log_bootstrap "loader_env LD_LIBRARY_PATH=${{LD_LIBRARY_PATH:-}} LD_PRELOAD=${{LD_PRELOAD:-}} DYLD_LIBRARY_PATH=${{DYLD_LIBRARY_PATH:-}} DYLD_INSERT_LIBRARIES=${{DYLD_INSERT_LIBRARIES:-}} DYLD_PRINT_TO_FILE=${{DYLD_PRINT_TO_FILE:-}}"

# r2modmac: prepare Steam runtime emulation wrappers used by some manual macOS builds
# (for example launchers that ship steam_appid.txt + ipcserver in Contents/MacOS).
if [ "$launch_entry_uses_wrapper" = "1" ]; then
    steamemu_macos_dir=$(dirname "$launch_entry_path")
else
    steamemu_macos_dir="$BASEDIR/MacOS"
fi
steamemu_appid_file="$steamemu_macos_dir/steam_appid.txt"
if [ -x "$steamemu_macos_dir/ipcserver" ] && [ -f "$steamemu_appid_file" ]; then
    steamemu_app_id=$(tr -d '[:space:]' < "$steamemu_appid_file" 2>/dev/null)
    if [ -n "$steamemu_app_id" ]; then
        export SteamAppId="$steamemu_app_id"
        export SteamGameId="$steamemu_app_id"
    fi

    steamemu_config_dir="$BASEDIR/../Config"
    if [ -d "$steamemu_config_dir" ]; then
        export STEAMEMU_SETTINGS_DIR="$steamemu_config_dir"
    fi

    if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
        export DYLD_LIBRARY_PATH="$steamemu_macos_dir:${{DYLD_LIBRARY_PATH}}"
    else
        export DYLD_LIBRARY_PATH="$steamemu_macos_dir"
    fi

    launchctl remove com.valvesoftware.steam.ipctool >/dev/null 2>&1 || true
    pkill ipcserver >/dev/null 2>&1 || true
    if [ -x "$steamemu_macos_dir/reset" ]; then
        "$steamemu_macos_dir/reset" >/dev/null 2>&1 || true
    fi
    "$steamemu_macos_dir/ipcserver" >/dev/null 2>&1 &
    steamemu_ipc_pid=$!
    log_bootstrap "steamemu_runtime_prepared app_id=$steamemu_app_id ipcpid=$steamemu_ipc_pid settings_dir=${{STEAMEMU_SETTINGS_DIR:-}}"
fi

maybe_retry_x64_after_arm64_failure() {{
    failed_mode="$1"
    failed_status="$2"
    shift 2
    bepinex_log="$BASEDIR/BepInEx/LogOutput.log"

    if [ "$failed_status" = "0" ]; then
        return 1
    fi

    if [ "$arch" != "arm64" ] || [ "$can_retry_x64" != true ]; then
        return 1
    fi

    if [ -s "$dyld_log" ] || [ -f "$bepinex_log" ]; then
        return 1
    fi

    log_bootstrap "${{failed_mode}}_retrying_x64_fallback status=${{failed_status}}"
    printf '[%s] %s_retrying_x64_fallback status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$failed_mode" "$failed_status" >> "$exec_log"
    /usr/bin/arch -x86_64 \
        -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
        -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
        -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
        -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
        -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
        -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
        -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
        -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
        -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
        -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
        -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
        -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
        -e SteamAppId="${{SteamAppId:-}}" \
        -e SteamGameId="${{SteamGameId:-}}" \
        -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
        -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
        -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
        -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
        -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
        -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
        -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
        "$@" >> "$exec_log" 2>&1
    retry_status=$?
    log_bootstrap "${{failed_mode}}_x64_fallback_failed status=${{retry_status}}"
    printf '[%s] %s_x64_fallback_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$failed_mode" "$retry_status" >> "$exec_log"
    exit "$retry_status"
}}

if [ "$steam_launch_args_ready" = true ]; then
    if [ "$arch" = "arm64" ] && [ "$wrapper_arch" = "arm64" ] && [ "$wrapper_translated" = "0" ]; then
        log_bootstrap "steam_launch_exec_modded_arm64_direct argv=$*"
        "$@" >> "$exec_log" 2>&1
        exec_status=$?
        log_bootstrap "steam_launch_exec_modded_arm64_direct_failed status=$exec_status"
        printf '[%s] steam_launch_exec_modded_arm64_direct_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        maybe_retry_x64_after_arm64_failure "steam_launch_exec_modded_arm64_direct" "$exec_status" "$@"
        exit "$exec_status"
    fi
    if [ "$arch" = "arm64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
        log_bootstrap "steam_launch_exec_modded_arm64_env argv=$*"
        /usr/bin/arch -arm64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            "$@" >> "$exec_log" 2>&1
        exec_status=$?
        log_bootstrap "steam_launch_exec_modded_arm64_env_failed status=$exec_status"
        printf '[%s] steam_launch_exec_modded_arm64_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        maybe_retry_x64_after_arm64_failure "steam_launch_exec_modded_arm64_env" "$exec_status" "$@"
        exit "$exec_status"
    fi
    if [ "$arch" = "x64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
        log_bootstrap "steam_launch_exec_modded_arch_env argv=$*"
        /usr/bin/arch -x86_64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            "$@" >> "$exec_log" 2>&1
        exec_status=$?
        log_bootstrap "steam_launch_exec_modded_arch_env_failed status=$exec_status"
        printf '[%s] steam_launch_exec_modded_arch_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        exit "$exec_status"
    fi
    log_bootstrap "steam_launch_exec_modded argv=$*"
    "$@" >> "$exec_log" 2>&1
    exec_status=$?
    log_bootstrap "steam_launch_exec_modded_failed status=$exec_status"
    printf '[%s] steam_launch_exec_modded_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    exit "$exec_status"
fi

if [ "$arch" = "arm64" ] && [ "$wrapper_arch" = "arm64" ] && [ "$wrapper_translated" = "0" ]; then
    log_bootstrap "exec_modded_arm64_direct=$executable_path"
    "${{executable_path}}" >> "$exec_log" 2>&1
    exec_status=$?
    log_bootstrap "exec_modded_arm64_direct_failed status=$exec_status"
    printf '[%s] exec_modded_arm64_direct_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    set -- "${{executable_path}}"
    maybe_retry_x64_after_arm64_failure "exec_modded_arm64_direct" "$exec_status" "${{executable_path}}"
    exit "$exec_status"
fi

if [ "$arch" = "arm64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
    log_bootstrap "exec_modded_arm64_env=$executable_path"
    /usr/bin/arch -arm64 \
        -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
        -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
        -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
        -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
        -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
        -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
        -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
        -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
        -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
        -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
        -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
        -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
        -e SteamAppId="${{SteamAppId:-}}" \
        -e SteamGameId="${{SteamGameId:-}}" \
        -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
        -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
        -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
        -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
        -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
        -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
        -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
        "${{executable_path}}" >> "$exec_log" 2>&1
    exec_status=$?
    log_bootstrap "exec_modded_arm64_env_failed status=$exec_status"
    printf '[%s] exec_modded_arm64_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    set -- "${{executable_path}}"
    maybe_retry_x64_after_arm64_failure "exec_modded_arm64_env" "$exec_status" "${{executable_path}}"
    exit "$exec_status"
fi

if [ "$arch" = "x64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
    log_bootstrap "exec_modded_arch_env=$executable_path"
    /usr/bin/arch -x86_64 \
        -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
        -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
        -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
        -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
        -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
        -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
        -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
        -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
        -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
        -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
        -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
        -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
        -e SteamAppId="${{SteamAppId:-}}" \
        -e SteamGameId="${{SteamGameId:-}}" \
        -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
        -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
        -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
        -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
        -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
        -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
        -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
        "${{executable_path}}" >> "$exec_log" 2>&1
    exec_status=$?
    log_bootstrap "exec_modded_arch_env_failed status=$exec_status"
    printf '[%s] exec_modded_arch_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    exit "$exec_status"
fi

log_bootstrap "exec_modded=$executable_path"
"${{executable_path}}" >> "$exec_log" 2>&1
exec_status=$?
log_bootstrap "exec_modded_failed status=$exec_status"
printf '[%s] exec_modded_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
exit "$exec_status"
"#
        )
    }

    if !script_path.exists() {
        return Ok(());
    }

    let executable_path = resolve_macos_executable_path(game_path)?;
    let executable_bundle = find_macos_app_bundle(game_path);
    let launch_entry_path = resolve_macos_launch_entry_path(game_path)?;
    let executable_entry = executable_bundle.as_deref().unwrap_or(executable_path.as_path());
    let relative_executable = executable_entry
        .strip_prefix(game_path)
        .ok()
        .unwrap_or(executable_entry)
        .to_string_lossy()
        .replace('\\', "/");
    let relative_launch_entry = launch_entry_path
        .strip_prefix(game_path)
        .ok()
        .unwrap_or(launch_entry_path.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    let launch_entry_uses_wrapper = launch_entry_path != executable_path;

    let mut script = fs::read_to_string(script_path).map_err(|e| e.to_string())?;
    let original = script.clone();
    script = script.replace("\r\n", "\n");

    // CRITICAL: detect if the script has a working runtime_disabled early-exit BEFORE
    // DYLD_INSERT_LIBRARIES. Original BepInEx scripts set the runtime_disabled variable but
    // never skip loading Doorstop when it's disabled. This causes a DYLD crash because
    // doorstop_libs is renamed to doorstop_libs_DISABLED in vanilla mode and the dylib
    // path no longer resolves. Always regenerate if the early-exit is absent or misplaced.
    let has_early_exit = script.contains("if [ \"$runtime_disabled\" = true ]");
    let early_exit_before_dyld = if has_early_exit {
        let exit_pos = script.find("if [ \"$runtime_disabled\" = true ]").unwrap_or(usize::MAX);
        let dyld_pos = script.find("DYLD_INSERT_LIBRARIES=").unwrap_or(usize::MAX);
        exit_pos < dyld_pos
    } else {
        false
    };

    let has_root_doorstop_fallback =
        script.contains("doorstop_dylib=") && script.contains("$BASEDIR/libdoorstop.dylib");
    let has_steam_arg_helper = script.contains("steam_arg_helper()");
    let has_root_bootstrap_log =
        script.contains("bootstrap_log=\"$BASEDIR/r2modmac_bootstrap.log\"");
    let has_expected_debug_log_setting = script.contains(&format!(
        "write_debug_logs={}",
        if write_debug_logs_to_game { 1 } else { 0 }
    ));
    let removes_codesign_signature = script.contains("codesign_remove_signature_attempt");
    let logs_loader_environment = script.contains("wrapper_arch=") && script.contains("loader_env LD_LIBRARY_PATH=");
    let has_root_loader_mode_env = script.contains("root_loader_mode=false")
        && script.contains("DOORSTOP_IGNORE_DISABLED_ENV=0")
        && script.contains("DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=\"$BASEDIR/BepInEx/core\"")
        && script.contains("-e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=")
        && script.contains("root_loader_mode=$root_loader_mode")
        && script.contains("export DYLD_INSERT_LIBRARIES=\"libdoorstop.dylib");
    let has_arch_env_exec = script.contains("exec_modded_arch_env=")
        && script.contains("exec_modded_arm64_env=")
        && script.contains("native_macos_arch=")
        && script.contains("/usr/bin/arch -arm64")
        && script.contains("-e DYLD_INSERT_LIBRARIES=");
    let has_dyld_loader_logging = script.contains("DYLD_PRINT_LIBRARIES=1")
        && script.contains("dyld_log=\"$BASEDIR/r2modmac_dyld.log\"");
    let has_exec_failure_logging = script.contains("exec_log=\"$BASEDIR/r2modmac_exec.log\"")
        && script.contains("exec_modded_arm64_env_failed status=$exec_status")
        && script.contains("steam_launch_exec_modded_arch_env_failed status=$exec_status");
    let has_native_arm64_direct_exec = script.contains("steam_launch_exec_modded_arm64_direct argv=$*")
        && script.contains("steam_launch_exec_modded_arm64_direct_failed status=$exec_status")
        && script.contains("exec_modded_arm64_direct=$executable_path")
        && script.contains("exec_modded_arm64_direct_failed status=$exec_status")
        && script.contains("[ \"$wrapper_arch\" = \"arm64\" ]")
        && script.contains("[ \"$wrapper_translated\" = \"0\" ]");
    let has_arm64_x64_fallback_retry = script.contains("maybe_retry_x64_after_arm64_failure()")
        && script.contains("can_retry_x64=true")
        && script.contains("retrying_x64_fallback status=")
        && script.contains("x64_fallback_failed status=$retry_status");
    let has_modern_doorstop_env_aliases = script.contains("DOORSTOP_ENABLED=1")
        && script.contains("DOORSTOP_TARGET_ASSEMBLY=")
        && script.contains("DOORSTOP_BOOT_CONFIG_OVERRIDE=")
        && script.contains("DOORSTOP_IGNORE_DISABLED_ENV=0")
        && script.contains("DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=\"$BASEDIR/BepInEx/core\"")
        && script.contains("DOORSTOP_REDIRECT_OUTPUT_LOG=1")
        && script.contains("-e DOORSTOP_TARGET_ASSEMBLY=")
        && script.contains("-e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=");
    let has_launch_entry_support = script.contains("launch_entry_name=")
        && script.contains("launch_entry_uses_wrapper=")
        && script.contains("resolved_launch_entry_path=");
    let has_steamemu_runtime_prep =
        script.contains("steamemu_runtime_prepared")
            && script.contains("export SteamAppId=")
            && script.contains("export SteamGameId=")
            && script.contains("steamemu_macos_dir=\"$BASEDIR/MacOS\"")
            && script.contains("-e SteamAppId=");
    let preserves_steam_dyld_hooks =
        script.contains("r2modmac: preserve Steam-provided DYLD hooks")
            && script.contains("DYLD_INSERT_LIBRARIES=\"${doorstop_dylib}:${DYLD_INSERT_LIBRARIES}\"");
    let steam_launch_exec_deferred =
        script.contains("steam_launch_args_ready=true")
            && script.contains("steam_launch_exec_modded argv=$*");
    let has_legacy_bepinex_bootstrap_log =
        script.contains("bootstrap_log=\"$BASEDIR/BepInEx/r2modmac_bootstrap.log\"")
            || script.contains("mkdir -p \"$BASEDIR/BepInEx\"");
    let steam_launch_pos = script.find("for a in \"$@\"").unwrap_or(usize::MAX);
    let doorstop_export_pos = script.find("DOORSTOP_INVOKE_DLL_PATH=").unwrap_or(usize::MAX);
    let steam_launch_order_ok = has_steam_arg_helper && doorstop_export_pos < steam_launch_pos;
    let needs_regeneration =
        !has_macos_doorstop_support(&script)
            || !early_exit_before_dyld
            || !has_root_doorstop_fallback
            || !steam_launch_order_ok
            || !has_root_bootstrap_log
            || !removes_codesign_signature
            || !logs_loader_environment
            || !has_root_loader_mode_env
            || !has_arch_env_exec
            || !has_dyld_loader_logging
            || !has_exec_failure_logging
            || !has_native_arm64_direct_exec
            || !has_arm64_x64_fallback_retry
            || !has_modern_doorstop_env_aliases
            || !has_launch_entry_support
            || !has_steamemu_runtime_prep
            || !preserves_steam_dyld_hooks
            || !steam_launch_exec_deferred
            || !has_expected_debug_log_setting
            || has_legacy_bepinex_bootstrap_log;
    if needs_regeneration {
        eprintln!(
            "[configure_macos_bepinex_script] Regenerating script (has_doorstop={} early_exit_ok={} root_fallback_ok={} steam_launch_order_ok={} root_bootstrap_log_ok={} removes_codesign_signature={} logs_loader_environment={} has_arch_env_exec={} has_dyld_loader_logging={} has_exec_failure_logging={} modern_doorstop_env_aliases={} launch_entry_support={} steamemu_runtime_prep={} preserves_steam_dyld_hooks={} steam_launch_exec_deferred={} legacy_bepinex_bootstrap_log={}).",
            has_macos_doorstop_support(&script),
            early_exit_before_dyld,
            has_root_doorstop_fallback,
            steam_launch_order_ok,
            has_root_bootstrap_log,
            removes_codesign_signature,
            logs_loader_environment,
            has_arch_env_exec,
            has_dyld_loader_logging,
            has_exec_failure_logging,
            has_modern_doorstop_env_aliases,
            has_launch_entry_support,
            has_steamemu_runtime_prep,
            preserves_steam_dyld_hooks,
            steam_launch_exec_deferred,
            has_legacy_bepinex_bootstrap_log
        );
        script = build_generated_macos_bepinex_script(
            &relative_executable,
            &relative_launch_entry,
            launch_entry_uses_wrapper,
            write_debug_logs_to_game,
        );
    } else {
        if let Ok(executable_re) = regex::Regex::new(r#"(?m)^executable_name=".*"$"#) {
            script = executable_re
                .replace(&script, format!("executable_name=\"{}\"", relative_executable))
                .into_owned();
        }
        if let Ok(launch_entry_re) = regex::Regex::new(r#"(?m)^launch_entry_name=".*"$"#) {
            script = launch_entry_re
                .replace(
                    &script,
                    format!("launch_entry_name=\"{}\"", relative_launch_entry),
                )
                .into_owned();
        }
        if let Ok(wrapper_flag_re) = regex::Regex::new(r#"(?m)^launch_entry_uses_wrapper=.*$"#) {
            script = wrapper_flag_re
                .replace(
                    &script,
                    format!(
                        "launch_entry_uses_wrapper={}",
                        if launch_entry_uses_wrapper { 1 } else { 0 }
                    ),
                )
                .into_owned();
        }
    }

    if let Some(idx) = script.find("BASEDIR=") {
        let insert_after = script[idx..]
            .find('\n')
            .map(|offset| idx + offset + 1)
            .unwrap_or(script.len());

        if !script.contains("cd \"$BASEDIR\"") {
            script.insert_str(
                insert_after,
                "cd \"$BASEDIR\" # r2modmac: run from game directory for macOS compatibility\n",
            );
        }
    }

    if !script.contains("r2modmac: if the runtime is marked disabled") {
        let runtime_disabled_block = "\n# r2modmac: if the runtime is marked disabled, launch the game without Doorstop.\nruntime_disabled=false\nif [ -e \"$BASEDIR/BepInEx_DISABLED\" ] || [ -e \"$BASEDIR/doorstop_libs_DISABLED\" ] || [ -e \"$BASEDIR/libdoorstop.dylib_DISABLED\" ] || [ -e \"$BASEDIR/doorstop_config.ini_DISABLED\" ]; then\n    runtime_disabled=true\nfi\n\nif [ \"$runtime_disabled\" = true ]; then\n    exec \"${{executable_path}}\"\nfi\n";
        if let Some(idx) = script.find("BASEDIR=") {
            let insert_at = script[idx..]
                .find('\n')
                .map(|offset| idx + offset + 1)
                .unwrap_or(script.len());
            script.insert_str(insert_at, runtime_disabled_block);
        } else {
            script.push_str(runtime_disabled_block);
        }
    }

    script = script.replace(
        r#"if ! echo "$real_executable_name" | grep "^.*\.app/Contents/MacOS/.*";"#,
        r#"if ! echo "$real_executable_name" | grep "^.*/Contents/MacOS/.*";"#,
    );

    if !script.contains("com.apple.quarantine") {
        let dequarantine_block = "\n# r2modmac: best-effort de-quarantine for Doorstop/BepInEx payloads\nif command -v xattr >/dev/null 2>&1; then\n  /usr/bin/xattr -d com.apple.quarantine \"$BASEDIR/run_bepinex.sh\" \"$BASEDIR/doorstop_libs\" \"$BASEDIR/BepInEx\" \"$BASEDIR\"/*.dylib 2>/dev/null || true\nfi\n";
        if let Some(idx) = script.find("BASEDIR=") {
            let insert_at = script[idx..]
                .find('\n')
                .map(|offset| idx + offset + 1)
                .unwrap_or(script.len());
            script.insert_str(insert_at, dequarantine_block);
        } else {
            script.push_str(dequarantine_block);
        }
    }

    if script != original {
        fs::write(script_path, script).map_err(|e| e.to_string())?;
    }

    let legacy_bootstrap_log = game_path.join("BepInEx").join("r2modmac_bootstrap.log");
    if legacy_bootstrap_log.exists() {
        let _ = fs::remove_file(&legacy_bootstrap_log);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(script_path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(script_path, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn resolve_macos_app_executable_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    find_macos_executable_path(game_path)
}

fn macho_file_supports_arm64(path: &std::path::Path) -> bool {
    let Ok(output) = std::process::Command::new("/usr/bin/file")
        .arg("-b")
        .arg(path)
        .output()
    else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    String::from_utf8_lossy(&output.stdout)
        .to_lowercase()
        .contains("arm64")
}

fn is_apple_silicon_host() -> bool {
    if std::env::consts::ARCH == "aarch64" {
        return true;
    }

    let Ok(output) = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-in", "sysctl.proc_translated"])
        .output()
    else {
        return false;
    };

    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1"
}

fn should_use_native_macos_bepinex_launcher(game_path: &std::path::Path) -> bool {
    if !is_apple_silicon_host() {
        return false;
    }

    let runtime_root = resolve_macos_runtime_root(game_path);
    let Some(executable_path) = resolve_macos_app_executable_path(game_path) else {
        return false;
    };

    let root_doorstop = runtime_root.join("libdoorstop.dylib");
    root_doorstop.is_file()
        && macho_file_supports_arm64(&executable_path)
        && macho_file_supports_arm64(&root_doorstop)
}

pub(crate) fn ensure_macos_steam_launch_options(
    app: &AppHandle,
    game_path: &std::path::Path,
    enable_mods: bool,
    relaunch_steam_after_update: bool,
) -> Result<(), String> {
    let ensure_started = std::time::Instant::now();
    eprintln!(
        "[ensure_macos_steam_launch_options] start enable_mods={} relaunch_after_update={} game_path={}",
        enable_mods,
        relaunch_steam_after_update,
        game_path.display()
    );
    let steam_roots = get_steam_roots_for_platform(app, false);
    if steam_roots.is_empty() {
        return Err("No Steam installation found to configure macOS launch options".to_string());
    }

    let managed_launch_option = if enable_mods {
        Some(managed_macos_launch_option_for_game(game_path)?)
    } else {
        None
    };

    let mut matched_steam_root: Option<std::path::PathBuf> = None;
    let mut app_id: Option<String> = None;
    for steam_root in &steam_roots {
        if let Some(found_app_id) = find_steam_app_id_for_game_path(steam_root, game_path) {
            matched_steam_root = Some(steam_root.clone());
            app_id = Some(found_app_id);
            break;
        }
    }

    if app_id.is_none() {
        if let Some(library_root) = find_embedded_steam_library_root_for_game_path(game_path) {
            app_id = find_steam_app_id_for_library_root(&library_root, game_path);
        }
    }

    let app_id = app_id.ok_or_else(|| {
        "Couldn't determine the Steam app ID for this macOS game. Automatic launch option setup failed.".to_string()
    })?;

    let steam_root_for_config = matched_steam_root
        .or_else(|| {
            steam_roots
                .iter()
                .find(|root| get_latest_localconfig_path(root).is_some())
                .cloned()
        })
        .or_else(|| steam_roots.first().cloned())
        .ok_or_else(|| "No Steam installation found to configure macOS launch options".to_string())?;

    let localconfig_paths = get_all_localconfig_paths(&steam_root_for_config);
    if localconfig_paths.is_empty() {
        return Err(
            "Couldn't locate Steam's localconfig.vdf for automatic macOS launch option setup."
                .to_string(),
        );
    }

    eprintln!(
        "[ensure_macos_steam_launch_options] app_id={} localconfigs={}",
        app_id,
        localconfig_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let legacy_backup_key = format!("steam::{}", app_id);
    let mut settings = load_settings_impl(app);
    let desired = if enable_mods {
        managed_launch_option.as_deref()
    } else {
        None
    };
    let mut settings_changed = false;
    let mut processed_localconfig = false;
    let mut steam_was_running: Option<bool> = None;
    let mut ensure_steam_stopped = || -> Result<(), String> {
        if steam_was_running.is_none() {
            let stop_started = std::time::Instant::now();
            if is_steam_app_running_on_macos() {
                emit_steam_launch_options_restart_event(app);
            }
            steam_was_running = Some(quit_steam_if_running(&steam_roots)?);
            eprintln!(
                "[ensure_macos_steam_launch_options] ensure_steam_stopped elapsed_ms={} steam_was_running={}",
                stop_started.elapsed().as_millis(),
                steam_was_running.unwrap_or(false)
            );
        }
        Ok(())
    };

    for localconfig_path in localconfig_paths {
        let localconfig_started = std::time::Instant::now();
        let localconfig = match fs::read_to_string(&localconfig_path) {
            Ok(localconfig) => localconfig,
            Err(error) => {
                eprintln!(
                    "[ensure_macos_steam_launch_options] skipping unreadable localconfig {}: {}",
                    localconfig_path.display(),
                    error
                );
                continue;
            }
        };

        let scoped_backup_key = format!(
            "steam::{}::{}",
            canonicalize_or_original(&localconfig_path).to_string_lossy(),
            app_id
        );
        let (updated_text, current_launch_options) = match update_launch_options_in_localconfig(
            &localconfig,
            &app_id,
            desired,
        ) {
            Ok(result) => result,
            Err(error) => {
                eprintln!(
                    "[ensure_macos_steam_launch_options] skipping localconfig {}: {}",
                    localconfig_path.display(),
                    error
                );
                continue;
            }
        };

        processed_localconfig = true;

        if enable_mods {
            let expected = managed_launch_option
                .as_ref()
                .ok_or_else(|| "Managed macOS launch option was not generated".to_string())?;
            let staged_value = get_launch_options_for_app(&updated_text, &app_id)
                .ok_or_else(|| format!("Failed to stage Steam launch options for app {}", app_id))?;
            if staged_value != *expected {
                return Err(format!(
                    "Failed to stage Steam launch options for app {} in {}. Expected {:?}, got {:?}",
                    app_id,
                    localconfig_path.display(),
                    expected,
                    staged_value
                ));
            }

            if let Some(current) = current_launch_options.as_ref() {
                if !current.trim().is_empty()
                    && !is_managed_macos_launch_option_for_game(current, game_path)
                    && !settings
                        .steam_launch_option_backups
                        .contains_key(&scoped_backup_key)
                {
                    settings
                        .steam_launch_option_backups
                        .insert(scoped_backup_key.clone(), current.clone());
                    settings_changed = true;
                }
            }

            if updated_text != localconfig {
                let write_started = std::time::Instant::now();
                ensure_steam_stopped()?;
                fs::write(&localconfig_path, updated_text)
                    .map_err(|e| format!("Failed to update Steam launch options: {}", e))?;
                let persisted = fs::read_to_string(&localconfig_path)
                    .map_err(|e| format!("Failed to verify updated Steam launch options: {}", e))?;
                if get_launch_options_for_app(&persisted, &app_id).as_deref()
                    != Some(expected.as_str())
                {
                    return Err(format!(
                        "Steam launch options were not persisted for app {} in {}",
                        app_id,
                        localconfig_path.display()
                    ));
                }
                eprintln!(
                    "[ensure_macos_steam_launch_options] updated localconfig={} elapsed_ms={}",
                    localconfig_path.display(),
                    write_started.elapsed().as_millis()
                );
            }
        } else if let Some(previous) = settings
            .steam_launch_option_backups
            .remove(&scoped_backup_key)
            .or_else(|| settings.steam_launch_option_backups.remove(&legacy_backup_key))
        {
            let (restored_text, _) =
                update_launch_options_in_localconfig(&localconfig, &app_id, Some(&previous))?;
            if restored_text != localconfig {
                let write_started = std::time::Instant::now();
                ensure_steam_stopped()?;
                fs::write(&localconfig_path, restored_text)
                    .map_err(|e| format!("Failed to restore Steam launch options: {}", e))?;
                let persisted = fs::read_to_string(&localconfig_path)
                    .map_err(|e| format!("Failed to verify restored Steam launch options: {}", e))?;
                if get_launch_options_for_app(&persisted, &app_id).as_deref()
                    != Some(previous.as_str())
                {
                    return Err(format!(
                        "Steam launch options were not restored correctly for app {} in {}",
                        app_id,
                        localconfig_path.display()
                    ));
                }
                eprintln!(
                    "[ensure_macos_steam_launch_options] restored localconfig={} elapsed_ms={}",
                    localconfig_path.display(),
                    write_started.elapsed().as_millis()
                );
            }
            settings_changed = true;
        } else if current_launch_options
            .as_deref()
            .map(|value| is_managed_macos_launch_option_for_game(value, game_path))
            .unwrap_or(false)
            && updated_text != localconfig
        {
            let write_started = std::time::Instant::now();
            ensure_steam_stopped()?;
            fs::write(&localconfig_path, updated_text)
                .map_err(|e| format!("Failed to clear Steam launch options: {}", e))?;
            let persisted = fs::read_to_string(&localconfig_path)
                .map_err(|e| format!("Failed to verify cleared Steam launch options: {}", e))?;
            if get_launch_options_for_app(&persisted, &app_id)
                .as_deref()
                .map(|value| is_managed_macos_launch_option_for_game(value, game_path))
                .unwrap_or(false)
            {
                return Err(format!(
                    "Managed Steam launch options are still present for app {} in {} after clearing",
                    app_id,
                    localconfig_path.display()
                ));
            }
            eprintln!(
                "[ensure_macos_steam_launch_options] cleared localconfig={} elapsed_ms={}",
                localconfig_path.display(),
                write_started.elapsed().as_millis()
            );
        }

        eprintln!(
            "[ensure_macos_steam_launch_options] processed localconfig={} total_elapsed_ms={}",
            localconfig_path.display(),
            localconfig_started.elapsed().as_millis()
        );
    }

    if !processed_localconfig {
        return Err(format!(
            "Couldn't update Steam launch options for app {} in any localconfig.vdf",
            app_id
        ));
    }

    if settings
        .steam_launch_option_backups
        .remove(&legacy_backup_key)
        .is_some()
    {
        settings_changed = true;
    }

    if settings_changed {
        save_settings_impl(app, &settings)?;
    }

    if steam_was_running.unwrap_or(false) {
        if relaunch_steam_after_update {
            eprintln!(
                "[ensure_macos_steam_launch_options] Steam was closed to update launch options; relaunching Steam now because no immediate steam://run launch follows."
            );
            let relaunch_started = std::time::Instant::now();
            relaunch_macos_steam_if_needed(&steam_root_for_config);
            eprintln!(
                "[ensure_macos_steam_launch_options] relaunch_requested elapsed_ms={}",
                relaunch_started.elapsed().as_millis()
            );
        } else {
            eprintln!(
                "[ensure_macos_steam_launch_options] Steam was closed to update launch options; leaving it closed so the upcoming steam://run launch starts Steam and the game together."
            );
        }
    }

    eprintln!(
        "[ensure_macos_steam_launch_options] done app_id={} total_elapsed_ms={}",
        app_id,
        ensure_started.elapsed().as_millis()
    );

    Ok(())
}

#[command]
pub async fn get_game_path(app: AppHandle, game_identifier: String, platform: Option<String>) -> Result<Option<String>, String> {
    let settings = load_settings_impl(&app);
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");
    let cache_key = if let Some(p) = platform {
        format!("{}::{}", game_identifier, p)
    } else {
        game_identifier.clone()
    };

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
            log_manual_override_once(&key, path);
            return Ok(Some(path.clone()));
        }
    }

    let steam_paths_to_check = get_steam_roots_for_platform(&app, is_windows_profile);

    if steam_paths_to_check.is_empty() {
        if is_windows_profile {
            return Err("No Windows Steam path configured. Go to Settings and set your Steam directory inside the Wine/CrossOver/Wineskin prefix, or set the game directory manually.".to_string());
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
                        if settings.game_paths.get(&cache_key) != Some(&game_path) {
                            let mut updated_settings = settings.clone();
                            updated_settings
                                .game_paths
                                .insert(cache_key.clone(), game_path.clone());
                            let _ = save_settings_impl(&app, &updated_settings);
                        }
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
pub async fn get_game_source(app: AppHandle, game_identifier: String, platform: Option<String>) -> Result<String, String> {
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");

    let Some(game_path_str) = get_game_path(app.clone(), game_identifier, platform.map(|p| p.to_string())).await? else {
        return Ok("unknown".to_string());
    };

    Ok(infer_distribution_from_game_path(
        &app,
        std::path::Path::new(&game_path_str),
        is_windows_profile,
    ))
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
    let Some(game_path_str) = get_game_path(app.clone(), game_identifier, platform).await? else {
        return Err("Game directory not found".to_string());
    };

    open::that(std::path::Path::new(&game_path_str))
        .map_err(|e| format!("Failed to open game directory: {}", e))?;
    Ok(())
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
    if let Some(executable_path) = find_windows_executable_path(path) {
        return Ok(Some(executable_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

#[command]
pub async fn install_to_game(app: AppHandle, game_identifier: String, profile_id: String, disabled_mods: Vec<String>, is_vanilla_override: Option<bool>) -> Result<(), String> {
    let profile_platform = get_profile_platform(&app, &profile_id);
    let is_mac_profile = profile_platform == "mac";
    let settings = load_settings_impl(&app);
    let use_legacy_plugin_cache = settings.legacy_install_mode;

    // 1. Find game path
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), Some(profile_platform.clone())).await?
        .ok_or("Game path not found")?;
    let game_path = std::path::Path::new(&game_path_str);
    let effective_distribution = if is_mac_profile {
        infer_distribution_from_game_path(&app, game_path, false)
    } else {
        get_profile_distribution(&app, &profile_id)
    };
    let launch_mode = get_profile_launch_mode(&app, &profile_id);
    let manage_steam_launch =
        is_mac_profile && should_manage_steam_launch_options(&effective_distribution, &launch_mode);

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

    eprintln!(
        "[install_to_game] platform={} stored_distribution={} effective_distribution={} launch_mode={} manage_steam_launch={}",
        profile_platform,
        get_profile_distribution(&app, &profile_id),
        effective_distribution,
        launch_mode,
        manage_steam_launch
    );

    if is_vanilla {
        if is_mac_profile {
            eprintln!("[install_to_game] Profile is in VANILLA mode on macOS. Runtime will be disabled while the Steam wrapper stays in place.");
        } else {
            eprintln!("[install_to_game] Profile is in VANILLA mode. Cleaning game folder.");
        }
    }

    eprintln!("[install_to_game] Disabled mods: {:?}", disabled_mods);

    let is_balatro_profile = is_mac_profile
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));
    let runtime_game_path_buf = if is_mac_profile && !is_balatro_profile {
        let resolved = resolve_macos_runtime_root(game_path);
        if resolved != game_path {
            eprintln!(
                "[install_to_game] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    } else {
        game_path.to_path_buf()
    };
    let runtime_game_path = runtime_game_path_buf.as_path();
    let mac_runtime_present_before_sync = is_mac_profile
        && !is_vanilla
        && !is_balatro_profile
        && has_complete_macos_bepinex_runtime(runtime_game_path);

    if is_mac_profile && !is_vanilla && !is_balatro_profile {
        validate_macos_bepinex_support(runtime_game_path)?;
    }

    if is_balatro_profile {
        let mods_dir = get_balatro_mods_dir()
            .ok_or_else(|| "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string())?;
        let disabled_dir = balatro_mods_disabled_dir()?;

        if is_vanilla {
            if mods_dir.exists() {
                if disabled_dir.exists() {
                    let _ = fs::remove_dir_all(&disabled_dir);
                }
                fs::rename(&mods_dir, &disabled_dir)
                    .map_err(|e| format!("Failed to disable Balatro mods: {}", e))?;
            }
            eprintln!("[install_to_game] Balatro vanilla mode complete.");
            return Ok(());
        }

        if disabled_dir.exists() && !mods_dir.exists() {
            fs::rename(&disabled_dir, &mods_dir)
                .map_err(|e| format!("Failed to restore Balatro mods: {}", e))?;
        }

        if !has_balatro_lovely_runtime(game_path) {
            return Err("No macOS Lovely runtime found".to_string());
        }

        set_executable_if_present(&game_path.join(BALATRO_LOVELY_SCRIPT))?;
        dequarantine_recursive(game_path);
        if mods_dir.exists() {
            dequarantine_recursive(&mods_dir);
        }

        eprintln!("[install_to_game] Balatro sync complete!");
        return Ok(());
    }

    // --- FIX BEPINEX STRUCTURE START ---
    if !is_vanilla && use_legacy_plugin_cache {
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

                if let Some(run_script_src) = find_bepinex_script_in_dir(&pack_dir) {
                    let run_script_dst = profile_dir.join("run_bepinex.sh");
                    if !run_script_dst.exists() {
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
                } else {
                    eprintln!("[install_to_game] Cached BepInEx pack has no macOS script; relying on profile/game root runtime fallback");
                }

                // Also copy doorstop_config.ini for macOS
                let doorstop_cfg_src = pack_dir.join("doorstop_config.ini");
                let doorstop_cfg_dst = profile_dir.join("doorstop_config.ini");
                if doorstop_cfg_src.exists() && !doorstop_cfg_dst.exists() {
                    eprintln!("[install_to_game] Copying doorstop_config.ini (macOS) to profile root");
                    fs::copy(&doorstop_cfg_src, &doorstop_cfg_dst)
                        .map_err(|e| format!("Failed to copy doorstop_config.ini: {}", e))?;
                    normalize_macos_doorstop_config_file(&doorstop_cfg_dst)?;
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

    eprintln!(
        "[install_to_game] Installing profile {} to game {} (runtime root {})",
        profile_id,
        game_path.display(),
        runtime_game_path.display()
    );

    // --- SYNC: Remove mods from game that are not in profile OR are disabled ---
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");
    let game_plugins = runtime_game_path.join("BepInEx").join("plugins");

    // Create set of enabled mod names (lowercase for comparison)
    let disabled_set: std::collections::HashSet<String> = disabled_mods.iter()
        .map(|s| s.to_lowercase())
        .collect();

    if is_mac_profile && is_vanilla {
        if find_bepinex_script_in_dir(runtime_game_path).is_some() {
            let run_script = canonicalize_macos_bepinex_script(runtime_game_path)?;
            configure_macos_bepinex_script(
                &run_script,
                runtime_game_path,
                load_settings_impl(&app).write_debug_logs_to_game,
            )?;
            dequarantine_recursive(runtime_game_path);
        }
        // Vanilla/modded is toggled by the runtime_disabled mechanism in
        // run_bepinex.sh. Do not touch Steam launch options here, otherwise a
        // profile disable would unexpectedly quit Steam before the user presses Play.
        sync_macos_runtime_disabled_state(runtime_game_path, true)?;
        eprintln!(
            "[install_to_game] macOS vanilla mode complete - runtime disabled while preserving the Steam wrapper."
        );
        return Ok(());
    }

    if !is_mac_profile && is_vanilla {
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
    } else if !is_mac_profile {
        // Normal mode: Check if BepInEx_DISABLED exists and restore it
        let bepinex_folder = game_path.join("BepInEx");
        let bepinex_disabled = game_path.join("BepInEx_DISABLED");

        if bepinex_disabled.exists() && !bepinex_folder.exists() {
            eprintln!("[install_to_game] Restoring BepInEx_DISABLED -> BepInEx");
            fs::rename(&bepinex_disabled, &bepinex_folder)
                .map_err(|e| format!("Failed to restore BepInEx: {}", e))?;
        }
    }

    if use_legacy_plugin_cache && !is_vanilla && profile_plugins.exists() && game_plugins.exists() {
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

    if use_legacy_plugin_cache && !is_vanilla {
    // 3. Copy BepInEx structure with filtering for disabled mods
    let source_bepinex = profile_dir.join("BepInEx");
    let dest_bepinex = runtime_game_path.join("BepInEx");

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
                } else if is_mac_profile && mac_runtime_present_before_sync {
                    eprintln!(
                        "[install_to_game] Keeping existing macOS runtime payload: {}",
                        name
                    );
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

    // 4. Sync platform-specific root payloads
    if is_mac_profile {
        if !is_vanilla && !is_balatro_profile {
            sync_macos_runtime_disabled_state(runtime_game_path, false)?;
            ensure_macos_bepinex_runtime_present(&app, &profile_id, runtime_game_path).await?;
        }

        for root in [game_path, runtime_game_path] {
            let leaked_windows_loader = root.join("winhttp.dll");
            if leaked_windows_loader.exists() {
                let _ = fs::remove_file(&leaked_windows_loader);
            }
        }

        let root_items = [
            ("doorstop_libs", true),
            ("doorstop_config.ini", false),
            ("libdoorstop.dylib", false),
        ];

        for (item_name, is_dir) in root_items {
            let source = profile_dir.join(item_name);
            let dest = runtime_game_path.join(item_name);

            if !source.exists() {
                continue;
            }

            if mac_runtime_present_before_sync {
                eprintln!(
                    "[install_to_game] Keeping existing macOS root payload: {}",
                    item_name
                );
                continue;
            }

            if is_dir {
                copy_dir_recursive(&source, &dest)
                    .map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
            } else {
                if dest.exists() {
                    let _ = fs::remove_file(&dest);
                }
                fs::copy(&source, &dest).map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
            }

            eprintln!("[install_to_game] Synced {} to game folder", item_name);
        }

        if let Some(source_script) = find_bepinex_script_in_dir(&profile_dir) {
            let dest_script = runtime_game_path.join(CANONICAL_MAC_BEPINEX_SCRIPT);
            if mac_runtime_present_before_sync {
                eprintln!(
                    "[install_to_game] Keeping existing macOS launcher script: {}",
                    CANONICAL_MAC_BEPINEX_SCRIPT
                );
            } else {
                if dest_script.exists() {
                    let _ = fs::remove_file(&dest_script);
                }
                fs::copy(&source_script, &dest_script)
                    .map_err(|e| format!("Failed to copy {}: {}", CANONICAL_MAC_BEPINEX_SCRIPT, e))?;
                eprintln!("[install_to_game] Synced {} to game folder", CANONICAL_MAC_BEPINEX_SCRIPT);
            }
        }

        migrate_root_plugins_into_bepinex(runtime_game_path)?;
        normalize_macos_doorstop_config_file(&runtime_game_path.join("doorstop_config.ini"))?;
        configure_macos_doorstop_target_assembly(
            &runtime_game_path.join("doorstop_config.ini"),
            runtime_game_path,
        )?;
        let run_script = canonicalize_macos_bepinex_script(runtime_game_path)?;
        configure_macos_bepinex_script(
            &run_script,
            runtime_game_path,
            load_settings_impl(&app).write_debug_logs_to_game,
        )?;
        sync_macos_runtime_disabled_state(runtime_game_path, false)?;
        dequarantine_recursive(runtime_game_path);
        if manage_steam_launch {
            eprintln!(
                "[install_to_game] macOS Steam launch options deferred until Play to avoid restarting Steam during Apply."
            );
        }
    } else {
        let root_files = ["doorstop_config.ini", "winhttp.dll"];

        for item_name in root_files {
            let dest = game_path.join(item_name);
            let disabled_name = format!("{}_DISABLED", item_name);
            let disabled_dest = game_path.join(&disabled_name);

            if is_vanilla {
                if dest.exists() {
                    if disabled_dest.exists() {
                        let _ = fs::remove_file(&disabled_dest);
                    }
                    let _ = fs::rename(&dest, &disabled_dest);
                    eprintln!("[install_to_game] Vanilla mode: Renamed {} -> {}", item_name, disabled_name);
                }
            } else {
                if disabled_dest.exists() && !dest.exists() {
                    let _ = fs::rename(&disabled_dest, &dest);
                    eprintln!("[install_to_game] Restored {} from disabled", item_name);
                }

                let source = profile_dir.join(item_name);
                if source.exists() && !dest.exists() {
                    fs::copy(&source, &dest).map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
                    eprintln!("[install_to_game] Synced {} to game folder", item_name);
                }
            }
        }

        if !is_vanilla {
            ensure_windows_bepinex_console_enabled(game_path)?;
        }
    }

    eprintln!("[install_to_game] Sync complete!");
    Ok(())
}

#[command]
pub async fn launch_game_with_mods(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    platform: Option<String>
) -> Result<(), String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);
    let settings = load_settings_impl(&app);
    let launch_mode = get_profile_launch_mode(&app, &profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    if is_windows_profile {
        return launch_windows_game(&app, &game_path);
    }

    let runtime_game_path = if is_balatro_identifier(&game_identifier) || is_balatro_game_path(&game_path) {
        game_path.clone()
    } else {
        let resolved = resolve_macos_runtime_root(&game_path);
        if resolved != game_path {
            eprintln!(
                "[launch_game_with_mods] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    };
    let executable_path = find_macos_executable_path(&runtime_game_path);

    if is_balatro_identifier(&game_identifier) || is_balatro_game_path(&game_path) {
        let run_script = game_path.join(BALATRO_LOVELY_SCRIPT);
        if !run_script.exists() {
            return Err("run_lovely_macos.sh not found".to_string());
        }

        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }

        set_executable_if_present(&run_script)?;
        dequarantine_recursive(&game_path);

        std::process::Command::new("/bin/sh")
            .arg(&run_script)
            .current_dir(&game_path)
            .spawn()
            .map_err(|e| format!("Failed to launch run_lovely_macos.sh: {}", e))?;

        if let Some(executable_path) = executable_path.as_ref() {
            if !wait_for_process_start(executable_path, 60_000) {
                eprintln!(
                    "[launch_game_with_mods] run_lovely_macos.sh launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }

        return Ok(());
    }

    validate_macos_bepinex_support(&runtime_game_path)?;

    // STEAM LAUNCH STRATEGY: Launch via steam://run/ so Steam services work
    // (overlay, multiplayer, achievements). The BepInEx script is configured as
    // a Steam launch option in localconfig.vdf. If the launch option isn't set
    // yet, we close Steam briefly (one-time setup), write the VDF, and reopen.
    let dist = infer_distribution_from_game_path(&app, &game_path, false);
    if dist == "steam" && !use_direct_launch {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        let run_script = canonicalize_macos_bepinex_script(&runtime_game_path)?;
        configure_macos_bepinex_script(
            &run_script,
            &runtime_game_path,
            settings.write_debug_logs_to_game,
        )?;
        dequarantine_recursive(&runtime_game_path);

        if let Ok(false) = macos_steam_launch_option_matches_desired(&app, &game_path) {
            eprintln!("[launch_game_with_mods] Steam launch option differs from desired value — reconciling before modded launch.");
            ensure_macos_steam_launch_options(&app, &game_path, true, false)?;
        }
        return launch_via_steam_for_game_path(&app, &game_path);
    }

    if launch_macos_bepinex_wrapper(&app, &runtime_game_path, executable_path.as_ref(), "launch_game_with_mods")? {
        Ok(())
    } else {
        // Fallback: open the resolved app bundle directly, even when the stored
        // game path points to Contents/ or another nested directory.
        let app_bundle = find_macos_launch_bundle(&game_path);
        if let Some(bundle) = app_bundle {
            if let Some(executable_path) = executable_path.as_ref() {
                if is_process_running_for_executable(executable_path) {
                    return Err("Game is already running.".to_string());
                }
            }
            let _ = open::that(&bundle);
            if let Some(executable_path) = executable_path.as_ref() {
            if !wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
                eprintln!(
                    "[launch_game_with_mods] App bundle launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
                }
            }
            Ok(())
        } else {
            Err("run_bepinex.sh not found and no .app bundle found either".to_string())
        }
    }
}

#[command]
pub async fn launch_game_vanilla(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    platform: Option<String>
) -> Result<(), String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);
    let settings = load_settings_impl(&app);
    let launch_mode = get_profile_launch_mode(&app, &profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    if is_windows_profile {
        return launch_windows_game(&app, &game_path);
    }

    let runtime_game_path = if is_balatro_identifier(&game_identifier) || is_balatro_game_path(&game_path) {
        game_path.clone()
    } else {
        let resolved = resolve_macos_runtime_root(&game_path);
        if resolved != game_path {
            eprintln!(
                "[launch_game_vanilla] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    };
    let executable_path = find_macos_executable_path(&runtime_game_path);

    // VANILLA: Launch the game directly without the BepInEx wrapper.
    // Before launching, ensure the app bundle is runnable: re-sign with ad-hoc
    // signature (in case a previous modded session stripped the codesign) and
    // remove quarantine attributes.
    #[cfg(target_os = "macos")]
    if let Some(app_bundle) = find_macos_launch_bundle(&game_path) {
        if is_steam_bundle_path(&app_bundle) {
            eprintln!(
                "[launch_game_vanilla] Skipping signature/quarantine changes for Steam bundle: {}",
                app_bundle.display()
            );
        } else {
            // Check if the app has NO valid signature (unsigned after mod removal)
            let needs_resign = std::process::Command::new("codesign")
                .args(["-v", "--strict", &app_bundle.to_string_lossy()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| !s.success())
                .unwrap_or(false);

            if needs_resign {
                eprintln!(
                    "[launch_game_vanilla] App bundle has invalid/no signature — re-signing with ad-hoc"
                );
                let _ = std::process::Command::new("codesign")
                    .args(["--force", "--deep", "-s", "-", &app_bundle.to_string_lossy()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }

            // De-quarantine
            let _ = std::process::Command::new("xattr")
                .args(["-dr", "com.apple.quarantine", &app_bundle.to_string_lossy()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }

    let dist = infer_distribution_from_game_path(&app, &game_path, false);
    if dist == "steam" && !use_direct_launch {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        if let Ok(true) = macos_steam_launch_option_is_managed(&app, &game_path) {
            if let Err(error) = sync_macos_runtime_disabled_state(&runtime_game_path, true) {
                eprintln!(
                    "[launch_game_vanilla] Failed to enforce runtime_disabled state ({}). Falling back to clearing managed launch option before vanilla Steam launch.",
                    error
                );
                ensure_macos_steam_launch_options(&app, &game_path, false, false)?;
            } else {
                eprintln!(
                    "[launch_game_vanilla] Managed BepInEx launch option detected — keeping it and relying on runtime_disabled for vanilla Steam launch."
                );
            }
        }

        return launch_via_steam_for_game_path(&app, &game_path);
    }

    let app_bundle = find_macos_launch_bundle(&game_path);

    if let Some(bundle) = app_bundle {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        open::that(&bundle).map_err(|e| format!("Failed to launch app bundle: {}", e))?;
        if let Some(executable_path) = executable_path.as_ref() {
            if !wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
                eprintln!(
                    "[launch_game_vanilla] App bundle launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    if let Ok(Some(executable)) = find_game_executable(game_path_str.clone()).await {
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        open::that(executable).map_err(|e| format!("Failed to launch game executable: {}", e))?;
        if let Some(executable_path) = executable_path.as_ref() {
            if !wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
                eprintln!(
                    "[launch_game_vanilla] Executable launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    if let Some(executable_path) = executable_path.as_ref() {
        if is_process_running_for_executable(executable_path) {
            return Err("Game is already running.".to_string());
        }
    }
    open::that(&game_path).map_err(|e| format!("Failed to launch game: {}", e))?;
    if let Some(executable_path) = executable_path.as_ref() {
        if !wait_for_process_start(executable_path, 15_000) {
            return Err("Game did not start in time.".to_string());
        }
    }
    Ok(())
}

#[command]
pub async fn is_game_running(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<bool, String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = match get_game_path(app.clone(), game_identifier, platform).await? {
        Some(path) => path,
        None => return Ok(false),
    };
    let game_path = std::path::PathBuf::from(&game_path_str);

    if is_windows_profile {
        let Some(executable_path) = find_windows_executable_path(&game_path) else {
            return Ok(false);
        };
        return Ok(is_process_running_for_patterns(&build_windows_process_match_patterns(&executable_path)));
    }

    let Some(executable_path) = find_macos_executable_path(&game_path) else {
        return Ok(false);
    };

    Ok(is_process_running_for_executable(&executable_path))
}

#[command]
pub async fn stop_game(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<(), String> {
    let is_windows_profile = normalized_platform(platform.as_deref()) == Some("windows");
    let game_path_str = get_game_path(app.clone(), game_identifier, platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);
    if is_windows_profile {
        let executable_path = find_windows_executable_path(&game_path)
            .ok_or_else(|| "Could not determine the Windows game executable.".to_string())?;
        let process_patterns = build_windows_process_match_patterns(&executable_path);

        if !is_process_running_for_patterns(&process_patterns) {
            return Ok(());
        }

        for pattern in &process_patterns {
			#[cfg(unix)] {
				let _ = std::process::Command::new("/usr/bin/pkill")
					.args(["-TERM", "-f", pattern])
					.status()
					.map_err(|e| format!("Failed to stop the game: {}", e))?;
			}

			#[cfg(windows)] {
			let _ = std::process::Command::new("taskkill")
			.args([ "/IM", &pattern.replace("\\", "")])
			.status()
			.map_err(|e| format!("Failed to stop the game: {}", e))?;
			}
		}

        if wait_for_process_exit_patterns(&process_patterns, 5_000) {
            return Ok(());
        }

        for pattern in &process_patterns {
            let _ = std::process::Command::new("/usr/bin/pkill")
                .args(["-KILL", "-f", pattern])
                .status()
                .map_err(|e| format!("Failed to force stop the game: {}", e))?;
        }

        if !wait_for_process_exit_patterns(&process_patterns, 3_000) {
            return Err("Game did not stop in time.".to_string());
        }

        return Ok(());
    }

    let executable_path = {
        find_macos_executable_path(&game_path)
            .ok_or_else(|| "Could not determine the game executable.".to_string())?
    };
    let exec_pattern = build_process_match_pattern(&executable_path);

    if !is_process_running_for_pattern(&exec_pattern) {
        return Ok(());
    }

    let _ = std::process::Command::new("/usr/bin/pkill")
        .args(["-TERM", "-f", &exec_pattern])
        .status()
        .map_err(|e| format!("Failed to stop the game: {}", e))?;

    if wait_for_process_exit_pattern(&exec_pattern, 5_000) {
        return Ok(());
    }

    let _ = std::process::Command::new("/usr/bin/pkill")
        .args(["-KILL", "-f", &exec_pattern])
        .status()
        .map_err(|e| format!("Failed to force stop the game: {}", e))?;

    if !wait_for_process_exit_pattern(&exec_pattern, 3_000) {
        return Err("Game did not stop in time.".to_string());
    }

    Ok(())
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
    let profile_is_vanilla = profile["is_vanilla"].as_bool().unwrap_or(false);

    // 2. Get game path for this specific profile platform
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), Some(profile_platform.clone())).await?
        .ok_or("Game path not configured. Please set it in Settings.")?;
    let game_path = std::path::Path::new(&game_path_str);
    let runtime_game_path_buf = if profile_platform == "mac"
        && !is_balatro_identifier(&game_identifier)
        && !is_balatro_game_path(game_path)
    {
        let resolved = resolve_macos_runtime_root(game_path);
        if resolved != game_path {
            eprintln!(
                "[sync_profile_to_game] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    } else {
        game_path.to_path_buf()
    };
    let runtime_game_path = runtime_game_path_buf.as_path();
    let game_plugins = if profile_platform == "mac"
        && profile_is_vanilla
        && runtime_game_path.join("BepInEx_DISABLED").is_dir()
    {
        runtime_game_path.join("BepInEx_DISABLED").join("plugins")
    } else {
        runtime_game_path.join("BepInEx").join("plugins")
    };

    // Profile cache path
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");

    eprintln!(
        "[sync_profile_to_game] Syncing profile {} to game {:?} (runtime root {:?}, legacy_cache: {})",
        profile_id,
        game_path,
        runtime_game_path,
        use_cache
    );

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

    let is_mac_profile = profile_platform == "mac";
    let is_balatro_profile = is_mac_profile
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));
    let profile_requires_bepinex = profile_mod_full_names
        .iter()
        .any(|name| name.to_lowercase().contains("bepinexpack"));

    if is_mac_profile && !is_balatro_profile && profile_requires_bepinex {
        validate_macos_bepinex_support(runtime_game_path)?;
    }

    let extract_mod_key = |name: &str| -> String {
        let parts: Vec<&str> = name.split('-').collect();
        if parts.len() >= 2 {
            format!("{}-{}", parts[0], parts[1]).to_lowercase()
        } else {
            name.to_lowercase()
        }
    };
    let extract_version_suffix = |name: &str| -> Option<String> {
        name.rsplit('-').next().and_then(|tail| {
            if tail.contains('.') && tail.chars().all(|c| c.is_ascii_digit() || c == '.') {
                Some(tail.to_lowercase())
            } else {
                None
            }
        })
    };

    // Desired profile state indexed by key (Author-ModName).
    let mut desired_key_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut desired_full_by_key: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut desired_version_by_key: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for full_name in &profile_mod_full_names {
        let key = extract_mod_key(full_name);
        desired_key_set.insert(key.clone());
        desired_full_by_key.insert(key.clone(), full_name.to_lowercase());
        if let Some(version) = extract_version_suffix(full_name) {
            desired_version_by_key.insert(key, version);
        }
    }

    eprintln!("[sync_profile_to_game] Profile has {} mods", profile_mod_full_names.len());

    let is_balatro_profile = profile["platform"].as_str() == Some("mac")
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));

    if is_balatro_profile {
        let mods_root = get_balatro_mods_dir()
            .ok_or_else(|| "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string())?;
        let profile_mods_cache = profile_dir.join("Balatro").join("Mods");

        let mut game_mod_folders: Vec<(String, String, Option<String>)> = vec![];
        if mods_root.exists() {
            if let Ok(entries) = fs::read_dir(&mods_root) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.path().is_dir() {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        let mod_key = if folder_name.eq_ignore_ascii_case("smods") {
                            "steamopollys-steamodded".to_string()
                        } else {
                            extract_mod_key(&folder_name)
                        };
                        let version = extract_version_suffix(&folder_name)
                            .or_else(|| read_manifest_version(&entry.path()));
                        game_mod_folders.push((folder_name, mod_key, version));
                    }
                }
            }
        }

        let lovely_installed = has_balatro_lovely_runtime(game_path);

        let mut to_remove: Vec<String> = Vec::new();
        for (folder_name, gm_key, game_version) in &game_mod_folders {
            if !desired_key_set.contains(gm_key) {
                to_remove.push(folder_name.clone());
                continue;
            }

            let desired_version = desired_version_by_key.get(gm_key);
            let desired_full = desired_full_by_key.get(gm_key);
            let full_mismatch = desired_full
                .map(|full| folder_name.to_lowercase() != *full && !folder_name.eq_ignore_ascii_case("smods"))
                .unwrap_or(false);
            let needs_replacement = match desired_version {
                Some(dv) => match game_version.as_ref() {
                    Some(gv) => gv != dv,
                    None => full_mismatch,
                },
                None => false,
            };

            if needs_replacement && full_mismatch {
                to_remove.push(folder_name.clone());
            }
        }

        let mut to_install: Vec<String> = desired_key_set
            .iter()
            .filter(|pm_key| {
                if *pm_key == "thunderstore-lovely" {
                    return !lovely_installed;
                }

                let desired_full = desired_full_by_key
                    .get(*pm_key)
                    .cloned()
                    .unwrap_or_default();
                let desired_version = desired_version_by_key.get(*pm_key);

                let has_exact_version = game_mod_folders.iter().any(|(folder_name, gm_key, game_version)| {
                    if gm_key != *pm_key {
                        return false;
                    }

                    if let Some(dv) = desired_version {
                        if let Some(gv) = game_version {
                            return gv == dv;
                        }
                        if folder_name.eq_ignore_ascii_case("smods") {
                            return true;
                        }
                        return folder_name.to_lowercase() == desired_full;
                    }

                    true
                });

                !has_exact_version
            })
            .map(|k| k.to_string())
            .collect();
        to_install.sort();

        let mut removed = 0;
        for folder_name in &to_remove {
            let folder_path = mods_root.join(folder_name);
            if folder_path.exists() {
                let _ = fs::remove_dir_all(&folder_path);
                removed += 1;
            }
        }

        let mut cached = 0;
        if use_cache && mods_root.exists() {
            if !profile_mods_cache.exists() {
                let _ = fs::create_dir_all(&profile_mods_cache);
            }

            if let Ok(entries) = fs::read_dir(&mods_root) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.path().is_dir() {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        let cache_path = profile_mods_cache.join(&folder_name);
                        if !cache_path.exists() {
                            if copy_dir_recursive(&entry.path(), &cache_path).is_ok() {
                                cached += 1;
                            }
                        }
                    }
                }
            }
        }

        return Ok(serde_json::json!({
            "removed": removed,
            "to_install": to_install,
            "already_installed": game_mod_folders.len(),
            "cached": cached
        }));
    }

    let stored_manifests = load_owned_mod_manifests(&app, &profile_id, GAME_MANIFEST_SCOPE)?
        .into_iter()
        .filter(|entry| manifest_matches_target_root(&entry.manifest, runtime_game_path))
        .collect::<Vec<_>>();
    let (manifests_to_remove, manifests_to_keep): (Vec<_>, Vec<_>) =
        stored_manifests.into_iter().partition(|entry| {
            let desired_full = desired_full_by_key.get(&entry.manifest.mod_key);
            match desired_full {
                Some(full) => full != &entry.manifest.mod_full_name.to_lowercase(),
                None => true,
            }
        });
    let removed_manifest_keys = manifests_to_remove
        .iter()
        .map(|entry| entry.manifest.mod_key.clone())
        .collect::<std::collections::HashSet<_>>();
    let removed_by_manifest = cleanup_owned_mod_manifests(
        runtime_game_path,
        &manifests_to_remove,
        &manifests_to_keep,
    )?;
    let stale_generated_removed =
        cleanup_stale_generated_mod_artifacts(runtime_game_path, &profile_mod_full_names)?;
    if removed_by_manifest > 0 || stale_generated_removed > 0 {
        eprintln!(
            "[sync_profile_to_game] Cleaned {} tracked manifests and {} stale generated artifacts",
            removed_by_manifest,
            stale_generated_removed
        );
    }

    // 3. Scan game plugins folder for currently installed mods
    // Store both the folder name AND the derived key
    let mut game_mod_folders: Vec<(String, String)> = vec![]; // (folder_name, author-modname key)
    let mut invalid_game_mod_folders: Vec<String> = vec![];
    if game_plugins.exists() {
        if let Ok(entries) = fs::read_dir(&game_plugins) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    let mod_key = extract_mod_key(&folder_name);
                    if game_mod_folder_has_payload(&game_plugins, &entry.path(), &folder_name, &mod_key) {
                        game_mod_folders.push((folder_name, mod_key));
                    } else {
                        eprintln!(
                            "[sync_profile_to_game] Detected broken/metadata-only mod folder: {}",
                            folder_name
                        );
                        invalid_game_mod_folders.push(folder_name);
                    }
                }
            }
        }
    }

    eprintln!(
        "[sync_profile_to_game] Game has {} valid mods installed ({} broken placeholders)",
        game_mod_folders.len(),
        invalid_game_mod_folders.len()
    );

    // 4. Calculate diff using Author-ModName key + version awareness.
    // Remove entries not present in profile OR with a mismatched pinned version.
    let mut to_remove: Vec<String> = invalid_game_mod_folders.clone();
    for (folder_name, gm_key) in &game_mod_folders {
        if !desired_key_set.contains(gm_key) {
            if game_mod_folder_is_auxiliary_payload(folder_name, gm_key, &desired_full_by_key) {
                eprintln!(
                    "[sync_profile_to_game] Keeping auxiliary payload folder: {}",
                    folder_name
                );
                continue;
            }
            to_remove.push(folder_name.clone());
            continue;
        }

        let desired_version = desired_version_by_key.get(gm_key);
        let desired_full = desired_full_by_key.get(gm_key);
        let game_version = extract_version_suffix(folder_name);
        let full_mismatch = desired_full
            .map(|full| folder_name.to_lowercase() != *full)
            .unwrap_or(false);
        let needs_replacement = match desired_version {
            Some(dv) => match game_version.as_ref() {
                Some(gv) => gv != dv,
                None => full_mismatch,
            },
            None => false,
        };

        if needs_replacement && full_mismatch {
            to_remove.push(folder_name.clone());
        }
    }

    // to_install: any profile key not present at the desired version in game
    // Special case: BepInExPack installs to game root, not plugins - check if BepInEx folder exists
    let bepinex_installed = if profile["platform"].as_str() == Some("mac") {
        if profile_is_vanilla {
            has_complete_disabled_macos_bepinex_runtime(runtime_game_path)
                || has_complete_macos_bepinex_runtime(runtime_game_path)
        } else {
            has_complete_macos_bepinex_runtime(runtime_game_path)
        }
    } else {
        game_path.join("BepInEx").join("core").exists()
    };

    let mut to_install: Vec<String> = desired_key_set
        .iter()
        .filter(|pm_key| {
            // Skip BepInExPack if BepInEx is already installed
            if pm_key.contains("bepinex") && bepinex_installed {
                return false;
            }

            let desired_full = desired_full_by_key
                .get(*pm_key)
                .cloned()
                .unwrap_or_default();
            let desired_version = desired_version_by_key.get(*pm_key);

            let has_exact_version = game_mod_folders.iter().any(|(folder_name, gm_key)| {
                if gm_key != *pm_key {
                    return false;
                }

                if let Some(dv) = desired_version {
                    if let Some(gv) = extract_version_suffix(folder_name) {
                        return gv == *dv;
                    }
                    // Fallback for unusual folder naming: compare full folder name.
                    return folder_name.to_lowercase() == desired_full;
                }

                true
            });

            !has_exact_version
        })
        .map(|k| k.to_string())
        .collect();
    to_install.sort();

    eprintln!("[sync_profile_to_game] To remove: {:?}, To install: {:?}", to_remove.len(), to_install.len());

    // 5. Remove mods not in profile (we have the exact folder names from the tuple)
    let mut removed = removed_manifest_keys.len();
    for folder_name in &to_remove {
        let folder_path = game_plugins.join(folder_name);
        if folder_path.exists() {
            eprintln!("[sync_profile_to_game] Removing: {}", folder_name);
            if remove_plugin_entry(&folder_path).is_ok() {
                if !removed_manifest_keys.contains(&extract_mod_key(folder_name)) {
                    removed += 1;
                }
            }
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
    let to_install_names: Vec<String> = to_install;
    let already_installed = game_mod_folders.len().saturating_sub(removed);

    Ok(serde_json::json!({
        "removed": removed,
        "to_install": to_install_names,
        "already_installed": already_installed,
        "cached": cached
    }))
}
