use super::*;

pub(super) fn is_bepinex_shell_script_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".sh") && lower.contains("bepinex")
}

pub(super) fn normalized_platform(platform: Option<&str>) -> Option<&'static str> {
    match platform {
        Some("windows") => Some("windows"),
        Some("mac") => Some("mac"),
        _ => None,
    }
}

pub(super) fn manual_override_keys(game_identifier: &str, platform: Option<&str>) -> Vec<String> {
    if let Some(p) = normalized_platform(platform) {
        vec![
            format!("{}::{}", game_identifier, p),
            game_identifier.to_string(),
        ]
    } else {
        vec![game_identifier.to_string()]
    }
}

pub(super) fn log_manual_override_once(key: &str, path: &str) {
    let dedupe_key = format!("{}::{}", key, path);
    let seen = LOGGED_GAME_PATH_OVERRIDES.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut seen) = seen.lock() else {
        eprintln!(
            "[get_game_path] Found manual override (key={}): {}",
            key, path
        );
        return;
    };

    if seen.insert(dedupe_key) {
        eprintln!(
            "[get_game_path] Found manual override (key={}): {}",
            key, path
        );
    }
}

pub(super) fn normalize_mod_match_value(input: &str) -> String {
    let stem = std::path::Path::new(input)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());

    stem.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

pub(super) fn derive_mod_match_terms(input: &str) -> Vec<String> {
    let stem = std::path::Path::new(input)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());
    let parts: Vec<&str> = stem.split('-').filter(|part| !part.is_empty()).collect();
    let mut terms = Vec::new();

    if parts.len() >= 2 {
        terms.push(normalize_mod_match_value(parts[1]));
        terms.push(normalize_mod_match_value(&format!(
            "{}-{}",
            parts[0], parts[1]
        )));
    }

    terms.push(normalize_mod_match_value(&stem));
    terms.retain(|term| term.len() >= 3);
    terms.sort();
    terms.dedup();
    terms
}

pub(super) fn is_metadata_only_plugin_file_name(name: &str) -> bool {
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

pub(super) fn path_has_plugin_payload(path: &std::path::Path, depth: usize) -> bool {
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

pub(super) fn entry_name_matches_mod_payload(
    entry_name: &str,
    folder_name: &str,
    mod_key: &str,
) -> bool {
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

pub(super) fn game_mod_folder_has_payload(
    plugins_root: &std::path::Path,
    folder_path: &std::path::Path,
    folder_name: &str,
    mod_key: &str,
) -> bool {
    if folder_path.join("manifest.json").exists() {
        return true;
    }

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

pub(super) fn game_mod_folder_is_auxiliary_payload(
    folder_name: &str,
    folder_key: &str,
    desired_full_by_key: &std::collections::HashMap<String, String>,
) -> bool {
    desired_full_by_key
        .iter()
        .any(|(desired_key, desired_full)| {
            desired_key != folder_key
                && entry_name_matches_mod_payload(folder_name, desired_full, desired_key)
        })
}

pub(super) fn remove_plugin_entry(path: &std::path::Path) -> std::io::Result<()> {
    if path.is_symlink() || path.is_file() {
        fs::remove_file(path)
    } else if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        Ok(())
    }
}

pub(super) fn is_stale_generated_entry_name(name: &str) -> bool {
    !matches!(name.to_lowercase().as_str(), ".ds_store" | "bepinex.cfg")
}

pub(super) fn cleanup_stale_generated_mod_artifacts(
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

pub(super) fn manual_path_matches_platform(
    path: &std::path::Path,
    is_windows_profile: bool,
) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    let mut has_app_bundle = path
        .file_name()
        .map(|name| name.to_string_lossy().ends_with(".app"))
        .unwrap_or(false);
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

pub(super) fn get_profile_platform(app: &AppHandle, profile_id: &str) -> String {
    let profiles_path = crate::utils::paths::app_data_dir(app)
        .unwrap_or_default()
        .join("profiles.json");
    if profiles_path.exists() {
        if let Ok(data) = fs::read_to_string(&profiles_path) {
            if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                if let Some(profile) = profiles
                    .iter()
                    .find(|p| p["id"].as_str() == Some(profile_id))
                {
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

pub(super) fn get_profile_distribution(app: &AppHandle, profile_id: &str) -> String {
    let profiles_path = crate::utils::paths::app_data_dir(app)
        .unwrap_or_default()
        .join("profiles.json");
    if profiles_path.exists() {
        if let Ok(data) = fs::read_to_string(&profiles_path) {
            if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                if let Some(profile) = profiles
                    .iter()
                    .find(|p| p["id"].as_str() == Some(profile_id))
                {
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

pub(super) fn get_profile_launch_mode(app: &AppHandle, profile_id: &str) -> String {
    let profiles_path = crate::utils::paths::app_data_dir(app)
        .unwrap_or_default()
        .join("profiles.json");
    if profiles_path.exists() {
        if let Ok(data) = fs::read_to_string(&profiles_path) {
            if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                if let Some(profile) = profiles
                    .iter()
                    .find(|p| p["id"].as_str() == Some(profile_id))
                {
                    if let Some(launch_mode) = profile["launchMode"].as_str() {
                        if launch_mode == "auto"
                            || launch_mode == "steam"
                            || launch_mode == "direct"
                        {
                            return launch_mode.to_string();
                        }
                    }
                }
            }
        }
    }
    "auto".to_string()
}

pub(super) fn should_manage_steam_launch_options(distribution: &str, launch_mode: &str) -> bool {
    distribution == "steam" && launch_mode != "direct"
}

pub(super) fn profile_prefers_direct_launch(launch_mode: &str) -> bool {
    launch_mode == "direct"
}

pub(super) fn canonicalize_or_original(path: &std::path::Path) -> std::path::PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn game_path_matches_install_root(
    game_path: &std::path::Path,
    install_root: &std::path::Path,
) -> bool {
    let canonical_game = canonicalize_or_original(game_path);
    let canonical_install = canonicalize_or_original(install_root);
    canonical_game == canonical_install
        || canonical_game.starts_with(&canonical_install)
        || canonical_install.starts_with(&canonical_game)
}

/// Save a one-time vanilla backup of Assembly-CSharp.dll before any patching.
/// The backup is written to Assembly-CSharp.dll.vanilla and is NEVER overwritten.
/// Also checks the legacy .bak path as a fallback source.
pub(crate) fn backup_outerwilds_vanilla_dll(game_path: &std::path::Path) -> Result<(), String> {
    let managed_dir = game_path.join("OuterWilds_Data").join("Managed");
    if !managed_dir.exists() {
        return Ok(());
    }

    let vanilla_path = managed_dir.join("Assembly-CSharp.dll.vanilla");
    if vanilla_path.exists() {
        // Backup already exists — never overwrite it.
        return Ok(());
    }

    // Prefer the legacy .bak as source if present (created by the original OWML.Launcher.exe).
    let bak_path = managed_dir.join("Assembly-CSharp.dll.bak");
    let dll_path = managed_dir.join("Assembly-CSharp.dll");

    let src = if bak_path.exists() {
        &bak_path
    } else if dll_path.exists() {
        &dll_path
    } else {
        eprintln!("[OuterWilds] WARNING: Assembly-CSharp.dll not found, cannot create vanilla backup.");
        return Ok(());
    };

    fs::copy(src, &vanilla_path)
        .map_err(|e| format!("Failed to create vanilla DLL backup {:?}: {}", vanilla_path, e))?;
    eprintln!("[OuterWilds] Created vanilla backup from {:?} -> {:?}", src, vanilla_path);
    Ok(())
}

/// Restore the vanilla Assembly-CSharp.dll by copying the .vanilla backup.
/// Falls back to .bak (legacy) if .vanilla is absent.
/// Does NOT touch mscorlib.dll — that file must not be swapped.
pub fn restore_outerwilds_vanilla(game_path: &std::path::Path) -> Result<(), String> {
    let managed_dir = game_path.join("OuterWilds_Data").join("Managed");
    if !managed_dir.exists() {
        return Ok(());
    }

    let dll_path = managed_dir.join("Assembly-CSharp.dll");
    let vanilla_path = managed_dir.join("Assembly-CSharp.dll.vanilla");
    let bak_path = managed_dir.join("Assembly-CSharp.dll.bak");

    let src = if vanilla_path.exists() {
        Some(vanilla_path)
    } else if bak_path.exists() {
        Some(bak_path)
    } else {
        None
    };

    match src {
        Some(src_path) => {
            fs::copy(&src_path, &dll_path)
                .map_err(|e| format!("Failed to restore vanilla Assembly-CSharp.dll from {:?}: {}", src_path, e))?;
            eprintln!("[OuterWilds] Restored vanilla Assembly-CSharp.dll from {:?}", src_path);
        }
        None => {
            eprintln!("[OuterWilds] WARNING: No vanilla DLL backup found (tried .vanilla, .bak). Cannot restore clean state.");
        }
    }
    Ok(())
}

/// No-op placeholder kept for callers in sync.rs that previously called restore_outerwilds_modded.
/// The actual patching is now always deferred to launch time via run_owml_patcher().
/// This function just ensures the vanilla base is in place so the launcher can patch it cleanly.
#[allow(dead_code)]
pub fn restore_outerwilds_modded(game_path: &std::path::Path) -> Result<(), String> {
    // Ensure vanilla base is ready for the patcher that will run at launch time.
    // We do NOT try to swap back a previously-patched DLL — that approach led to double-patching.
    restore_outerwilds_vanilla(game_path)
}

/// Restore the vanilla mscorlib.dll from mscorlib.dll.bak.
/// If `delete_bak` is true, the backup file is deleted after successful restoration.
pub fn restore_mscorlib_vanilla(game_path: &std::path::Path, delete_bak: bool) -> Result<(), String> {
    let managed_dir = game_path.join("OuterWilds_Data").join("Managed");
    if !managed_dir.exists() {
        return Ok(());
    }

    let mscorlib_path = managed_dir.join("mscorlib.dll");
    let bak_path = managed_dir.join("mscorlib.dll.bak");

    if bak_path.exists() {
        fs::copy(&bak_path, &mscorlib_path)
            .map_err(|e| format!("Failed to restore vanilla mscorlib.dll from {:?}: {}", bak_path, e))?;
        eprintln!("[OuterWilds] Restored vanilla mscorlib.dll from {:?}", bak_path);
        if delete_bak {
            let _ = fs::remove_file(&bak_path);
            eprintln!("[OuterWilds] Cleaned up mscorlib.dll.bak");
        }
    }
    Ok(())
}
