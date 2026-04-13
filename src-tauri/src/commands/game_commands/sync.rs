use super::*;
use tauri::command;

#[command]
pub async fn sync_profile_to_game(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
    use_legacy_cache: Option<bool>,
) -> Result<serde_json::Value, String> {
    let use_cache = use_legacy_cache.unwrap_or(false);

    // 1. Read profile mods and platform from profiles.json
    let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;

    let profile = profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(&profile_id))
        .ok_or("Profile not found")?;
    let profile_platform = profile["platform"]
        .as_str()
        .unwrap_or("windows")
        .to_string();
    let profile_is_vanilla = profile["is_vanilla"].as_bool().unwrap_or(false);

    // 2. Get game path for this specific profile platform
    let game_path_str = get_game_path(
        app.clone(),
        game_identifier.clone(),
        Some(profile_platform.clone()),
    )
    .await?
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
    let profile_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(&profile_id);
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
    let mut desired_full_by_key: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut desired_version_by_key: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for full_name in &profile_mod_full_names {
        let key = extract_mod_key(full_name);
        desired_key_set.insert(key.clone());
        desired_full_by_key.insert(key.clone(), full_name.to_lowercase());
        if let Some(version) = extract_version_suffix(full_name) {
            desired_version_by_key.insert(key, version);
        }
    }

    eprintln!(
        "[sync_profile_to_game] Profile has {} mods",
        profile_mod_full_names.len()
    );

    let is_balatro_profile = profile["platform"].as_str() == Some("mac")
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));

    if is_balatro_profile {
        let mods_root = get_balatro_mods_dir().ok_or_else(|| {
            "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string()
        })?;
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
                .map(|full| {
                    folder_name.to_lowercase() != *full
                        && !folder_name.eq_ignore_ascii_case("smods")
                })
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

                let has_exact_version =
                    game_mod_folders
                        .iter()
                        .any(|(folder_name, gm_key, game_version)| {
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
    let removed_by_manifest =
        cleanup_owned_mod_manifests(runtime_game_path, &manifests_to_remove, &manifests_to_keep)?;
    let stale_generated_removed =
        cleanup_stale_generated_mod_artifacts(runtime_game_path, &profile_mod_full_names)?;
    if removed_by_manifest > 0 || stale_generated_removed > 0 {
        eprintln!(
            "[sync_profile_to_game] Cleaned {} tracked manifests and {} stale generated artifacts",
            removed_by_manifest, stale_generated_removed
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
                    if game_mod_folder_has_payload(
                        &game_plugins,
                        &entry.path(),
                        &folder_name,
                        &mod_key,
                    ) {
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

    eprintln!(
        "[sync_profile_to_game] To remove: {:?}, To install: {:?}",
        to_remove.len(),
        to_install.len()
    );

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
                        eprintln!(
                            "[sync_profile_to_game] Caching mod from game: {}",
                            folder_name
                        );
                        if copy_dir_recursive(&entry.path(), &cache_path).is_ok() {
                            cached += 1;
                        }
                    }
                }
            }
        }

        if cached > 0 {
            eprintln!(
                "[sync_profile_to_game] Cached {} mods from game to profile",
                cached
            );
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
