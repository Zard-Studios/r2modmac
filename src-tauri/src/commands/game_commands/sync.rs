use super::*;
use crate::tracing::{perfetto_te_ns, scoped_track_event, EventContext, TrackEventDebugArg};

fn ensure_finalize_ready(finalize: bool, missing_payloads: usize) -> Result<(), String> {
    if finalize && missing_payloads > 0 {
        return Err(format!(
            "Cannot finalize profile while {} mod(s) are still missing",
            missing_payloads
        ));
    }
    Ok(())
}
use tauri::command;

#[command]
pub async fn sync_profile_to_game(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
    use_legacy_cache: Option<bool>,
    finalize: Option<bool>,
) -> Result<serde_json::Value, String> {
    scoped_track_event!(
        "install",
        "sync_profile_to_game",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id.as_str()));
            ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier.as_str()));
        }
    );
    let use_cache = use_legacy_cache.unwrap_or(false);
    let finalize = finalize.unwrap_or(false);

    // 1. Read profile mods and platform from profiles.json
    let profiles_path = crate::utils::paths::app_data_dir(&app)
        .unwrap()
        .join("profiles.json");
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
        && !is_outerwilds_identifier(&game_identifier)
        && !is_outerwilds_game_path(game_path)
        && !is_risk_of_rain_returns_identifier(&game_identifier)
        && !is_risk_of_rain_returns_game_path(game_path)
    {
        let resolved = resolve_macos_runtime_root(game_path);
        if resolved != game_path {
            log::info!(
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
    let game_plugins = if runtime_game_path.join("BepInEx_DISABLED").is_dir() {
        runtime_game_path.join("BepInEx_DISABLED").join("plugins")
    } else {
        runtime_game_path.join("BepInEx").join("plugins")
    };

    // Profile cache path
    let profile_dir = crate::utils::paths::app_data_dir(&app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(&profile_id);
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");
    let app_data_dir = crate::utils::paths::app_data_dir(&app).map_err(|e| e.to_string())?;

    crate::utils::config_backup::apply_profile_configs(
        &app_data_dir,
        &profile_id,
        game_path,
        runtime_game_path,
        &game_identifier,
    );

    log::info!(
        "[sync_profile_to_game] Syncing profile {} to game {:?} (runtime root {:?}, legacy_cache: {}, finalize: {})",
        profile_id,
        game_path,
        runtime_game_path,
        use_cache,
        finalize
    );

    let is_outerwilds_profile =
        is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(game_path);
    let is_risk_of_rain_returns_profile = is_risk_of_rain_returns_identifier(&game_identifier)
        || is_risk_of_rain_returns_game_path(game_path);

    // Get list of mod names from profile (format: "Author-ModName-Version")
    // We keep the full name for matching
    // IMPORTANT: Only include ENABLED mods for BepInEx (disabled mods should not be installed).
    // For Outer Wilds, OWML uses config.json "enabled" flag — mods should stay physically present
    // in OWML/Mods even when individually disabled. A separate all-mods set is built inside the
    // OWML branch to prevent disabled mods from being physically removed.
    let profile_mod_full_names: Vec<String> = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|m| m["enabled"].as_bool().unwrap_or(true))
        .filter_map(|m| m["fullName"].as_str().map(|s| s.to_string()))
        .collect();

    let is_mac_profile = profile_platform == "mac";
    let is_balatro_profile = is_mac_profile
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));
    let profile_requires_bepinex = profile_mod_full_names
        .iter()
        .any(|name| name.to_lowercase().contains("bepinexpack"));

    if is_mac_profile
        && !is_balatro_profile
        && !is_outerwilds_profile
        && !is_risk_of_rain_returns_profile
        && profile_requires_bepinex
    {
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

    log::debug!(
        "[sync_profile_to_game] Profile has {} mods",
        profile_mod_full_names.len()
    );

    let is_balatro_profile = profile["platform"].as_str() == Some("mac")
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));

    // --- Outer Wilds (OWML) sync branch ---
    if is_outerwilds_profile {
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if !profile_is_vanilla {
            if owml_disabled.exists() && !owml_folder.exists() {
                let _ = fs::rename(&owml_disabled, &owml_folder);
                log::info!("[sync_profile_to_game] Restored OWML_DISABLED -> OWML");
            }
        }

        // Resolve the actual OWML directory: it may be OWML or OWML_DISABLED depending on
        // whether the profile is currently in vanilla mode. After the potential restore above
        // (lines 150-154), OWML should be back if !profile_is_vanilla. For vanilla mode, the
        // folder may still be at OWML_DISABLED from a previous sync, so we check both.
        let owml_dir = if owml_folder.exists() {
            owml_folder.clone()
        } else if owml_disabled.exists() {
            // OWML was previously disabled (vanilla mode). Use OWML_DISABLED as the working dir
            // for reading mods and writing config.json so that individually-disabled mods are
            // correctly updated even while the profile stays in vanilla mode.
            owml_disabled.clone()
        } else {
            // Neither exists yet (fresh install). Default to OWML.
            owml_folder.clone()
        };
        let owml_mods_dir = owml_dir.join("Mods");
        let profile_owml_cache = profile_dir.join("OWML").join("Mods");

        // Read manifest.json uniqueName from a mod folder to derive the mod key.
        let read_owml_unique_name = |mod_dir: &std::path::Path| -> Option<String> {
            let manifest_path = mod_dir.join("manifest.json");
            if let Ok(data) = fs::read_to_string(&manifest_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(unique_name) = v["uniqueName"].as_str() {
                        // Convert dots to hyphens for Thunderstore key matching
                        return Some(unique_name.replace('.', "-").to_lowercase());
                    }
                }
            }
            None
        };

        // Read installed version from a mod's manifest.json
        let read_owml_version = |mod_dir: &std::path::Path| -> Option<String> {
            let manifest_path = mod_dir.join("manifest.json");
            if let Ok(data) = fs::read_to_string(&manifest_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                    return v["version"].as_str().map(|s| s.to_string());
                }
            }
            None
        };

        let owml_installed = owml_dir.join("OWML.Launcher.exe").exists();

        // Scan installed mods
        let mut game_mod_folders: Vec<(String, String, Option<String>)> = vec![]; // (folder, key, version)
        if owml_mods_dir.exists() {
            if let Ok(entries) = fs::read_dir(&owml_mods_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.path().is_dir() {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        let mod_key = read_owml_unique_name(&entry.path())
                            .unwrap_or_else(|| extract_mod_key(&folder_name));
                        let version = read_owml_version(&entry.path())
                            .or_else(|| extract_version_suffix(&folder_name));
                        game_mod_folders.push((folder_name, mod_key, version));
                    }
                }
            }
        }

        // Build a set of ALL mod keys in the profile (enabled + disabled). OWML keeps mods
        // physically present on disk and uses config.json to enable/disable them, so we must
        // NOT remove a mod just because the user individually disabled it in the UI.
        let owml_all_key_set: std::collections::HashSet<String> = {
            let mut set = std::collections::HashSet::new();
            if let Some(arr) = profile["mods"].as_array() {
                for m in arr {
                    if let Some(full_name) = m["fullName"].as_str() {
                        set.insert(extract_mod_key(full_name));
                    }
                }
            }
            set
        };

        let mut to_remove: Vec<String> = Vec::new();
        for (folder_name, gm_key, game_version) in &game_mod_folders {
            // Use owml_all_key_set so disabled mods are NOT removed from disk.
            if !owml_all_key_set.contains(gm_key) {
                to_remove.push(folder_name.clone());
                continue;
            }

            // Only check version replacement for mods that are ENABLED (and thus will be
            // re-installed if out of date). Disabled mods stay at whatever version they have.
            if !desired_key_set.contains(gm_key) {
                continue;
            }

            let desired_version = desired_version_by_key.get(gm_key);
            let needs_replacement = match desired_version {
                Some(dv) => match game_version.as_ref() {
                    Some(gv) => gv != dv,
                    None => false,
                },
                None => false,
            };
            if needs_replacement {
                to_remove.push(folder_name.clone());
            }
        }

        let mut to_install: Vec<String> = desired_key_set
            .iter()
            .filter(|pm_key| {
                // If this is the OWML loader itself, check if already installed
                if pm_key.contains("owml") || pm_key.contains("outerwildsmodmanager") {
                    return !owml_installed;
                }

                let desired_version = desired_version_by_key.get(*pm_key);

                let has_exact_version = game_mod_folders.iter().any(|(_, gm_key, game_version)| {
                    if gm_key != *pm_key {
                        return false;
                    }
                    if let Some(dv) = desired_version {
                        if let Some(gv) = game_version {
                            return gv == dv;
                        }
                        return false;
                    }
                    true
                });

                !has_exact_version
            })
            .map(|k| k.to_string())
            .collect();
        to_install.sort();

        ensure_finalize_ready(finalize, to_install.len())?;

        // Remove mods only after every desired payload has been installed.
        let mut removed = 0;
        if finalize {
            for folder_name in &to_remove {
                let folder_path = owml_mods_dir.join(folder_name);
                if folder_path.exists() {
                    let _ = fs::remove_dir_all(&folder_path);
                    removed += 1;
                }
            }
        }

        // Cache: copy installed mods to profile cache
        let mut cached = 0;
        if finalize && use_cache && owml_mods_dir.exists() {
            if !profile_owml_cache.exists() {
                let _ = fs::create_dir_all(&profile_owml_cache);
            }
            if let Ok(entries) = fs::read_dir(&owml_mods_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.path().is_dir() {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        let cache_path = profile_owml_cache.join(&folder_name);
                        if !cache_path.exists() {
                            if copy_dir_recursive(&entry.path(), &cache_path).is_ok() {
                                cached += 1;
                            }
                        }
                    }
                }
            }
        }

        // OWML detects Wine via wine_get_version() and prepends "Z:" to gamePath/owmlPath.
        // So these must be Unix-style paths relative to drive_c root: /Program Files/...
        // NOT Windows-style C:\Program Files\ (which would become Z:C:\... = broken)
        // Always use the canonical "OWML" path (not "OWML_DISABLED") in OWML.Config.json,
        // because the folder will be restored to OWML/ before launch.
        let game_wine_path = convert_to_owml_unix_path(game_path);
        let owml_canonical_dir = game_path.join("OWML");
        let owml_unix_path_raw = convert_to_owml_unix_path(&owml_canonical_dir);
        let mut owml_wine_path = owml_unix_path_raw.clone();
        if !owml_wine_path.ends_with('/') {
            owml_wine_path.push('/');
        }

        // Write/update OWML.Config.json with socketPort=0 to prevent Wine socket crashes.
        // This is done on every sync so the setting is always enforced.
        if owml_dir.exists() {
            let config_path = owml_dir.join("OWML.Config.json");
            let mut config: serde_json::Value = if config_path.exists() {
                fs::read_to_string(&config_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            // Force socketPort=0 to prevent SocketListener from binding in Wine/CrossOver
            config["socketPort"] = serde_json::json!(0);
            config["incrementalGC"] = serde_json::json!(true);
            config["gamePath"] = serde_json::json!(&game_wine_path);
            config["owmlPath"] = serde_json::json!(&owml_wine_path);
            if let Some(obj) = config.as_object_mut() {
                obj.entry("forceExe").or_insert(serde_json::json!(false));
                obj.entry("disableVersionPopup")
                    .or_insert(serde_json::json!(true));
            }

            if let Ok(serialized) = serde_json::to_string_pretty(&config) {
                let _ = fs::write(&config_path, serialized);
                log::debug!(
                    "[sync_profile_to_game] Wrote OWML.Config.json (socketPort=0) at {:?}",
                    config_path
                );
            }
        }

        // Also write/update the OWML.Config.json in OuterWilds_Data/Managed/ to make sure it points to our OWML folder!
        let managed_dir = game_path.join("OuterWilds_Data").join("Managed");
        if managed_dir.exists() {
            let config_path = managed_dir.join("OWML.Config.json");
            let mut config: serde_json::Value = if config_path.exists() {
                fs::read_to_string(&config_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            config["gamePath"] = serde_json::json!(&game_wine_path);
            config["owmlPath"] = serde_json::json!(&owml_wine_path);
            config["socketPort"] = serde_json::json!(0);
            config["incrementalGC"] = serde_json::json!(true);
            if let Some(obj) = config.as_object_mut() {
                obj.entry("forceExe").or_insert(serde_json::json!(false));
                obj.entry("disableVersionPopup")
                    .or_insert(serde_json::json!(true));
            }

            if let Ok(serialized) = serde_json::to_string_pretty(&config) {
                let _ = fs::write(&config_path, serialized);
                log::debug!(
                    "[sync_profile_to_game] Wrote Managed/OWML.Config.json at {:?}",
                    config_path
                );
            }
        }

        // Set mod enabled/disabled state in OWML mods configs
        if owml_mods_dir.exists() {
            if let Some(mods_arr) = profile["mods"].as_array() {
                for m in mods_arr {
                    if let (Some(full_name), Some(is_enabled)) =
                        (m["fullName"].as_str(), m["enabled"].as_bool())
                    {
                        let mod_key = extract_mod_key(full_name);

                        // Find installed folder for this mod_key
                        let mut found_folder = None;
                        if let Ok(entries) = fs::read_dir(&owml_mods_dir) {
                            for entry in entries.filter_map(|e| e.ok()) {
                                if entry.path().is_dir() {
                                    let f_name = entry.file_name().to_string_lossy().to_string();
                                    let m_key = read_owml_unique_name(&entry.path())
                                        .unwrap_or_else(|| extract_mod_key(&f_name));
                                    if m_key == mod_key {
                                        found_folder = Some(f_name);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(folder) = found_folder {
                            let mod_config_path = owml_mods_dir.join(&folder).join("config.json");

                            let mut config: serde_json::Value = if mod_config_path.exists() {
                                fs::read_to_string(&mod_config_path)
                                    .ok()
                                    .and_then(|s| serde_json::from_str(&s).ok())
                                    .unwrap_or_else(|| serde_json::json!({}))
                            } else {
                                // Fallback: try default-config.json first if config.json doesn't exist
                                let def_config_path =
                                    owml_mods_dir.join(&folder).join("default-config.json");
                                if def_config_path.exists() {
                                    fs::read_to_string(&def_config_path)
                                        .ok()
                                        .and_then(|s| serde_json::from_str(&s).ok())
                                        .unwrap_or_else(|| serde_json::json!({}))
                                } else {
                                    serde_json::json!({})
                                }
                            };

                            config["enabled"] = serde_json::json!(is_enabled);

                            if let Ok(serialized) = serde_json::to_string_pretty(&config) {
                                let _ = fs::write(&mod_config_path, serialized);
                                log::debug!(
                                    "[sync_profile_to_game] Set Outer Wilds mod {} enabled={}",
                                    folder,
                                    is_enabled
                                );
                            }
                        }
                    }
                }
            }
        }

        // Toggle vanilla mode via shared helpers (covers Assembly-CSharp + mscorlib)
        if profile_is_vanilla {
            let _ = restore_outerwilds_vanilla(&game_path);
            let _ = restore_mscorlib_vanilla(&game_path, false);
            if owml_folder.exists() {
                if owml_disabled.exists() {
                    let _ = fs::remove_dir_all(&owml_disabled);
                }
                let _ = fs::rename(&owml_folder, &owml_disabled);
                log::info!("[sync_profile_to_game] Renamed OWML -> OWML_DISABLED");
            }
        } else {
            if owml_disabled.exists() && !owml_folder.exists() {
                let _ = fs::rename(&owml_disabled, &owml_folder);
                log::info!("[sync_profile_to_game] Restored OWML_DISABLED -> OWML");
            }
            let _ = restore_outerwilds_modded(&game_path);
        }

        if finalize {
            crate::utils::config_backup::capture_profile_configs(
                &app_data_dir,
                &profile_id,
                game_path,
                runtime_game_path,
                &game_identifier,
            );
        }

        return Ok(serde_json::json!({
            "removed": removed,
            "to_install": to_install,
            "already_installed": game_mod_folders.len(),
            "cached": cached,
            "pending_removals": if finalize { 0 } else { to_remove.len() }
        }));
    }

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

        ensure_finalize_ready(finalize, to_install.len())?;

        let mut removed = 0;
        if finalize {
            for folder_name in &to_remove {
                let folder_path = mods_root.join(folder_name);
                if folder_path.exists() {
                    let _ = fs::remove_dir_all(&folder_path);
                    removed += 1;
                }
            }
        }

        let mut cached = 0;
        if finalize && use_cache && mods_root.exists() {
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
            "cached": cached,
            "pending_removals": if finalize { 0 } else { to_remove.len() }
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
                        log::debug!(
                            "[sync_profile_to_game] Detected broken/metadata-only mod folder: {}",
                            folder_name
                        );
                        invalid_game_mod_folders.push(folder_name);
                    }
                }
            }
        }
    }

    log::warn!(
        "[sync_profile_to_game] Game has {} valid mods installed ({} broken placeholders)",
        game_mod_folders.len(),
        invalid_game_mod_folders.len()
    );

    // 4. Calculate diff using Author-ModName key + version awareness.
    // Remove entries not present in profile OR with a mismatched pinned version.
    let mut to_remove: Vec<String> = invalid_game_mod_folders.clone();
    for (folder_name, gm_key) in &game_mod_folders {
        if !desired_key_set.contains(gm_key) {
            // Check if this folder is owned by any manifest we want to keep
            let folder_prefix_1 = format!("bepinex/plugins/{}/", folder_name.to_lowercase());
            let folder_prefix_2 =
                format!("bepinex_disabled/plugins/{}/", folder_name.to_lowercase());
            let folder_exact_1 = format!("bepinex/plugins/{}", folder_name.to_lowercase());
            let folder_exact_2 = format!("bepinex_disabled/plugins/{}", folder_name.to_lowercase());
            let is_owned_by_kept_manifest = manifests_to_keep.iter().any(|entry| {
                entry.manifest.files.iter().any(|file| {
                    let file_lower = file.to_lowercase();
                    file_lower.starts_with(&folder_prefix_1)
                        || file_lower.starts_with(&folder_prefix_2)
                        || file_lower == folder_exact_1
                        || file_lower == folder_exact_2
                })
            });
            if is_owned_by_kept_manifest {
                log::debug!(
                    "[sync_profile_to_game] Keeping folder owned by kept manifest: {}",
                    folder_name
                );
                continue;
            }

            if game_mod_folder_is_auxiliary_payload(folder_name, gm_key, &desired_full_by_key) {
                log::debug!(
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
        let game_version = extract_version_suffix(folder_name)
            .or_else(|| read_manifest_version(&game_plugins.join(folder_name)));
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
        windows_bepinex_runtime_is_installed(game_path)
    };
    log::debug!(
        "[sync_profile_to_game] bepinex_installed={} (BepInExPack is skipped from to_install when true)",
        bepinex_installed
    );

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

            let has_exact_version = manifests_to_keep.iter().any(|entry| {
                entry.manifest.mod_key == **pm_key
                    && manifest_files_exist(runtime_game_path, &entry.manifest.files)
            }) || game_mod_folders.iter().any(|(folder_name, gm_key)| {
                if gm_key != *pm_key {
                    return false;
                }

                if let Some(dv) = desired_version {
                    let game_version = extract_version_suffix(folder_name)
                        .or_else(|| read_manifest_version(&game_plugins.join(folder_name)));
                    if let Some(gv) = game_version {
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

    // Names, not just counts: a mod silently absent from `to_install` is the
    // failure mode that is impossible to diagnose from a count alone.
    log::debug!(
        "[sync_profile_to_game] To remove ({}): {:?}",
        to_remove.len(),
        to_remove
    );
    log::debug!(
        "[sync_profile_to_game] To install ({}): {:?}",
        to_install.len(),
        to_install
    );
    log::debug!(
        "[sync_profile_to_game] Profile wants {} mods, kept {} manifests, dropping {} manifests",
        desired_key_set.len(),
        manifests_to_keep.len(),
        manifests_to_remove.len()
    );

    // 5. Remove mods not in profile (we have the exact folder names from the tuple)
    ensure_finalize_ready(finalize, to_install.len())?;
    let mut removed = 0;
    if finalize {
        let removed_by_manifest = cleanup_owned_mod_manifests(
            runtime_game_path,
            &manifests_to_remove,
            &manifests_to_keep,
        )?;
        let stale_generated_removed =
            cleanup_stale_generated_mod_artifacts(runtime_game_path, &profile_mod_full_names)?;
        removed += removed_by_manifest + stale_generated_removed;
        if removed_by_manifest > 0 || stale_generated_removed > 0 {
            log::debug!(
                "[sync_profile_to_game] Cleaned {} tracked manifests and {} stale generated artifacts",
                removed_by_manifest, stale_generated_removed
            );
        }
        for folder_name in &to_remove {
            let folder_path = game_plugins.join(folder_name);
            if folder_path.exists() {
                log::debug!("[sync_profile_to_game] Removing: {}", folder_name);
                if remove_plugin_entry(&folder_path).is_ok() {
                    if !removed_manifest_keys.contains(&extract_mod_key(folder_name)) {
                        removed += 1;
                    }
                }
            }
        }
    }

    // 6. If legacy cache enabled, copy mods from game to profile cache (reverse sync)
    let mut cached = 0;
    if finalize && use_cache && game_plugins.exists() {
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
                        log::debug!(
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
            log::debug!(
                "[sync_profile_to_game] Cached {} mods from game to profile",
                cached
            );
        }
    }

    if finalize {
        crate::utils::config_backup::capture_profile_configs(
            &app_data_dir,
            &profile_id,
            game_path,
            runtime_game_path,
            &game_identifier,
        );
    }

    // 7. Return info about what needs to be installed (frontend will handle download)
    let to_install_names: Vec<String> = to_install;
    let already_installed = game_mod_folders.len().saturating_sub(removed);

    Ok(serde_json::json!({
        "removed": removed,
        "to_install": to_install_names,
        "already_installed": already_installed,
        "cached": cached,
        "pending_removals": if finalize { 0 } else { to_remove.len() + manifests_to_remove.len() }
    }))
}

/// Whether a Windows/CrossOver game folder already carries a usable BepInEx.
///
/// Deleting BepInEx by hand routinely leaves an empty `BepInEx/core` behind, so
/// a bare directory check would report the runtime as installed and make
/// `to_install` skip BepInExPack forever - the runtime could then never be
/// reinstalled from the app.
fn windows_bepinex_runtime_is_installed(game_path: &std::path::Path) -> bool {
    [
        game_path.join("BepInEx").join("core"),
        game_path.join("BepInEx_DISABLED").join("core"),
    ]
    .iter()
    .any(|core_dir| super::runtime_health::core_directory_has_preloader(core_dir))
}

#[cfg(test)]
mod tests {
    use super::{ensure_finalize_ready, windows_bepinex_runtime_is_installed};

    #[test]
    fn empty_core_folder_does_not_count_as_an_installed_runtime() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-sync-bepinex-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let core = root.join("BepInEx/core");
        std::fs::create_dir_all(&core).unwrap();
        assert!(!windows_bepinex_runtime_is_installed(&root));

        std::fs::write(core.join("BepInEx.Preloader.Core.dll"), b"").unwrap();
        assert!(windows_bepinex_runtime_is_installed(&root));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn analysis_phase_allows_missing_payloads() {
        assert!(ensure_finalize_ready(false, 3).is_ok());
    }

    #[test]
    fn cleanup_phase_requires_every_payload() {
        assert!(ensure_finalize_ready(true, 0).is_ok());
        assert!(ensure_finalize_ready(true, 1).is_err());
    }
}

fn manifest_files_exist(target_root: &std::path::Path, files: &[String]) -> bool {
    if files.is_empty() {
        return false;
    }
    for file in files {
        let path = target_root.join(file);
        if !path.exists() {
            return false;
        }
    }
    true
}

fn convert_to_owml_unix_path(path: &std::path::Path) -> String {
    if cfg!(windows) {
        return path.to_string_lossy().to_string();
    }
    path.to_string_lossy().replace('\\', "/")
}
