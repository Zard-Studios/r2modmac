use super::*;

pub(super) fn find_steam_app_id_for_library_root(
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

        let manifest_game_path = library_root
            .join("steamapps")
            .join("common")
            .join(install_dir);
        if game_path_matches_install_root(game_path, &manifest_game_path) {
            return Some(app_id);
        }
    }

    None
}

pub(super) fn find_embedded_steam_library_root_for_game_path(
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

pub(super) fn can_launch_via_steam_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> bool {
    let Some(steam_root) =
        find_matching_steam_root_for_game_path(app, game_path, is_windows_profile)
    else {
        return false;
    };

    find_steam_app_id_for_game_path(&steam_root, game_path).is_some()
}

pub(super) fn infer_distribution_from_game_path(
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

pub(super) fn get_steam_roots_for_platform(
    app: &AppHandle,
    is_windows_profile: bool,
) -> Vec<std::path::PathBuf> {
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
        settings.mac_steam_path.as_ref().or(legacy_mac_steam_path)
    };

    if let Some(steam_path_str) = configured_steam_path {
        let configured_steam = expand_user_path(steam_path_str);
        if configured_steam.exists() && !steam_paths_to_check.contains(&configured_steam) {
            steam_paths_to_check.push(configured_steam);
        }
    }

    steam_paths_to_check
}

pub(super) fn parse_manifest_value(content: &str, key: &str) -> Option<String> {
    let pattern = format!(r#""{}"\s+"([^"]+)""#, regex::escape(key));
    let re = regex::Regex::new(&pattern).ok()?;
    re.captures(content)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
}

pub(super) fn find_steam_app_id_for_game_path(
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

pub(super) fn find_steam_app_id_for_game_path_any(
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

pub(super) fn find_matching_steam_root_for_game_path(
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
