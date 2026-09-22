use super::*;
use crate::tracing::{perfetto_te_ns, scoped_track_event, EventContext, TrackEventDebugArg};
use crate::utils::mod_manifest::save_owned_mod_manifest;

fn ensure_finalize_ready(finalize: bool, missing_payloads: usize) -> Result<(), String> {
    if finalize && missing_payloads > 0 {
        return Err(format!(
            "Cannot finalize profile while {} mod(s) are still missing",
            missing_payloads
        ));
    }
    Ok(())
}

fn managed_install_root(
    is_return_of_modding: bool,
    game_path: &std::path::Path,
    bepinex_root: std::path::PathBuf,
) -> std::path::PathBuf {
    // Profile isolation moves only a BepInEx tree. ReturnOfModding packages
    // are installed beside the game and their ownership manifests point there;
    // reconciling them against the isolated profile makes every successful
    // install appear missing during finalization (issue #38).
    if is_return_of_modding {
        game_path.to_path_buf()
    } else {
        bepinex_root
    }
}

fn return_of_modding_mods_yaml(
    profile: &serde_json::Value,
    community: &str,
) -> Result<String, String> {
    let entries = profile["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|profile_mod| {
            let full_name = profile_mod["fullName"].as_str()?;
            let version = profile_mod["versionNumber"].as_str().unwrap_or("0.0.0");
            let package_name = full_name
                .strip_suffix(&format!("-{version}"))
                .unwrap_or(full_name);
            let (namespace, short_name) = package_name
                .split_once('-')
                .unwrap_or((package_name, package_name));
            let mut version_parts = version
                .split('.')
                .map(|part| part.parse::<u64>().unwrap_or(0));
            let source_is_online = profile_mod["source"].as_str() != Some("local");
            let website_url = if source_is_online {
                format!("https://thunderstore.io/c/{community}/p/{namespace}/{short_name}/")
            } else {
                String::new()
            };

            Some(serde_json::json!({
                "manifestVersion": 1,
                "name": package_name,
                "authorName": profile_mod["author"].as_str().unwrap_or(namespace),
                "websiteUrl": website_url,
                "displayName": profile_mod["displayName"].as_str().unwrap_or(short_name),
                "description": profile_mod["description"].as_str().unwrap_or(""),
                "gameVersion": "0",
                "networkMode": "both",
                "packageType": "other",
                "installMode": "managed",
                "installedAtTime": 0,
                "loaders": [],
                "dependencies": [],
                "incompatibilities": [],
                "optionalDependencies": [],
                "versionNumber": {
                    "major": version_parts.next().unwrap_or(0),
                    "minor": version_parts.next().unwrap_or(0),
                    "patch": version_parts.next().unwrap_or(0),
                },
                "enabled": profile_mod["enabled"].as_bool().unwrap_or(true),
                "onlineSource": source_is_online,
                "trustedPackage": false,
            }))
        })
        .collect::<Vec<_>>();

    serde_yaml::to_string(&entries)
        .map_err(|error| format!("Failed to serialize ReturnOfModding mods.yml: {error}"))
}

fn write_return_of_modding_mods_yml(
    profile: &serde_json::Value,
    community: &str,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let output = game_path.join("mods.yml");
    let yaml = return_of_modding_mods_yaml(profile, community)?;
    fs::write(&output, yaml)
        .map_err(|error| format!("Failed to write ReturnOfModding mod list at {output:?}: {error}"))
}

fn set_return_of_modding_plugin_enabled(
    game_path: &std::path::Path,
    package_name: &str,
    enabled: bool,
) -> Result<bool, String> {
    let plugin_dir = game_path
        .join("ReturnOfModding")
        .join("plugins")
        .join(package_name);
    if !plugin_dir.is_dir() {
        return Ok(false);
    }

    // ReturnOfModding discovers a plugin by finding main.lua and then walking
    // up to manifest.json. It does not consume the `enabled` field in
    // mods.yml. Hide only the discovery marker instead of renaming every file
    // in the package as r2modman does; this is reversible and leaves configs
    // and plugin data untouched.
    let active_manifest = plugin_dir.join("manifest.json");
    let disabled_manifest = plugin_dir.join("manifest.json.old");
    let (source, destination) = if enabled {
        (&disabled_manifest, &active_manifest)
    } else {
        (&active_manifest, &disabled_manifest)
    };

    if !source.is_file() {
        return Ok(false);
    }
    if destination.exists() {
        return Err(format!(
            "Cannot {} ReturnOfModding plugin {package_name}: both {:?} and {:?} exist",
            if enabled { "enable" } else { "disable" },
            active_manifest,
            disabled_manifest
        ));
    }

    fs::rename(source, destination).map_err(|error| {
        format!(
            "Failed to {} ReturnOfModding plugin {package_name}: {error}",
            if enabled { "enable" } else { "disable" }
        )
    })?;
    Ok(true)
}

fn return_of_modding_package_name(full_name: &str) -> &str {
    full_name
        .rsplit_once('-')
        .filter(|(_, version)| {
            version.contains('.')
                && version
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '.')
        })
        .map(|(package, _)| package)
        .unwrap_or(full_name)
}

fn reconcile_return_of_modding_plugin_visibility(
    game_path: &std::path::Path,
    managed_packages: impl IntoIterator<Item = String>,
    active_profile_mods: &[serde_json::Value],
) -> Result<usize, String> {
    let active_states = active_profile_mods
        .iter()
        .filter_map(|profile_mod| {
            let full_name = profile_mod["fullName"].as_str()?;
            if crate::models::loaders::is_loader_package(
                &crate::models::loaders::PackageLoader::ReturnOfModding,
                full_name,
            ) {
                return None;
            }
            Some((
                return_of_modding_package_name(full_name).to_lowercase(),
                profile_mod["enabled"].as_bool().unwrap_or(true),
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();

    // ReturnOfModding has one game-side plugin directory even though
    // r2modmac exposes independent profiles. Only touch packages that an
    // r2modmac ownership manifest claims; manually installed plugins remain
    // outside our control. A package absent from the active profile is hidden
    // just like a package whose frontend toggle is off.
    let mut packages = managed_packages
        .into_iter()
        .map(|package| (package.to_lowercase(), package))
        .collect::<std::collections::HashMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    packages.sort_by_key(|package| package.to_lowercase());

    let mut changed = 0;
    for package in packages {
        let enabled = active_states
            .get(&package.to_lowercase())
            .copied()
            .unwrap_or(false);
        if set_return_of_modding_plugin_enabled(game_path, &package, enabled)? {
            changed += 1;
        }
    }
    Ok(changed)
}

fn move_return_of_modding_payload_entry(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), String> {
    if !destination.exists() {
        return fs::rename(source, destination).map_err(|error| {
            format!(
                "Failed to migrate ReturnOfModding payload {} -> {}: {error}",
                source.display(),
                destination.display()
            )
        });
    }

    if source.is_dir() && destination.is_dir() {
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            move_return_of_modding_payload_entry(
                &entry.path(),
                &destination.join(entry.file_name()),
            )?;
        }
        fs::remove_dir(source).map_err(|error| error.to_string())?;
        return Ok(());
    }

    if source.is_file()
        && destination.is_file()
        && fs::read(source).ok() == fs::read(destination).ok()
    {
        fs::remove_file(source).map_err(|error| error.to_string())?;
        return Ok(());
    }

    Err(format!(
        "Cannot migrate ReturnOfModding payload because {} already exists with different contents",
        destination.display()
    ))
}

/// Repair the layout produced by r2modmac builds that preserved a package's
/// top-level `plugins/` wrapper. ReturnOfModding searches for `main.lua`
/// directly below `Author-Mod`, so the wrapper made otherwise valid Hades II
/// packages invisible. This moves only the wrapper's direct children; nested
/// folders such as `Scripts/` remain intact.
fn migrate_nested_return_of_modding_plugins(
    game_path: &std::path::Path,
    package_name: &str,
) -> Result<bool, String> {
    let package_root = game_path
        .join("ReturnOfModding")
        .join("plugins")
        .join(package_name);
    let nested_plugins = package_root.join("plugins");
    if !nested_plugins.is_dir() {
        return Ok(false);
    }

    for entry in fs::read_dir(&nested_plugins).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        move_return_of_modding_payload_entry(&entry.path(), &package_root.join(entry.file_name()))?;
    }
    fs::remove_dir(&nested_plugins).map_err(|error| error.to_string())?;
    Ok(true)
}

fn flatten_return_of_modding_manifest_paths(
    files: &[String],
    package_name: &str,
) -> Vec<std::path::PathBuf> {
    let nested_prefix = format!("ReturnOfModding/plugins/{package_name}/plugins/");
    let flat_prefix = format!("ReturnOfModding/plugins/{package_name}/");
    files
        .iter()
        .map(|file| {
            std::path::PathBuf::from(
                file.strip_prefix(&nested_prefix)
                    .map(|suffix| format!("{flat_prefix}{suffix}"))
                    .unwrap_or_else(|| file.clone()),
            )
        })
        .collect()
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
    let is_return_of_modding_profile =
        crate::models::loaders::uses_return_of_modding(&game_identifier, game_path);
    let runtime_game_path_buf = if profile_platform == "mac"
        && !is_balatro_identifier(&game_identifier)
        && !is_balatro_game_path(game_path)
        && !is_outerwilds_identifier(&game_identifier)
        && !is_outerwilds_game_path(game_path)
        && !is_return_of_modding_profile
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
    // Under isolation the tree the game loads lives in the profile, so that is
    // the tree to reconcile against.
    let bepinex_root = managed_install_root(
        is_return_of_modding_profile,
        runtime_game_path,
        bepinex_install_root(&app, &profile_id, runtime_game_path)?,
    );
    let profile_isolated = bepinex_root != runtime_game_path;
    if !profile_isolated && runtime_game_path.join("BepInEx").is_symlink() {
        // A prior isolated profile may have left its game-side alias. Never
        // reconcile a game-local profile through another profile's tree.
        crate::commands::game_commands::detach_isolated_bepinex_link(runtime_game_path)?;
    }
    let bepinex_scope = if profile_isolated {
        PROFILE_MANIFEST_SCOPE
    } else {
        GAME_MANIFEST_SCOPE
    };
    // A profile that was installed before isolation has its tree in the game.
    // Move it once, rather than leaving the user with an empty profile and a
    // game full of mods nobody claims.
    if profile_isolated
        && !bepinex_root.join("BepInEx").is_dir()
        && !bepinex_root.join("BepInEx_DISABLED").is_dir()
        && ((runtime_game_path.join("BepInEx").is_dir()
            && !runtime_game_path.join("BepInEx").is_symlink())
            || (runtime_game_path.join("BepInEx_DISABLED").is_dir()
                && !runtime_game_path.join("BepInEx_DISABLED").is_symlink()))
    {
        log::info!(
            "[sync_profile_to_game] Moving the existing tree from {:?} into {:?}",
            runtime_game_path,
            bepinex_root
        );
        crate::commands::mod_commands::relocate_bepinex_tree(runtime_game_path, &bepinex_root)?;
        crate::commands::mod_commands::point_game_doorstop_ini_at_tree(
            runtime_game_path,
            &bepinex_root,
        )?;
    }

    let game_plugins = if bepinex_root.join("BepInEx_DISABLED").is_dir() {
        bepinex_root.join("BepInEx_DISABLED").join("plugins")
    } else {
        bepinex_root.join("BepInEx").join("plugins")
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
        &bepinex_root,
        &game_identifier,
    );

    log::info!(
        "[sync_profile_to_game] Syncing profile {} to game {:?} (runtime root {:?}, BepInEx root {:?}, legacy_cache: {}, finalize: {})",
        profile_id,
        game_path,
        runtime_game_path,
        bepinex_root,
        use_cache,
        finalize
    );

    let is_outerwilds_profile =
        is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(game_path);
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
        && !is_return_of_modding_profile
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

    // --- shimloader sync branch ---
    //
    // Nothing about a shimloader mod is copied into the game: the mods, paks
    // and configs stay in the profile and are handed to the loader as
    // `--mod-dir`/`--pak-dir`/`--cfg-dir`/`--overlay-dir` at launch. Applying a
    // profile is therefore only about the runtime files, which do have to sit
    // next to the game executable for the game to load them.
    if crate::models::loaders::uses_shimloader(&game_identifier) {
        let stored = load_owned_mod_manifests(&app, &profile_id, PROFILE_MANIFEST_SCOPE)?
            .into_iter()
            .filter(|entry| manifest_matches_target_root(&entry.manifest, &profile_dir))
            .collect::<Vec<_>>();
        let (manifests_to_remove, manifests_to_keep): (Vec<_>, Vec<_>) =
            stored.into_iter().partition(|entry| {
                match desired_full_by_key.get(&entry.manifest.mod_key) {
                    Some(full) => full != &entry.manifest.mod_full_name.to_lowercase(),
                    None => true,
                }
            });

        let installed_keys = manifests_to_keep
            .iter()
            .filter(|entry| manifest_files_exist(&profile_dir, &entry.manifest.files))
            .map(|entry| entry.manifest.mod_key.clone())
            .collect::<std::collections::HashSet<_>>();
        let mut to_install = desired_key_set
            .iter()
            .filter(|key| !installed_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        to_install.sort();

        ensure_finalize_ready(finalize, to_install.len())?;

        let mut removed = 0;
        if finalize {
            removed += cleanup_owned_mod_manifests(
                &profile_dir,
                &manifests_to_remove,
                &manifests_to_keep,
            )?;
        }

        log::info!(
            "[sync_profile_to_game] Shimloader profile {}: {} to install, {} removed, vanilla={}",
            profile_id,
            to_install.len(),
            removed,
            profile_is_vanilla
        );

        return Ok(serde_json::json!({
            "removed": removed,
            "to_install": to_install,
            "already_installed": installed_keys.len(),
            "cached": 0,
            "pending_removals": if finalize { 0 } else { manifests_to_remove.len() }
        }));
    }

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
                &bepinex_root,
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

    // ReturnOfModding keeps disabled plugins on disk, but it does not read the
    // enabled field in mods.yml. Reusing the BepInEx reconciliation below
    // would delete disabled plugins; this branch instead hides/restores each
    // plugin's manifest.json, which is the runtime's discovery marker.
    if is_return_of_modding_profile {
        let all_profile_mods = profile["mods"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let mut managed_plugin_packages = std::collections::HashSet::new();
        for candidate_profile in &profiles {
            let Some(candidate_profile_id) = candidate_profile["id"].as_str() else {
                continue;
            };
            let candidate_manifests = match load_owned_mod_manifests(
                &app,
                candidate_profile_id,
                GAME_MANIFEST_SCOPE,
            ) {
                Ok(manifests) => manifests,
                Err(error) => {
                    log::warn!(
                            "[sync_profile_to_game] Could not inspect ReturnOfModding ownership for profile {}: {}",
                            candidate_profile_id,
                            error
                        );
                    continue;
                }
            };
            for entry in candidate_manifests {
                if !manifest_matches_target_root(&entry.manifest, runtime_game_path)
                    || crate::models::loaders::is_loader_package(
                        &crate::models::loaders::PackageLoader::ReturnOfModding,
                        &entry.manifest.mod_full_name,
                    )
                {
                    continue;
                }
                managed_plugin_packages.insert(
                    return_of_modding_package_name(&entry.manifest.mod_full_name).to_string(),
                );
            }
        }
        let all_profile_full_names = all_profile_mods
            .iter()
            .filter_map(|mod_entry| mod_entry["fullName"].as_str())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let all_profile_full_by_key = all_profile_full_names
            .iter()
            .map(|full_name| (extract_mod_key(full_name), full_name.to_lowercase()))
            .collect::<std::collections::HashMap<_, _>>();

        let mut stored = load_owned_mod_manifests(&app, &profile_id, GAME_MANIFEST_SCOPE)?
            .into_iter()
            .filter(|entry| manifest_matches_target_root(&entry.manifest, runtime_game_path))
            .collect::<Vec<_>>();
        // Profiles created by the affected builds already look "installed" to
        // reconciliation, so merely fixing new extraction would strand their
        // nested payload forever. Repair it in place and rewrite the ownership
        // manifest so later updates/removals still own the correct files.
        for entry in &mut stored {
            if crate::models::loaders::is_loader_package(
                &crate::models::loaders::PackageLoader::ReturnOfModding,
                &entry.manifest.mod_full_name,
            ) {
                continue;
            }
            let package_name = return_of_modding_package_name(&entry.manifest.mod_full_name);
            if migrate_nested_return_of_modding_plugins(runtime_game_path, package_name)? {
                let flattened =
                    flatten_return_of_modding_manifest_paths(&entry.manifest.files, package_name);
                save_owned_mod_manifest(
                    &app,
                    &profile_id,
                    GAME_MANIFEST_SCOPE,
                    &entry.manifest.mod_full_name,
                    runtime_game_path,
                    &flattened,
                    &entry.manifest.backed_up_files,
                )?;
                entry.manifest.files = flattened
                    .iter()
                    .map(|path| path.to_string_lossy().replace('\\', "/"))
                    .collect();
                log::info!(
                    "[sync_profile_to_game] Flattened legacy ReturnOfModding plugins wrapper for {}",
                    entry.manifest.mod_full_name
                );
            }
        }
        let (manifests_to_remove, manifests_to_keep): (Vec<_>, Vec<_>) =
            stored.into_iter().partition(|entry| {
                all_profile_full_by_key
                    .get(&entry.manifest.mod_key)
                    .is_none_or(|full| full != &entry.manifest.mod_full_name.to_lowercase())
            });

        let installed_manifest_keys = manifests_to_keep
            .iter()
            .filter(|entry| manifest_files_exist(runtime_game_path, &entry.manifest.files))
            .map(|entry| entry.manifest.mod_key.clone())
            .collect::<std::collections::HashSet<_>>();
        let plugin_root = runtime_game_path.join("ReturnOfModding").join("plugins");
        let installed_plugin_keys = fs::read_dir(&plugin_root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| extract_mod_key(&entry.file_name().to_string_lossy()))
            .collect::<std::collections::HashSet<_>>();
        let loader_installed =
            !crate::models::loaders::return_of_modding_proxies(runtime_game_path).is_empty();

        let mut to_install = desired_key_set
            .iter()
            .filter(|key| {
                let Some(full_name) = desired_full_by_key.get(*key) else {
                    return false;
                };
                if crate::models::loaders::is_loader_package(
                    &crate::models::loaders::PackageLoader::ReturnOfModding,
                    full_name,
                ) {
                    return !loader_installed && !installed_manifest_keys.contains(*key);
                }
                !installed_manifest_keys.contains(*key) && !installed_plugin_keys.contains(*key)
            })
            .cloned()
            .collect::<Vec<_>>();
        to_install.sort();

        ensure_finalize_ready(finalize, to_install.len())?;
        let mut removed = 0;
        if finalize {
            // A disabled plugin owns manifest.json even though its live file is
            // manifest.json.old. Restore that one marker before uninstall or a
            // version replacement so the ordinary ownership cleanup can remove
            // the complete old package and never strand a stale .old file.
            for entry in &manifests_to_remove {
                let full_name = &entry.manifest.mod_full_name;
                if !crate::models::loaders::is_loader_package(
                    &crate::models::loaders::PackageLoader::ReturnOfModding,
                    full_name,
                ) {
                    set_return_of_modding_plugin_enabled(
                        runtime_game_path,
                        return_of_modding_package_name(full_name),
                        true,
                    )?;
                }
            }
            removed = cleanup_owned_mod_manifests(
                runtime_game_path,
                &manifests_to_remove,
                &manifests_to_keep,
            )?;
            let visibility_changes = reconcile_return_of_modding_plugin_visibility(
                runtime_game_path,
                managed_plugin_packages,
                all_profile_mods,
            )?;
            log::debug!(
                "[sync_profile_to_game] Reconciled ReturnOfModding visibility for the active profile ({} marker changes)",
                visibility_changes
            );
            write_return_of_modding_mods_yml(profile, &game_identifier, runtime_game_path)?;
            log::debug!(
                "[sync_profile_to_game] ReturnOfModding profile wrote mods.yml at {:?}",
                runtime_game_path.join("mods.yml")
            );
        }

        return Ok(serde_json::json!({
            "removed": removed,
            "to_install": to_install,
            "already_installed": installed_manifest_keys.len() + installed_plugin_keys.len(),
            "cached": 0,
            "pending_removals": if finalize { 0 } else { manifests_to_remove.len() }
        }));
    }

    let all_manifests = load_owned_mod_manifests(&app, &profile_id, bepinex_scope)?;
    let (stored_manifests, foreign_manifests): (Vec<_>, Vec<_>) = all_manifests
        .into_iter()
        .partition(|entry| manifest_matches_target_root(&entry.manifest, &bepinex_root));
    // A mod removed and reinstalled on every Apply has lost its manifest to one
    // of these two comparisons, and neither said so before.
    for entry in &foreign_manifests {
        log::info!(
            "[sync_profile_to_game] Ignoring the manifest for {}: it was installed under {:?}, this run targets {:?}",
            entry.manifest.mod_full_name,
            entry.manifest.target_root_hint,
            runtime_game_path
        );
    }
    let (manifests_to_remove, manifests_to_keep): (Vec<_>, Vec<_>) =
        stored_manifests.into_iter().partition(|entry| {
            let desired_full = desired_full_by_key.get(&entry.manifest.mod_key);
            match desired_full {
                Some(full) => {
                    let dropped = full != &entry.manifest.mod_full_name.to_lowercase();
                    if dropped {
                        log::info!(
                            "[sync_profile_to_game] Dropping the manifest for {}: the profile asks for {}",
                            entry.manifest.mod_full_name,
                            full
                        );
                    }
                    dropped
                }
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
    // Looked for under `bepinex_root`, which is the profile when it is isolated:
    // asking the game folder there answers no on every Apply, and the pack gets
    // reinstalled over a runtime that was already in place.
    let bepinex_installed = if profile["platform"].as_str() == Some("mac") {
        if profile_is_vanilla {
            has_complete_disabled_macos_bepinex_runtime_rooted(
                runtime_game_path,
                Some(&bepinex_root),
            ) || has_complete_macos_bepinex_runtime_rooted(runtime_game_path, Some(&bepinex_root))
        } else {
            has_complete_macos_bepinex_runtime_rooted(runtime_game_path, Some(&bepinex_root))
        }
    } else {
        windows_bepinex_runtime_is_installed(&bepinex_root)
    };
    log::debug!(
        "[sync_profile_to_game] bepinex_installed={} (BepInExPack is skipped from to_install when true)",
        bepinex_installed
    );

    let mut to_install: Vec<String> = desired_key_set
        .iter()
        .filter(|pm_key| {
            // Skip BepInExPack if BepInEx is already installed
            if key_is_bepinex_runtime_pack(pm_key) && bepinex_installed {
                return false;
            }

            let desired_full = desired_full_by_key
                .get(*pm_key)
                .cloned()
                .unwrap_or_default();
            let desired_version = desired_version_by_key.get(*pm_key);

            let has_exact_version = manifests_to_keep.iter().any(|entry| {
                entry.manifest.mod_key == **pm_key
                    && manifest_files_exist(&bepinex_root, &entry.manifest.files)
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
        let removed_by_manifest =
            cleanup_owned_mod_manifests(&bepinex_root, &manifests_to_remove, &manifests_to_keep)?;
        let stale_generated_removed =
            cleanup_stale_generated_mod_artifacts(&bepinex_root, &profile_mod_full_names)?;
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
            &bepinex_root,
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
/// Whether a profile key names the BepInEx runtime itself.
///
/// Asking whether the key merely contains "bepinex" also caught the mods built
/// around it — BepInEx_GUI and RoR2BepInExPack among them — so Apply left them
/// out and then reported that the profile was fully applied.
fn key_is_bepinex_runtime_pack(key: &str) -> bool {
    let package = key.split_once('-').map(|(_, name)| name).unwrap_or(key);
    package == "bepinexpack" || package.starts_with("bepinexpack_")
}

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
    use super::{
        ensure_finalize_ready, flatten_return_of_modding_manifest_paths,
        key_is_bepinex_runtime_pack, managed_install_root,
        migrate_nested_return_of_modding_plugins, reconcile_return_of_modding_plugin_visibility,
        return_of_modding_mods_yaml, set_return_of_modding_plugin_enabled,
        windows_bepinex_runtime_is_installed,
    };

    #[test]
    fn return_of_modding_mod_list_tracks_enabled_and_disabled_profile_entries() {
        let profile = serde_json::json!({
            "mods": [
                {
                    "fullName": "zerp-MainMenuRestoration-1.0.2",
                    "versionNumber": "1.0.2",
                    "displayName": "MainMenuRestoration",
                    "author": "zerp",
                    "description": "Restores an earlier main menu",
                    "enabled": false
                },
                {
                    "fullName": "Hell2Modding-Hell2Modding-1.0.112",
                    "versionNumber": "1.0.112",
                    "enabled": true
                }
            ]
        });

        let yaml = return_of_modding_mods_yaml(&profile, "hades-ii").unwrap();
        let entries: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap();
        let entries = entries.as_sequence().unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0]["name"].as_str(),
            Some("zerp-MainMenuRestoration")
        );
        assert_eq!(entries[0]["enabled"].as_bool(), Some(false));
        assert_eq!(entries[0]["versionNumber"]["patch"].as_u64(), Some(2));
        assert_eq!(entries[1]["enabled"].as_bool(), Some(true));
        assert_eq!(entries[1]["versionNumber"]["patch"].as_u64(), Some(112));
    }

    #[test]
    fn return_of_modding_toggle_changes_the_runtime_discovery_marker_only() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-rom-toggle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let plugin = root.join("ReturnOfModding/plugins/zerp-MainMenuRestoration");
        let config = root.join("ReturnOfModding/config/zerp-MainMenuRestoration/menu.cfg");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(plugin.join("manifest.json"), b"{}").unwrap();
        std::fs::write(plugin.join("main.lua"), b"return {}").unwrap();
        std::fs::write(&config, b"menu = random").unwrap();

        assert!(
            set_return_of_modding_plugin_enabled(&root, "zerp-MainMenuRestoration", false,)
                .unwrap()
        );
        assert!(!plugin.join("manifest.json").exists());
        assert!(plugin.join("manifest.json.old").is_file());
        assert!(plugin.join("main.lua").is_file());
        assert!(config.is_file());

        assert!(
            set_return_of_modding_plugin_enabled(&root, "zerp-MainMenuRestoration", true,).unwrap()
        );
        assert!(plugin.join("manifest.json").is_file());
        assert!(!plugin.join("manifest.json.old").exists());
        assert!(config.is_file());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn return_of_modding_profile_switches_and_individual_toggles_share_one_game_tree() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-rom-profiles-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let plugins = root.join("ReturnOfModding/plugins");
        let first = plugins.join("Author-FirstMod");
        let second = plugins.join("Author-SecondMod");
        let manually_installed = plugins.join("Manual-UnmanagedMod");
        let preserved_config = root.join("ReturnOfModding/config/Author-FirstMod/settings.cfg");
        let preserved_data =
            root.join("ReturnOfModding/plugins_data/Author-SecondMod/save-data.json");
        for plugin in [&first, &second, &manually_installed] {
            std::fs::create_dir_all(plugin).unwrap();
            std::fs::write(plugin.join("manifest.json"), b"{}").unwrap();
            std::fs::write(plugin.join("main.lua"), b"return {}").unwrap();
        }
        std::fs::create_dir_all(preserved_config.parent().unwrap()).unwrap();
        std::fs::create_dir_all(preserved_data.parent().unwrap()).unwrap();
        std::fs::write(&preserved_config, b"setting = true").unwrap();
        std::fs::write(&preserved_data, b"persistent").unwrap();

        let managed = || {
            vec![
                "Author-FirstMod".to_string(),
                "Author-SecondMod".to_string(),
            ]
        };
        let first_profile = vec![serde_json::json!({
            "fullName": "Author-FirstMod-1.0.0",
            "enabled": true
        })];
        reconcile_return_of_modding_plugin_visibility(&root, managed(), &first_profile).unwrap();
        assert!(first.join("manifest.json").is_file());
        assert!(second.join("manifest.json.old").is_file());

        let second_profile = vec![serde_json::json!({
            "fullName": "Author-SecondMod-1.0.0",
            "enabled": true
        })];
        reconcile_return_of_modding_plugin_visibility(&root, managed(), &second_profile).unwrap();
        assert!(first.join("manifest.json.old").is_file());
        assert!(second.join("manifest.json").is_file());

        reconcile_return_of_modding_plugin_visibility(&root, managed(), &[]).unwrap();
        assert!(first.join("manifest.json.old").is_file());
        assert!(second.join("manifest.json.old").is_file());
        assert!(manually_installed.join("manifest.json").is_file());

        let individually_disabled = vec![serde_json::json!({
            "fullName": "Author-FirstMod-1.0.0",
            "enabled": false
        })];
        reconcile_return_of_modding_plugin_visibility(&root, managed(), &individually_disabled)
            .unwrap();
        assert!(first.join("manifest.json.old").is_file());
        assert!(second.join("manifest.json.old").is_file());
        assert!(manually_installed.join("manifest.json").is_file());
        assert_eq!(std::fs::read(&preserved_config).unwrap(), b"setting = true");
        assert_eq!(std::fs::read(&preserved_data).unwrap(), b"persistent");

        reconcile_return_of_modding_plugin_visibility(&root, managed(), &first_profile).unwrap();
        assert!(first.join("manifest.json").is_file());
        assert!(second.join("manifest.json.old").is_file());
        assert!(manually_installed.join("manifest.json").is_file());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_hades_plugin_wrappers_are_flattened_without_unpacking_child_folders() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-rom-wrapper-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package = root.join("ReturnOfModding/plugins/NikkelM-Cosmetics_API");
        std::fs::create_dir_all(package.join("plugins/Scripts")).unwrap();
        std::fs::write(package.join("manifest.json"), b"manifest").unwrap();
        std::fs::write(package.join("plugins/main.lua"), b"plugin").unwrap();
        std::fs::write(package.join("plugins/Scripts/helper.lua"), b"helper").unwrap();

        assert!(migrate_nested_return_of_modding_plugins(&root, "NikkelM-Cosmetics_API").unwrap());
        assert_eq!(std::fs::read(package.join("main.lua")).unwrap(), b"plugin");
        assert_eq!(
            std::fs::read(package.join("Scripts/helper.lua")).unwrap(),
            b"helper"
        );
        assert_eq!(
            std::fs::read(package.join("manifest.json")).unwrap(),
            b"manifest"
        );
        assert!(!package.join("plugins").exists());

        let flattened = flatten_return_of_modding_manifest_paths(
            &[
                "ReturnOfModding/plugins/NikkelM-Cosmetics_API/manifest.json".to_string(),
                "ReturnOfModding/plugins/NikkelM-Cosmetics_API/plugins/main.lua".to_string(),
                "ReturnOfModding/plugins/NikkelM-Cosmetics_API/plugins/Scripts/helper.lua"
                    .to_string(),
            ],
            "NikkelM-Cosmetics_API",
        );
        assert_eq!(
            flattened,
            vec![
                std::path::PathBuf::from(
                    "ReturnOfModding/plugins/NikkelM-Cosmetics_API/manifest.json"
                ),
                std::path::PathBuf::from("ReturnOfModding/plugins/NikkelM-Cosmetics_API/main.lua"),
                std::path::PathBuf::from(
                    "ReturnOfModding/plugins/NikkelM-Cosmetics_API/Scripts/helper.lua"
                ),
            ]
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_the_runtime_pack_is_skipped_once_bepinex_is_installed() {
        assert!(key_is_bepinex_runtime_pack("bepinex-bepinexpack"));
        assert!(key_is_bepinex_runtime_pack("bbepis-bepinexpack"));
        assert!(key_is_bepinex_runtime_pack("bepinex-bepinexpack_muck"));
        assert!(key_is_bepinex_runtime_pack("bepinex-bepinexpack_gtfo"));

        // Mods that merely carry the name: they were silently never installed,
        // while Apply still reported the profile as fully applied.
        assert!(!key_is_bepinex_runtime_pack("riskofthunder-bepinex_gui"));
        assert!(!key_is_bepinex_runtime_pack(
            "riskofthunder-ror2bepinexpack"
        ));
        assert!(!key_is_bepinex_runtime_pack("someone-bepinexconfigmanager"));
    }

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
    fn an_isolated_runtime_counts_as_installed_although_the_game_folder_is_bare() {
        // The regression this guards: asking the game folder about a profile
        // that keeps its tree elsewhere answers no, so every Apply queued
        // BepInExPack again over a runtime that was already there.
        let root = std::env::temp_dir().join(format!(
            "r2modmac-sync-isolated-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let game = root.join("game");
        let profile = root.join("profile");
        let core = profile.join("BepInEx/core");
        std::fs::create_dir_all(&core).unwrap();
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(core.join("BepInEx.Preloader.dll"), b"").unwrap();

        assert!(windows_bepinex_runtime_is_installed(&profile));
        assert!(!windows_bepinex_runtime_is_installed(&game));
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

    #[test]
    fn return_of_modding_reconciles_the_game_even_with_an_isolated_profile() {
        let game = std::path::Path::new("/games/Hades II");
        let isolated_profile = std::path::PathBuf::from("/profiles/test");

        assert_eq!(
            managed_install_root(true, game, isolated_profile.clone()),
            game.to_path_buf()
        );
        assert_eq!(
            managed_install_root(false, game, isolated_profile.clone()),
            isolated_profile
        );
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

#[cfg(test)]
mod isolation_scan_target_tests {

    fn world(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-scan-{}-{}-{}",
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

    /// Sync reconciles against whichever tree the game will load. Pointed at
    /// the game it must not see the profile's mods, and the other way round,
    /// or every Apply would remove and reinstall everything.
    #[test]
    fn each_root_sees_only_its_own_mods() {
        let root = world("roots");
        let game = root.join("game");
        let profile = root.join("profiles/abc");
        std::fs::create_dir_all(game.join("BepInEx/plugins/Author-InGame-1.0.0")).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx/plugins/Author-InProfile-1.0.0")).unwrap();

        let names = |base: &std::path::Path| {
            let mut found = std::fs::read_dir(base.join("BepInEx/plugins"))
                .unwrap()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>();
            found.sort();
            found
        };

        assert_eq!(names(&game), vec!["Author-InGame-1.0.0".to_string()]);
        assert_eq!(names(&profile), vec!["Author-InProfile-1.0.0".to_string()]);

        std::fs::remove_dir_all(root).unwrap();
    }

    /// A vanilla profile renames its own tree, so the disabled folder has to be
    /// looked for under the same root the mods were installed into.
    #[test]
    fn the_disabled_tree_is_looked_for_in_the_same_root() {
        let root = world("disabled");
        let profile = root.join("profiles/abc");
        std::fs::create_dir_all(profile.join("BepInEx_DISABLED/plugins")).unwrap();

        let plugins = if profile.join("BepInEx_DISABLED").is_dir() {
            profile.join("BepInEx_DISABLED").join("plugins")
        } else {
            profile.join("BepInEx").join("plugins")
        };
        assert!(plugins.ends_with("BepInEx_DISABLED/plugins"));

        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod isolation_migration_tests {
    use crate::commands::mod_commands::relocate_bepinex_tree;

    fn world(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-migrate-{}-{}-{}",
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

    fn write(path: &std::path::Path, content: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// Turning isolation on for a profile that was installed the old way must
    /// carry its mods and configs across, not leave the user with nothing.
    #[test]
    fn an_existing_install_moves_into_the_profile_with_its_configs() {
        let root = world("existing");
        let game = root.join("game");
        let profile = root.join("profiles/abc");
        write(
            &game.join("BepInEx/plugins/Author-Mod-1.0.0/Mod.dll"),
            b"mod",
        );
        write(
            &game.join("BepInEx/config/xyz.alcan.comfortcalc.cfg"),
            b"user settings",
        );
        write(
            &game.join("BepInEx/core/BepInEx.Preloader.dll"),
            b"preloader",
        );
        write(&game.join("run_bepinex.sh"), b"#!/bin/sh\n");

        relocate_bepinex_tree(&game, &profile).unwrap();

        assert!(profile
            .join("BepInEx/plugins/Author-Mod-1.0.0/Mod.dll")
            .is_file());
        assert_eq!(
            std::fs::read(profile.join("BepInEx/config/xyz.alcan.comfortcalc.cfg")).unwrap(),
            b"user settings"
        );
        assert!(profile.join("BepInEx/core/BepInEx.Preloader.dll").is_file());
        assert!(!game.join("BepInEx").exists());
        assert!(game.join("run_bepinex.sh").is_file());

        std::fs::remove_dir_all(root).unwrap();
    }

    /// Only the first profile to sync claims the tree the game already had.
    #[test]
    fn a_second_profile_does_not_steal_the_first_ones_tree() {
        let root = world("second");
        let game = root.join("game");
        let first = root.join("profiles/first");
        let second = root.join("profiles/second");
        write(
            &game.join("BepInEx/plugins/Author-Mod-1.0.0/Mod.dll"),
            b"mod",
        );

        relocate_bepinex_tree(&game, &first).unwrap();
        // The game has nothing left, so the second profile starts empty.
        relocate_bepinex_tree(&game, &second).unwrap();

        assert!(first
            .join("BepInEx/plugins/Author-Mod-1.0.0/Mod.dll")
            .is_file());
        assert!(!second.join("BepInEx").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    /// A profile that already has its own tree keeps it: the migration is a
    /// one-off, and running it twice must not merge the game back in.
    #[test]
    fn a_profile_that_already_moved_is_not_touched_again() {
        let root = world("idempotent");
        let game = root.join("game");
        let profile = root.join("profiles/abc");
        write(
            &profile.join("BepInEx/plugins/Author-Mine-1.0.0/Mine.dll"),
            b"mine",
        );
        write(
            &game.join("BepInEx/plugins/Author-Other-1.0.0/Other.dll"),
            b"other",
        );

        // This is the guard the sync applies: the profile already has a tree.
        let should_migrate = !profile.join("BepInEx").is_dir()
            && !profile.join("BepInEx_DISABLED").is_dir()
            && game.join("BepInEx").is_dir();
        assert!(!should_migrate);

        assert!(profile
            .join("BepInEx/plugins/Author-Mine-1.0.0/Mine.dll")
            .is_file());
        assert!(!profile.join("BepInEx/plugins/Author-Other-1.0.0").exists());

        std::fs::remove_dir_all(root).unwrap();
    }
}
