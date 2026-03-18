use std::fs;
use tauri::{command, AppHandle, Manager};
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
};

const CANONICAL_MAC_BEPINEX_SCRIPT: &str = "run_bepinex.sh";
const BALATRO_LOVELY_SCRIPT: &str = "run_lovely_macos.sh";

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

    let localconfig_path = get_latest_localconfig_path(&steam_root_for_config).ok_or_else(|| {
        "Couldn't locate Steam's localconfig.vdf for macOS launch option inspection.".to_string()
    })?;

    let localconfig = fs::read_to_string(&localconfig_path)
        .map_err(|e| format!("Failed to read Steam localconfig.vdf: {}", e))?;

    Ok(get_launch_options_for_app(&localconfig, &app_id)
        .as_deref()
        .map(is_managed_macos_launch_option)
        .unwrap_or(false))
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
            let common_root = canonicalize_or_original(&library_root.join("steamapps").join("common"));
            if game_path.starts_with(&common_root) {
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
            let common_root = canonicalize_or_original(&library_root.join("steamapps").join("common"));
            if canonical_game.starts_with(&common_root) {
                return Some(steam_root);
            }
        }
    }

    None
}

fn get_latest_localconfig_path(steam_root: &std::path::Path) -> Option<std::path::PathBuf> {
    fn steamid64_to_accountid(user_id: &str) -> Option<String> {
        const STEAMID64_BASE: u64 = 76561197960265728;
        let parsed = user_id.parse::<u64>().ok()?;
        parsed
            .checked_sub(STEAMID64_BASE)
            .map(|account_id| account_id.to_string())
    }

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
                            let candidate = steam_root
                                .join("userdata")
                                .join(account_id)
                                .join("config")
                                .join("localconfig.vdf");
                            if candidate.exists() {
                                return Some(candidate);
                            }
                        }
                    }
                }
            }
        }
    }

    let userdata_dir = steam_root.join("userdata");
    if !userdata_dir.exists() {
        return None;
    }

    fs::read_dir(userdata_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("config").join("localconfig.vdf"))
        .filter(|path| path.exists())
        .max_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
}

fn find_macos_app_bundle(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    if game_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".app"))
        .unwrap_or(false)
    {
        return Some(game_path.to_path_buf());
    }

    if !game_path.is_dir() {
        return None;
    }

    fs::read_dir(game_path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name().to_string_lossy().ends_with(".app"))
        .map(|entry| entry.path())
}

fn find_macos_executable_path(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let app_bundle = find_macos_app_bundle(game_path)?;
    let macos_dir = app_bundle.join("Contents").join("MacOS");
    if !macos_dir.is_dir() {
        return None;
    }

    let defaults_output = std::process::Command::new("/usr/bin/defaults")
        .args([
            "read",
            &app_bundle.join("Contents").join("Info").to_string_lossy(),
            "CFBundleExecutable",
        ])
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
    std::process::Command::new("/usr/bin/pgrep")
        .args(["-f", pattern])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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

    let prefix_root = find_wine_prefix_root(&executable_path).or_else(|| find_wine_prefix_root(game_path));

    if let Some(runner_path) = find_wine_runner_binary(prefix_root.as_deref(), &executable_path) {
        let mut command = std::process::Command::new(&runner_path);
        let executable_dir = executable_path.parent().unwrap_or(game_path);
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

fn launch_macos_bepinex_wrapper(
    game_path: &std::path::Path,
    executable_path: Option<&std::path::PathBuf>,
    context: &str,
) -> Result<bool, String> {
    if find_bepinex_script_in_dir(game_path).is_none() {
        return Ok(false);
    }

    let run_script = canonicalize_macos_bepinex_script(game_path)?;

    if let Some(executable_path) = executable_path.as_ref() {
        if is_process_running_for_executable(executable_path) {
            return Err("Game is already running.".to_string());
        }
    }

    configure_macos_bepinex_script(&run_script, game_path)?;
    dequarantine_recursive(game_path);

    eprintln!("[{}] Launching via run_bepinex.sh at {:?}", context, run_script);

    std::process::Command::new(&run_script)
        .current_dir(game_path)
        .spawn()
        .map_err(|e| format!("Failed to launch run_bepinex.sh: {}", e))?;

    if let Some(executable_path) = executable_path.as_ref() {
        if !wait_for_process_start(executable_path, 60_000) {
            eprintln!(
                "[{}] run_bepinex.sh launch request succeeded, but the game process was not observed in time. Continuing optimistically.",
                context
            );
        }
    }

    Ok(true)
}

fn launch_via_steam_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let app_id = find_steam_app_id_for_game_path_any(app, game_path, false)
        .ok_or_else(|| "Couldn't determine the Steam app ID for this game".to_string())?;
    let executable_path = find_macos_executable_path(game_path);

    let status = std::process::Command::new("/usr/bin/open")
        .arg(format!("steam://run/{}", app_id))
        .status()
        .map_err(|e| format!("Failed to ask Steam to launch the game: {}", e))?;

    if !status.success() {
        return Err("Steam rejected the launch request.".to_string());
    }

    if let Some(executable_path) = executable_path.as_ref() {
        if !wait_for_process_start(executable_path, 60_000) {
            eprintln!(
                "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                app_id
            );
        }
    }

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
    let has_core = game_path.join("BepInEx").join("core").is_dir();
    let has_doorstop_payload = game_path.join("doorstop_libs").is_dir()
        || game_path.join("libdoorstop.dylib").exists();
    let has_script = find_bepinex_script_in_dir(game_path).is_some();

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
    let dir_items = ["BepInEx", "doorstop_libs"];
    let file_items = [
        "doorstop_config.ini",
        "libdoorstop.dylib",
        ".doorstop_version",
    ];

    for item in dir_items {
        let active = game_path.join(item);
        let disabled = game_path.join(format!("{}_DISABLED", item));
        if disable {
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
        let active = game_path.join(item);
        let disabled = game_path.join(format!("{}_DISABLED", item));
        if disable {
            rename_path_if_present(&active, &disabled)?;
        } else if disabled.exists() {
            if !active.exists() {
                rename_path_if_present(&disabled, &active)?;
            } else {
                let _ = fs::remove_file(&disabled);
            }
        }
    }

    let active_script = game_path.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    let disabled_script = game_path.join(format!("{}_DISABLED", CANONICAL_MAC_BEPINEX_SCRIPT));
    if !active_script.exists() && disabled_script.exists() {
        rename_path_if_present(&disabled_script, &active_script)?;
    } else if active_script.exists() && disabled_script.exists() {
        if disabled_script.is_dir() && !disabled_script.is_symlink() {
            let _ = fs::remove_dir_all(&disabled_script);
        } else {
            let _ = fs::remove_file(&disabled_script);
        }
    }

    if let Ok(entries) = fs::read_dir(game_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if disable {
                if path.is_file() && is_bepinex_shell_script_name(&name) && name != CANONICAL_MAC_BEPINEX_SCRIPT {
                    let disabled = game_path.join(format!("{}_DISABLED", name));
                    rename_path_if_present(&path, &disabled)?;
                }
            } else if name.ends_with("_DISABLED") {
                let active_name = name.trim_end_matches("_DISABLED").to_string();
                if is_bepinex_shell_script_name(&active_name) {
                    let active_path = game_path.join(&active_name);
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

fn quit_steam_if_running() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let steam_running = std::process::Command::new("/usr/bin/pgrep")
            .args(["-x", "Steam"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        if !steam_running {
            return Ok(false);
        }

        let _ = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"Steam\" to quit"])
            .status();

        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let still_running = std::process::Command::new("/usr/bin/pgrep")
                .args(["-x", "Steam"])
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if !still_running {
                return Ok(true);
            }
        }

        return Err(
            "Steam is still running. r2modmac needs Steam closed briefly to update launch options."
                .to_string(),
        );
    }

    #[allow(unreachable_code)]
    Ok(false)
}

fn reopen_steam_if_needed(was_running: bool) {
    #[cfg(target_os = "macos")]
    {
        if !was_running {
            return;
        }

        let status = std::process::Command::new("/usr/bin/open")
            .args(["-a", "Steam"])
            .status();
        if let Err(error) = status {
            eprintln!(
                "[ensure_macos_steam_launch_options] failed to relaunch Steam after updating launch options: {}",
                error
            );
        }
    }
}

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
    }

    Ok(())
}

async fn ensure_macos_bepinex_runtime_present(
    app: &AppHandle,
    profile_id: &str,
    game_path: &std::path::Path,
) -> Result<(), String> {
    if has_complete_macos_bepinex_runtime(game_path) {
        return Ok(());
    }

    let profile_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id);

    if has_complete_macos_bepinex_runtime(&profile_dir) {
        copy_macos_bepinex_runtime_root(&profile_dir, game_path)?;
        dequarantine_recursive(game_path);
        if has_complete_macos_bepinex_runtime(game_path) {
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
    let runtime_kind = detect_unity_runtime_kind(game_path);
    let runtime_bytes = download_official_macos_bepinex_runtime(&version_number, runtime_kind).await?;

    fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut game_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    extract_bepinex_pack_to_root(&mut game_archive, game_path, true)?;

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut profile_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    extract_bepinex_pack_to_root(&mut profile_archive, &profile_dir, true)?;

    dequarantine_recursive(game_path);

    if has_complete_macos_bepinex_runtime(game_path) {
        Ok(())
    } else {
        Err("No macOS BepInEx startup script found".to_string())
    }
}

fn configure_macos_bepinex_script(
    script_path: &std::path::Path,
    game_path: &std::path::Path,
) -> Result<(), String> {
    fn resolve_macos_executable_path(
        game_path: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        let app_bundle = if game_path
            .file_name()
            .map(|name| name.to_string_lossy().ends_with(".app"))
            .unwrap_or(false)
        {
            game_path.to_path_buf()
        } else {
            fs::read_dir(game_path)
                .map_err(|e| format!("Failed to scan macOS game directory: {}", e))?
                .filter_map(|e| e.ok())
                .find(|entry| entry.file_name().to_string_lossy().ends_with(".app"))
                .map(|entry| entry.path())
                .ok_or_else(|| "No .app bundle found in macOS game directory".to_string())?
        };

        let macos_dir = app_bundle.join("Contents").join("MacOS");
        fs::read_dir(&macos_dir)
            .map_err(|e| format!("Failed to inspect app bundle executable: {}", e))?
            .filter_map(|e| e.ok())
            .find(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|entry| entry.path())
            .ok_or_else(|| "No executable found inside .app/Contents/MacOS".to_string())
    }

    fn has_macos_doorstop_support(script: &str) -> bool {
        let lower = script.to_lowercase();
        lower.contains("dyld_insert_libraries")
            && lower.contains("dylib")
            && (lower.contains("doorstop_enable") || lower.contains("doorstop_enabled"))
    }

    fn build_generated_macos_bepinex_script(relative_exec: &str) -> String {
        format!(
            r#"#!/bin/sh
# r2modmac generated macOS BepInEx launcher
a="/$0"; a=${{a%/*}}; a=${{a#/}}; a=${{a:-.}}; BASEDIR=$(cd "$a"; pwd -P)
cd "$BASEDIR"

# r2modmac: if the runtime is marked disabled, launch the game without Doorstop.
runtime_disabled=false
if [ -e "$BASEDIR/BepInEx_DISABLED" ] || [ -e "$BASEDIR/doorstop_libs_DISABLED" ] || [ -e "$BASEDIR/libdoorstop.dylib_DISABLED" ] || [ -e "$BASEDIR/doorstop_config.ini_DISABLED" ]; then
    runtime_disabled=true
fi

if [ "$2" = "SteamLaunch" ]; then
    "$1" "$2" "$3" "$4" "$0" "$7"
    exit
fi

if command -v xattr >/dev/null 2>&1; then
  xattr -dr com.apple.quarantine "$BASEDIR/run_bepinex.sh" "$BASEDIR/doorstop_libs" "$BASEDIR/BepInEx" "$BASEDIR"/*.dylib 2>/dev/null || true
fi

export DOORSTOP_ENABLE=TRUE
export DOORSTOP_INVOKE_DLL_PATH="$BASEDIR/BepInEx/core/BepInEx.Preloader.dll"
export DOORSTOP_CORLIB_OVERRIDE_PATH=""

doorstop_libs="$BASEDIR/doorstop_libs"
executable_path="$BASEDIR/{relative_exec}"

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
executable_type=$(LD_PRELOAD="" file -b "${{executable_path}}")

case $executable_type in
    *64-bit*)
        arch="x64"
        ;;
    *32-bit*|*i386*)
        arch="x86"
        ;;
    *)
        echo "Cannot identify executable type: $executable_type"
        exit 1
        ;;
esac

doorstop_libname="libdoorstop_${{arch}}.dylib"

if [ "$runtime_disabled" = true ]; then
    exec "${{executable_path}}"
fi

export LD_LIBRARY_PATH="${{doorstop_libs}}:${{LD_LIBRARY_PATH}}"
export LD_PRELOAD="${{doorstop_libname}}:${{LD_PRELOAD}}"
export DYLD_LIBRARY_PATH="${{doorstop_libs}}"
export DYLD_INSERT_LIBRARIES="${{doorstop_libs}}/${{doorstop_libname}}"

exec "${{executable_path}}"
"#
        )
    }

    if !script_path.exists() {
        return Ok(());
    }

    let executable_path = resolve_macos_executable_path(game_path)?;
    let relative_executable = executable_path
        .strip_prefix(game_path)
        .ok()
        .unwrap_or(executable_path.as_path())
        .to_string_lossy()
        .replace('\\', "/");

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

    let needs_regeneration = !has_macos_doorstop_support(&script) || !early_exit_before_dyld;
    if needs_regeneration {
        eprintln!(
            "[configure_macos_bepinex_script] Regenerating script (has_doorstop={} early_exit_ok={}).",
            has_macos_doorstop_support(&script),
            early_exit_before_dyld
        );
        script = build_generated_macos_bepinex_script(&relative_executable);
    } else if script.contains("\nexecutable_name=\"\"") {
        script = script.replace(
            "\nexecutable_name=\"\"",
            &format!("\nexecutable_name=\"{}\"", relative_executable),
        );
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
        let dequarantine_block = "\n# r2modmac: best-effort de-quarantine for Doorstop/BepInEx payloads\nif command -v xattr >/dev/null 2>&1; then\n  xattr -dr com.apple.quarantine \"$BASEDIR/run_bepinex.sh\" \"$BASEDIR/doorstop_libs\" \"$BASEDIR/BepInEx\" \"$BASEDIR\"/*.dylib 2>/dev/null || true\nfi\n";
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(script_path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(script_path, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub(crate) fn ensure_macos_steam_launch_options(
    app: &AppHandle,
    game_path: &std::path::Path,
    enable_mods: bool,
) -> Result<(), String> {
    let steam_roots = get_steam_roots_for_platform(app, false);
    if steam_roots.is_empty() {
        return Err("No Steam installation found to configure macOS launch options".to_string());
    }

    let managed_launch_option = if enable_mods {
        let script_path = canonicalize_macos_bepinex_script(game_path)?;
        Some(format!(
            "/usr/bin/arch -x86_64 /bin/bash \"{}\" %command%",
            script_path.display()
        ))
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

    let localconfig_path = get_latest_localconfig_path(&steam_root_for_config).ok_or_else(|| {
        "Couldn't locate Steam's localconfig.vdf for automatic macOS launch option setup.".to_string()
    })?;

    eprintln!(
        "[ensure_macos_steam_launch_options] app_id={} localconfig={}",
        app_id,
        localconfig_path.display()
    );

    let localconfig = fs::read_to_string(&localconfig_path)
        .map_err(|e| format!("Failed to read Steam localconfig.vdf: {}", e))?;

    let scoped_backup_key = format!(
        "steam::{}::{}",
        canonicalize_or_original(&localconfig_path).to_string_lossy(),
        app_id
    );
    let legacy_backup_key = format!("steam::{}", app_id);
    let mut settings = load_settings_impl(app);

    let desired = if enable_mods {
        managed_launch_option.as_deref()
    } else {
        None
    };
    let (updated_text, current_launch_options) =
        update_launch_options_in_localconfig(&localconfig, &app_id, desired)?;

    if enable_mods {
        let expected = managed_launch_option
            .as_ref()
            .ok_or_else(|| "Managed macOS launch option was not generated".to_string())?;
        let staged_value = get_launch_options_for_app(&updated_text, &app_id)
            .ok_or_else(|| format!("Failed to stage Steam launch options for app {}", app_id))?;
        if staged_value != *expected {
            return Err(format!(
                "Failed to stage Steam launch options for app {}. Expected {:?}, got {:?}",
                app_id, expected, staged_value
            ));
        }
    }

    let mut settings_changed = false;
    let mut steam_was_running: Option<bool> = None;
    let mut ensure_steam_stopped = || -> Result<(), String> {
        if steam_was_running.is_none() {
            steam_was_running = Some(quit_steam_if_running()?);
        }
        Ok(())
    };

    if enable_mods {
        if let Some(current) = current_launch_options.as_ref() {
            if !current.trim().is_empty() && !is_managed_macos_launch_option(current) {
                if !settings
                    .steam_launch_option_backups
                    .contains_key(&scoped_backup_key)
                {
                    settings
                        .steam_launch_option_backups
                        .insert(scoped_backup_key.clone(), current.clone());
                    settings_changed = true;
                }
            }
        }
        if settings
            .steam_launch_option_backups
            .remove(&legacy_backup_key)
            .is_some()
        {
            settings_changed = true;
        }
    } else {
        if let Some(previous) = settings
            .steam_launch_option_backups
            .remove(&scoped_backup_key)
            .or_else(|| settings.steam_launch_option_backups.remove(&legacy_backup_key))
        {
            let (restored_text, _) =
                update_launch_options_in_localconfig(&localconfig, &app_id, Some(&previous))?;
            if restored_text != localconfig {
                ensure_steam_stopped()?;
                fs::write(&localconfig_path, restored_text)
                    .map_err(|e| format!("Failed to restore Steam launch options: {}", e))?;
                let persisted = fs::read_to_string(&localconfig_path)
                    .map_err(|e| format!("Failed to verify restored Steam launch options: {}", e))?;
                if get_launch_options_for_app(&persisted, &app_id).as_deref() != Some(previous.as_str()) {
                    return Err(format!(
                        "Steam launch options were not restored correctly for app {}",
                        app_id
                    ));
                }
            }
            settings_changed = true;
        } else if current_launch_options
            .as_deref()
            .map(is_managed_macos_launch_option)
            .unwrap_or(false)
            && updated_text != localconfig
        {
            ensure_steam_stopped()?;
            fs::write(&localconfig_path, updated_text)
                .map_err(|e| format!("Failed to clear Steam launch options: {}", e))?;
            let persisted = fs::read_to_string(&localconfig_path)
                .map_err(|e| format!("Failed to verify cleared Steam launch options: {}", e))?;
            if get_launch_options_for_app(&persisted, &app_id)
                .as_deref()
                .map(is_managed_macos_launch_option)
                .unwrap_or(false)
            {
                return Err(format!(
                    "Managed Steam launch options are still present for app {} after clearing",
                    app_id
                ));
            }
        }

        if settings_changed {
            save_settings_impl(app, &settings)?;
        }
        reopen_steam_if_needed(steam_was_running.unwrap_or(false));
        return Ok(());
    }

    if updated_text != localconfig {
        ensure_steam_stopped()?;
        fs::write(&localconfig_path, updated_text)
            .map_err(|e| format!("Failed to update Steam launch options: {}", e))?;
        let persisted = fs::read_to_string(&localconfig_path)
            .map_err(|e| format!("Failed to verify updated Steam launch options: {}", e))?;
        let expected = managed_launch_option
            .as_ref()
            .ok_or_else(|| "Managed macOS launch option was not generated".to_string())?;
        if get_launch_options_for_app(&persisted, &app_id).as_deref() != Some(expected.as_str()) {
            return Err(format!(
                "Steam launch options were not persisted for app {}",
                app_id
            ));
        }
    }

    if settings_changed {
        save_settings_impl(app, &settings)?;
    }

    reopen_steam_if_needed(steam_was_running.unwrap_or(false));
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
            eprintln!("[get_game_path] Found manual override (key={}): {}", key, path);
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

    eprintln!("[install_to_game] Installing profile {} to game {}", profile_id, game_path.display());

    // --- SYNC: Remove mods from game that are not in profile OR are disabled ---
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");
    let game_plugins = game_path.join("BepInEx").join("plugins");
    
    // Create set of enabled mod names (lowercase for comparison)
    let disabled_set: std::collections::HashSet<String> = disabled_mods.iter()
        .map(|s| s.to_lowercase())
        .collect();

    if is_mac_profile && is_vanilla {
        if find_bepinex_script_in_dir(game_path).is_some() {
            let run_script = canonicalize_macos_bepinex_script(game_path)?;
            configure_macos_bepinex_script(&run_script, game_path)?;
            dequarantine_recursive(game_path);
        }
        // Always ensure launch options are REMOVED in vanilla mode (remove the BepInEx wrapper).
        // Do not gate on !macos_steam_launch_option_is_managed: the current state doesn't matter,
        // we always want the options to reflect the desired mode.
        if manage_steam_launch {
            ensure_macos_steam_launch_options(&app, game_path, false)?;
        }
        sync_macos_runtime_disabled_state(game_path, true)?;
        eprintln!("[install_to_game] macOS vanilla mode complete - runtime disabled while preserving the Steam wrapper.");
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
            sync_macos_runtime_disabled_state(game_path, false)?;
            ensure_macos_bepinex_runtime_present(&app, &profile_id, game_path).await?;
        }

        let leaked_windows_loader = game_path.join("winhttp.dll");
        if leaked_windows_loader.exists() {
            let _ = fs::remove_file(&leaked_windows_loader);
        }

        let root_items = [
            ("doorstop_libs", true),
            ("doorstop_config.ini", false),
            ("libdoorstop.dylib", false),
        ];

        for (item_name, is_dir) in root_items {
            let source = profile_dir.join(item_name);
            let dest = game_path.join(item_name);

            if !source.exists() {
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
            let dest_script = game_path.join(CANONICAL_MAC_BEPINEX_SCRIPT);
            if dest_script.exists() {
                let _ = fs::remove_file(&dest_script);
            }
            fs::copy(&source_script, &dest_script)
                .map_err(|e| format!("Failed to copy {}: {}", CANONICAL_MAC_BEPINEX_SCRIPT, e))?;
            eprintln!("[install_to_game] Synced {} to game folder", CANONICAL_MAC_BEPINEX_SCRIPT);
        }

        migrate_root_plugins_into_bepinex(game_path)?;
        let run_script = canonicalize_macos_bepinex_script(game_path)?;
        configure_macos_bepinex_script(&run_script, game_path)?;
        sync_macos_runtime_disabled_state(game_path, false)?;
        dequarantine_recursive(game_path);
        // Always ensure launch options are SET in modded mode (add the BepInEx wrapper).
        // Do not gate on !macos_steam_launch_option_is_managed: always push the correct value
        // so that switching between profiles/modes always produces the right launch options.
        if manage_steam_launch {
            ensure_macos_steam_launch_options(&app, game_path, true)?;
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
    let launch_mode = get_profile_launch_mode(&app, &profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    if is_windows_profile {
        return launch_windows_game(&app, &game_path);
    }

    let executable_path = find_macos_executable_path(&game_path);

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

    // Safety-net: before launching via Steam as MODDED, ensure the BepInEx launch
    // option is actually set in localconfig.vdf. This covers the case where the user
    // presses Play without having run "Apply to Game" first, or where a previous
    // vanilla launch left the options cleared.
    let dist = infer_distribution_from_game_path(&app, &game_path, false);
    if should_manage_steam_launch_options(&dist, &launch_mode) && !use_direct_launch {
        if let Ok(false) = macos_steam_launch_option_is_managed(&app, &game_path) {
            eprintln!("[launch_game_with_mods] Launch options not set — forcing BepInEx launch option before Steam launch.");
            let _ = ensure_macos_steam_launch_options(&app, &game_path, true);
        }
    }

    if dist == "steam" && !use_direct_launch {
        return launch_via_steam_for_game_path(&app, &game_path);
    }

    if launch_macos_bepinex_wrapper(&game_path, executable_path.as_ref(), "launch_game_with_mods")? {
        Ok(())
    } else {
        // Fallback: open the .app directly
        let app_bundle = fs::read_dir(&game_path)
            .ok()
            .and_then(|entries| {
                entries.filter_map(|e| e.ok())
                    .find(|e| e.file_name().to_string_lossy().ends_with(".app"))
                    .map(|e| e.path())
            });
        if let Some(bundle) = app_bundle {
            if let Some(executable_path) = executable_path.as_ref() {
                if is_process_running_for_executable(executable_path) {
                    return Err("Game is already running.".to_string());
                }
            }
            let _ = open::that(&bundle);
            if let Some(executable_path) = executable_path.as_ref() {
                if !wait_for_process_start(executable_path, 60_000) {
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
    let game_path_str = get_game_path(app.clone(), game_identifier, platform)
        .await?
        .ok_or_else(|| "Game path not found".to_string())?;
    let game_path = std::path::PathBuf::from(&game_path_str);
    let launch_mode = get_profile_launch_mode(&app, &profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    if is_windows_profile {
        return launch_windows_game(&app, &game_path);
    }

    let executable_path = find_macos_executable_path(&game_path);

    if use_direct_launch {
        if launch_macos_bepinex_wrapper(&game_path, executable_path.as_ref(), "launch_game_vanilla")? {
            return Ok(());
        }
    }

    // Safety-net: before launching via Steam as VANILLA, ensure any BepInEx launch
    // option is removed from localconfig.vdf. This covers the case where the user
    // presses Play without having run "Apply to Game" (switch to vanilla) first.
    let dist = infer_distribution_from_game_path(&app, &game_path, false);
    if should_manage_steam_launch_options(&dist, &launch_mode) && !use_direct_launch {
        if let Ok(true) = macos_steam_launch_option_is_managed(&app, &game_path) {
            eprintln!("[launch_game_vanilla] BepInEx launch option still set — forcing removal before vanilla Steam launch.");
            let _ = ensure_macos_steam_launch_options(&app, &game_path, false);
        }
    }

    if dist == "steam" && !use_direct_launch {
        return launch_via_steam_for_game_path(&app, &game_path);
    }

    let app_bundle = if game_path.is_dir() {
        fs::read_dir(&game_path)
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .find(|e| e.file_name().to_string_lossy().ends_with(".app"))
                    .map(|e| e.path())
            })
    } else if game_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".app"))
        .unwrap_or(false)
    {
        Some(game_path.clone())
    } else {
        None
    };

    if let Some(bundle) = app_bundle {
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        open::that(&bundle).map_err(|e| format!("Failed to launch app bundle: {}", e))?;
        if let Some(executable_path) = executable_path.as_ref() {
            if !wait_for_process_start(executable_path, 60_000) {
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
            if !wait_for_process_start(executable_path, 60_000) {
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
            let _ = std::process::Command::new("/usr/bin/pkill")
                .args(["-TERM", "-f", pattern])
                .status()
                .map_err(|e| format!("Failed to stop the game: {}", e))?;
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

    // 2. Get game path for this specific profile platform
    let game_path_str = get_game_path(app.clone(), game_identifier.clone(), Some(profile_platform)).await?
        .ok_or("Game path not configured. Please set it in Settings.")?;
    let game_path = std::path::Path::new(&game_path_str);
    let game_plugins = game_path.join("BepInEx").join("plugins");
    
    // Profile cache path
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");

    eprintln!("[sync_profile_to_game] Syncing profile {} to game {:?} (legacy_cache: {})", profile_id, game_path, use_cache);
    
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
        .filter(|entry| manifest_matches_target_root(&entry.manifest, game_path))
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
    let removed_by_manifest =
        cleanup_owned_mod_manifests(game_path, &manifests_to_remove, &manifests_to_keep)?;
    let stale_generated_removed =
        cleanup_stale_generated_mod_artifacts(game_path, &profile_mod_full_names)?;
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
        has_complete_macos_bepinex_runtime(game_path)
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
