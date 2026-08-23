use super::*;
use crate::tracing::{perfetto_te_ns, scoped_track_event, EventContext, TrackEventDebugArg};
use tauri::command;

#[command]
pub async fn install_to_game(
    app: AppHandle,
    game_identifier: String,
    profile_id: String,
    disabled_mods: Vec<String>,
    is_vanilla_override: Option<bool>,
) -> Result<(), String> {
    scoped_track_event!("install", "install_to_game", |ctx: &mut EventContext| {
        ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id.as_str()));
        ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier.as_str()));
    });
    let profile_platform = get_profile_platform(&app, &profile_id);
    let is_mac_profile = profile_platform == "mac";
    let settings = load_settings_impl(&app);
    let use_legacy_plugin_cache = settings.legacy_install_mode;

    // 1. Find game path
    let game_path_str = get_game_path(
        app.clone(),
        game_identifier.clone(),
        Some(profile_platform.clone()),
    )
    .await?
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
    let profile_dir = crate::utils::paths::app_data_dir(&app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(&profile_id);

    // In non-legacy mode, profile dir might not exist - that's OK for vanilla mode
    // if !profile_dir.exists() {
    //     return Err("Profile not found".to_string());
    // }

    // Check is_vanilla - prefer override from frontend (for timing issues)
    let is_vanilla = if let Some(override_val) = is_vanilla_override {
        log::debug!(
            "[install_to_game] Using is_vanilla override: {}",
            override_val
        );
        override_val
    } else {
        // Fallback: Read from profiles.json
        let profiles_path = crate::utils::paths::app_data_dir(&app)
            .unwrap()
            .join("profiles.json");
        let mut vanilla = false;
        if profiles_path.exists() {
            if let Ok(data) = fs::read_to_string(&profiles_path) {
                if let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                    if let Some(p) = profiles
                        .iter()
                        .find(|p| p["id"].as_str() == Some(&profile_id))
                    {
                        vanilla = p["is_vanilla"].as_bool().unwrap_or(false);
                    }
                }
            }
        }
        vanilla
    };

    log::debug!(
        "[install_to_game] platform={} stored_distribution={} effective_distribution={} launch_mode={} manage_steam_launch={}",
        profile_platform,
        get_profile_distribution(&app, &profile_id),
        effective_distribution,
        launch_mode,
        manage_steam_launch
    );

    if is_vanilla {
        if is_mac_profile {
            log::info!(
                "[install_to_game] Profile is in VANILLA mode on macOS. Runtime will be disabled now, and any managed Steam launch option will be cleared when the user presses Play."
            );
        } else {
            log::info!("[install_to_game] Profile is in VANILLA mode. Cleaning game folder.");
        }
    }

    log::debug!("[install_to_game] Disabled mods: {:?}", disabled_mods);

    let is_balatro_profile = is_mac_profile
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path));
    let is_outerwilds_profile =
        is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(game_path);
    let is_return_of_modding_profile =
        crate::models::loaders::uses_return_of_modding(&game_identifier, game_path);
    let runtime_game_path_buf = if is_mac_profile
        && !is_balatro_profile
        && !is_outerwilds_profile
        && !is_return_of_modding_profile
    {
        let resolved = resolve_macos_runtime_root(game_path);
        if resolved != game_path {
            log::info!(
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
        && !is_outerwilds_profile
        && !is_return_of_modding_profile
        && has_complete_macos_bepinex_runtime(runtime_game_path);

    if is_mac_profile
        && !is_vanilla
        && !is_balatro_profile
        && !is_outerwilds_profile
        && !is_return_of_modding_profile
    {
        validate_macos_bepinex_support(runtime_game_path)?;
    }

    // Shimloader keeps every mod in the profile; the only thing that reaches
    // the game is the runtime the shim is loaded from.
    if crate::models::loaders::uses_shimloader(&game_identifier) {
        link_shimloader_runtime(&profile_dir, game_path, &game_identifier, is_vanilla)?;
        log::debug!("[install_to_game] Shimloader profile - skipping BepInEx operations");
        return Ok(());
    }

    if is_outerwilds_profile {
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");

        if is_vanilla {
            restore_outerwilds_vanilla(&game_path).ok();
            restore_mscorlib_vanilla(&game_path, false).ok();
            if owml_folder.exists() {
                if owml_disabled.exists() {
                    let _ = fs::remove_dir_all(&owml_disabled);
                }
                let _ = fs::rename(&owml_folder, &owml_disabled);
                log::info!("[install_to_game] Renamed OWML -> OWML_DISABLED");
            }
        } else {
            if owml_disabled.exists() && !owml_folder.exists() {
                let _ = fs::rename(&owml_disabled, &owml_folder);
                log::info!("[install_to_game] Restored OWML_DISABLED -> OWML");
            }
            restore_outerwilds_modded(&game_path).ok();
        }
        // Outer Wilds uses OWML; no BepInEx operations needed here.
        // install_mod_bytes handles all OWML-specific installation.
        log::debug!(
            "[install_to_game] Outer Wilds profile - skipping BepInEx install, OWML manages mods."
        );
        return Ok(());
    }

    if is_return_of_modding_profile {
        // Vanilla mode parks the loader beside the game instead of deleting it.
        // The file to rename is whichever proxy the pack installed - version.dll
        // for ReturnOfModding, d3d12.dll for Hell2Modding on Hades II - so both
        // directions work off the names a pack can ship rather than one of them.
        for name in crate::models::loaders::RETURN_OF_MODDING_PROXY_NAMES {
            let loader = game_path.join(name);
            let disabled_loader = game_path.join(format!("{name}_DISABLED"));
            if is_vanilla {
                if loader.is_file() {
                    if disabled_loader.exists() {
                        fs::remove_file(&disabled_loader).map_err(|error| error.to_string())?;
                    }
                    fs::rename(&loader, &disabled_loader)
                        .map_err(|error| format!("Failed to disable ReturnOfModding: {error}"))?;
                }
            } else if disabled_loader.is_file() && !loader.exists() {
                fs::rename(&disabled_loader, &loader)
                    .map_err(|error| format!("Failed to enable ReturnOfModding: {error}"))?;
            }
        }
        log::debug!("[install_to_game] ReturnOfModding profile - skipping BepInEx operations");
        return Ok(());
    }

    if is_balatro_profile {
        let mods_dir = get_balatro_mods_dir().ok_or_else(|| {
            "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string()
        })?;
        let disabled_dir = balatro_mods_disabled_dir()?;

        if is_vanilla {
            if mods_dir.exists() {
                if disabled_dir.exists() {
                    let _ = fs::remove_dir_all(&disabled_dir);
                }
                fs::rename(&mods_dir, &disabled_dir)
                    .map_err(|e| format!("Failed to disable Balatro mods: {}", e))?;
            }
            log::info!("[install_to_game] Balatro vanilla mode complete.");
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

        log::info!("[install_to_game] Balatro sync complete!");
        return Ok(());
    }

    // --- FIX BEPINEX STRUCTURE START ---
    if !is_vanilla && use_legacy_plugin_cache {
        // Always check for BepInExPack in plugins and ensure it's properly installed at root
        let plugins_dir = profile_dir.join("BepInEx").join("plugins");
        if plugins_dir.exists() {
            let bepinex_sentinel_file = if is_mac_profile {
                "doorstop_libs"
            } else {
                "winhttp.dll"
            };
            let sentinel_is_dir = is_mac_profile; // doorstop_libs is a directory, winhttp.dll is a file

            let mut best_candidate: Option<(std::path::PathBuf, i32)> = None;

            if let Ok(entries) = fs::read_dir(&plugins_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }

                    let folder_name = path
                        .file_name()
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
                        log::debug!(
                            "[install_to_game] Found nested BepInExPack candidate: {:?}",
                            nested_pack
                        );
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
                        if folder_name.contains("bepinex") {
                            score += 1;
                        }
                        log::debug!("[install_to_game] Found direct BepInExPack candidate: {:?} (score: {})", path, score);
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
                                let sub_name = sub_path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                let mut score = 1;
                                if sub_name.contains("bepinex") {
                                    score += 1;
                                }
                                if best_candidate.as_ref().map_or(true, |(_, s)| score > *s) {
                                    best_candidate = Some((sub_path, score));
                                }
                            }
                        }
                    }
                }
            }

            if let Some((pack_dir, score)) = best_candidate {
                log::debug!(
                    "[install_to_game] Selected BepInExPack: {:?} (score: {})",
                    pack_dir,
                    score
                );

                if is_mac_profile {
                    // === macOS: copy doorstop_libs/ + run_bepinex.sh + doorstop_config.ini ===
                    // The actual BepInEx Unix pack structure:
                    // - doorstop_libs/ (folder with all dylib/so files)
                    // - run_bepinex.sh
                    // - doorstop_config.ini
                    let doorstop_libs_src = pack_dir.join("doorstop_libs");
                    let doorstop_libs_dst = profile_dir.join("doorstop_libs");
                    if doorstop_libs_src.exists() && !doorstop_libs_dst.exists() {
                        log::debug!("[install_to_game] Copying doorstop_libs/ to profile root");
                        copy_dir_recursive(&doorstop_libs_src, &doorstop_libs_dst)
                            .map_err(|e| format!("Failed to copy doorstop_libs: {}", e))?;
                    }

                    if let Some(run_script_src) = find_bepinex_script_in_dir(&pack_dir) {
                        let run_script_dst = profile_dir.join("run_bepinex.sh");
                        if !run_script_dst.exists() {
                            log::debug!("[install_to_game] Copying run_bepinex.sh to profile root");
                            fs::copy(&run_script_src, &run_script_dst)
                                .map_err(|e| format!("Failed to copy run_bepinex.sh: {}", e))?;
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let mut perms = fs::metadata(&run_script_dst)
                                    .map_err(|e| e.to_string())?
                                    .permissions();
                                perms.set_mode(0o755);
                                let _ = fs::set_permissions(&run_script_dst, perms);
                            }
                        }
                    } else {
                        log::warn!("[install_to_game] Cached BepInEx pack has no macOS script; relying on profile/game root runtime fallback");
                    }

                    // Also copy doorstop_config.ini for macOS
                    let doorstop_cfg_src = pack_dir.join("doorstop_config.ini");
                    let doorstop_cfg_dst = profile_dir.join("doorstop_config.ini");
                    if doorstop_cfg_src.exists() && !doorstop_cfg_dst.exists() {
                        log::debug!(
                            "[install_to_game] Copying doorstop_config.ini (macOS) to profile root"
                        );
                        fs::copy(&doorstop_cfg_src, &doorstop_cfg_dst)
                            .map_err(|e| format!("Failed to copy doorstop_config.ini: {}", e))?;
                        normalize_macos_doorstop_config_file(&doorstop_cfg_dst)?;
                    }
                } else {
                    // === Windows: copy winhttp.dll + doorstop_config.ini ===
                    let winhttp_src = pack_dir.join("winhttp.dll");
                    let winhttp_dst = profile_dir.join("winhttp.dll");
                    if winhttp_src.exists() && !winhttp_dst.exists() {
                        log::debug!("[install_to_game] Copying winhttp.dll to profile root");
                        fs::copy(&winhttp_src, &winhttp_dst)
                            .map_err(|e| format!("Failed to copy winhttp.dll: {}", e))?;
                    }

                    let doorstop_src = pack_dir.join("doorstop_config.ini");
                    let doorstop_dst = profile_dir.join("doorstop_config.ini");
                    if doorstop_src.exists() && !doorstop_dst.exists() {
                        log::debug!(
                            "[install_to_game] Copying doorstop_config.ini to profile root"
                        );
                        fs::copy(&doorstop_src, &doorstop_dst)
                            .map_err(|e| format!("Failed to copy doorstop_config.ini: {}", e))?;
                        // FORCE ENABLE DOORSTOP
                        if let Ok(content) = fs::read_to_string(&doorstop_dst) {
                            if !content.contains("enabled=true")
                                && !content.contains("enabled = true")
                            {
                                log::debug!("[install_to_game] Enforcing enabled=true in doorstop_config.ini");
                                let new_content = content
                                    .replace("enabled=false", "enabled=true")
                                    .replace("enabled = false", "enabled = true");
                                let _ = fs::write(&doorstop_dst, new_content);
                            }
                        }
                    }
                }

                // Merge BepInEx core/config from the pack (shared for both platforms)
                let pack_bepinex = pack_dir.join("BepInEx");
                if pack_bepinex.exists() {
                    log::debug!("[install_to_game] Merging BepInEx core/config from pack...");
                    let target_bepinex = profile_dir.join("BepInEx");
                    copy_dir_recursive(&pack_bepinex, &target_bepinex)
                        .map_err(|e| format!("Failed to merge BepInEx folder: {}", e))?;
                }
            } else {
                log::warn!(
                    "[install_to_game] Warning: No BepInExPack found in plugins for {}!",
                    if is_mac_profile {
                        "macOS (looking for libdoorstop.dylib)"
                    } else {
                        "Windows (looking for winhttp.dll)"
                    }
                );
            }
        }
    } // End if !is_vanilla
      // --- FIX BEPINEX STRUCTURE END ---

    log::info!(
        "[install_to_game] Installing profile {} to game {} (runtime root {})",
        profile_id,
        game_path.display(),
        runtime_game_path.display()
    );

    // --- SYNC: Remove mods from game that are not in profile OR are disabled ---
    let profile_plugins = profile_dir.join("BepInEx").join("plugins");
    let game_plugins = runtime_game_path.join("BepInEx").join("plugins");

    // Create set of enabled mod names (lowercase for comparison)
    let disabled_set: std::collections::HashSet<String> =
        disabled_mods.iter().map(|s| s.to_lowercase()).collect();

    if is_mac_profile && is_vanilla {
        if find_bepinex_script_in_dir(runtime_game_path).is_some() {
            let run_script = canonicalize_macos_bepinex_script(runtime_game_path)?;
            configure_macos_bepinex_script(
                &run_script,
                runtime_game_path,
                load_settings_impl(&app).write_debug_logs_to_game,
                None,
            )?;
            dequarantine_recursive(runtime_game_path);
        }
        // Keep the runtime disabled on Apply, but defer any Steam launch-option
        // changes until Play. Updating localconfig.vdf can force a Steam restart,
        // which would be a surprising side effect during a profile sync.
        let tree_root = bepinex_install_root(&app, &profile_id, runtime_game_path)?;
        sync_macos_runtime_disabled_state_rooted(runtime_game_path, true, Some(&tree_root))?;
        log::info!(
            "[install_to_game] macOS vanilla mode complete - runtime disabled now, Steam launch state will be reconciled on Play."
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
                log::debug!("[install_to_game] Vanilla mode: Removing old disabled folder");
                let _ = fs::remove_dir_all(&bepinex_disabled);
            }
            log::debug!("[install_to_game] Vanilla mode: Renaming BepInEx -> BepInEx_DISABLED");
            fs::rename(&bepinex_folder, &bepinex_disabled)
                .map_err(|e| format!("Failed to disable BepInEx: {}", e))?;
        }
    } else if !is_mac_profile {
        // Normal mode: Check if BepInEx_DISABLED exists and restore it
        let bepinex_folder = game_path.join("BepInEx");
        let bepinex_disabled = game_path.join("BepInEx_DISABLED");

        if bepinex_disabled.exists() && !bepinex_folder.exists() {
            log::debug!("[install_to_game] Restoring BepInEx_DISABLED -> BepInEx");
            fs::rename(&bepinex_disabled, &bepinex_folder)
                .map_err(|e| format!("Failed to restore BepInEx: {}", e))?;
        }
    }

    if use_legacy_plugin_cache && !is_vanilla && profile_plugins.exists() && game_plugins.exists() {
        // Get list of ENABLED mod folders in profile
        let enabled_profile_mods: std::collections::HashSet<String> =
            fs::read_dir(&profile_plugins)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                        .filter_map(|e| {
                            let name = e.file_name().to_str().map(|s| s.to_string())?;
                            // Check if this mod is disabled
                            let is_disabled =
                                disabled_set.iter().any(|d| name.to_lowercase().contains(d));
                            if is_disabled {
                                log::debug!(
                                    "[install_to_game] Skipping disabled mod in profile: {}",
                                    name
                                );
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
                    let is_disabled = disabled_set
                        .iter()
                        .any(|d| folder_name.to_lowercase().contains(d));
                    let not_in_profile = !enabled_profile_mods.contains(&folder_name);

                    if is_disabled || not_in_profile {
                        log::debug!(
                            "[install_to_game] Removing mod from game (disabled={}, orphan={}): {}",
                            is_disabled,
                            not_in_profile,
                            folder_name
                        );
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
                                let dest_plugin_name =
                                    dest_entry.file_name().to_string_lossy().to_string();
                                let source_plugin_path = src_path.join(&dest_plugin_name);
                                let is_disabled = disabled_set
                                    .iter()
                                    .any(|d| dest_plugin_name.to_lowercase().contains(d));

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
                                    log::debug!(
                                        "[install_to_game] Removed old/disabled plugin: {}",
                                        dest_plugin_name
                                    );
                                }
                            }
                        }

                        // Now create symlinks for enabled plugins
                        if let Ok(plugin_entries) = fs::read_dir(&src_path) {
                            for plugin_entry in plugin_entries.filter_map(|e| e.ok()) {
                                let plugin_name =
                                    plugin_entry.file_name().to_string_lossy().to_string();

                                // Check if this plugin is disabled
                                let is_disabled = disabled_set
                                    .iter()
                                    .any(|d| plugin_name.to_lowercase().contains(d));

                                if is_disabled {
                                    log::debug!(
                                        "[install_to_game] Skipping disabled plugin: {}",
                                        plugin_name
                                    );
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
                                    std::os::unix::fs::symlink(&plugin_src, &plugin_dst).map_err(
                                        |e| {
                                            format!(
                                                "Failed to create symlink for {}: {}",
                                                plugin_name, e
                                            )
                                        },
                                    )?;
                                    log::debug!(
                                        "[install_to_game] Created symlink: {} -> {:?}",
                                        plugin_name,
                                        plugin_src
                                    );
                                }

                                #[cfg(windows)]
                                {
                                    // Fallback to copy on Windows (symlinks require admin)
                                    if plugin_src.is_dir() {
                                        copy_dir_recursive(&plugin_src, &plugin_dst).map_err(
                                            |e| {
                                                format!(
                                                    "Failed to copy plugin {}: {}",
                                                    plugin_name, e
                                                )
                                            },
                                        )?;
                                    } else {
                                        fs::copy(&plugin_src, &plugin_dst).map_err(|e| {
                                            format!(
                                                "Failed to copy plugin file {}: {}",
                                                plugin_name, e
                                            )
                                        })?;
                                    }
                                }
                            }
                        }
                    } else if is_mac_profile
                        && has_complete_macos_bepinex_runtime(runtime_game_path)
                    {
                        log::debug!(
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
            log::debug!("[install_to_game] Synced BepInEx to game folder");
        }
    } // End if !is_vanilla

    // 4. Sync platform-specific root payloads
    if is_mac_profile {
        if !is_vanilla && !is_balatro_profile {
            let tree_root = bepinex_install_root(&app, &profile_id, runtime_game_path)?;
            sync_macos_runtime_disabled_state_rooted(runtime_game_path, false, Some(&tree_root))?;
            ensure_macos_bepinex_runtime_present(&app, &profile_id, runtime_game_path).await?;
        }
        let profile_script_is_macos = find_bepinex_script_in_dir(&profile_dir)
            .and_then(|script_path| fs::read_to_string(script_path).ok())
            .map(|script| has_macos_doorstop_support(&script))
            .unwrap_or(false);
        let profile_has_safe_macos_runtime_payload =
            has_complete_macos_bepinex_runtime(&profile_dir) && profile_script_is_macos;
        let keep_existing_macos_runtime_payload = !is_vanilla
            && !is_balatro_profile
            && (mac_runtime_present_before_sync
                || has_complete_macos_bepinex_runtime(runtime_game_path));

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

            if keep_existing_macos_runtime_payload {
                log::debug!(
                    "[install_to_game] Keeping existing macOS root payload: {}",
                    item_name
                );
                continue;
            }
            if !profile_has_safe_macos_runtime_payload {
                log::debug!(
                    "[install_to_game] Skipping profile macOS root payload copy for {} because the cached profile runtime is incomplete or not macOS-compatible.",
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
                fs::copy(&source, &dest)
                    .map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
            }

            log::debug!("[install_to_game] Synced {} to game folder", item_name);
        }

        if let Some(source_script) = find_bepinex_script_in_dir(&profile_dir) {
            let dest_script = runtime_game_path.join(CANONICAL_MAC_BEPINEX_SCRIPT);
            if keep_existing_macos_runtime_payload {
                log::debug!(
                    "[install_to_game] Keeping existing macOS launcher script: {}",
                    CANONICAL_MAC_BEPINEX_SCRIPT
                );
            } else if !profile_has_safe_macos_runtime_payload {
                log::debug!(
                    "[install_to_game] Skipping profile launcher script copy because the cached profile runtime is incomplete or not macOS-compatible."
                );
            } else {
                if dest_script.exists() {
                    let _ = fs::remove_file(&dest_script);
                }
                fs::copy(&source_script, &dest_script).map_err(|e| {
                    format!("Failed to copy {}: {}", CANONICAL_MAC_BEPINEX_SCRIPT, e)
                })?;
                log::debug!(
                    "[install_to_game] Synced {} to game folder",
                    CANONICAL_MAC_BEPINEX_SCRIPT
                );
            }
        }

        migrate_root_plugins_into_bepinex(runtime_game_path)?;
        let tree_root = bepinex_install_root(&app, &profile_id, runtime_game_path)?;
        normalize_macos_doorstop_config_file(&runtime_game_path.join("doorstop_config.ini"))?;
        configure_macos_doorstop_target_assembly(
            &runtime_game_path.join("doorstop_config.ini"),
            &tree_root,
        )?;
        let run_script = canonicalize_macos_bepinex_script(runtime_game_path)?;
        // Apply leaves the script pointing at the tree it just wrote, so that
        // starting the game from Steam rather than from here still loads the
        // profile's mods instead of quietly running unmodded.
        let script_bepinex_dir =
            (tree_root != *runtime_game_path).then(|| tree_root.join("BepInEx"));
        configure_macos_bepinex_script(
            &run_script,
            runtime_game_path,
            load_settings_impl(&app).write_debug_logs_to_game,
            script_bepinex_dir.as_deref(),
        )?;
        sync_macos_runtime_disabled_state_rooted(runtime_game_path, false, Some(&tree_root))?;
        dequarantine_recursive(runtime_game_path);
        if manage_steam_launch {
            log::info!(
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
                    log::debug!(
                        "[install_to_game] Vanilla mode: Renamed {} -> {}",
                        item_name,
                        disabled_name
                    );
                }
            } else {
                if disabled_dest.exists() && !dest.exists() {
                    let _ = fs::rename(&disabled_dest, &dest);
                    log::debug!("[install_to_game] Restored {} from disabled", item_name);
                }

                let source = profile_dir.join(item_name);
                if source.exists() && !dest.exists() {
                    fs::copy(&source, &dest)
                        .map_err(|e| format!("Failed to copy {}: {}", item_name, e))?;
                    log::debug!("[install_to_game] Synced {} to game folder", item_name);
                }
            }
        }

        if !is_vanilla {
            ensure_windows_bepinex_console_enabled(game_path)?;
            // Apply is also where a config left over from an earlier layout gets
            // corrected, so a game whose tree has moved into the profile is not
            // left loading from a folder beside the executable that is empty.
            let tree_root = bepinex_install_root(&app, &profile_id, game_path)?;
            if tree_root != *game_path {
                crate::commands::mod_commands::point_game_doorstop_ini_at_tree(
                    game_path, &tree_root,
                )?;
            }
        }
    }

    log::info!("[install_to_game] Sync complete!");
    Ok(())
}

/// Put the shimloader runtime where the game will load it from.
///
/// A shimloader install lives entirely in the profile, but the game does not
/// know that: it loads `dwmapi.dll` as a proxy from
/// `<game>/<data folder>/Binaries/Win64`, and that shim then loads `ue4ss.dll`
/// beside it. r2modman bridges the same gap by copying the profile's root files
/// into that folder (`ModLinker.getRootFilesDestination`), and nothing else
/// from the profile follows them. A vanilla profile is the same operation in
/// reverse: take the runtime out and the game starts unmodded.
fn link_shimloader_runtime(
    profile_dir: &std::path::Path,
    game_path: &std::path::Path,
    game_identifier: &str,
    vanilla: bool,
) -> Result<(), String> {
    let data_folder = crate::models::loaders::shimloader_game(game_identifier)
        .map(|game| game.data_folder)
        .unwrap_or_default();
    let binaries_dir = crate::models::loaders::shimloader_binaries_dir(game_path, &data_folder)
        .ok_or_else(|| {
            format!(
                "Could not find {}/Binaries/Win64 in {}. Check the game folder in Settings.",
                if data_folder.is_empty() {
                    "the game's data folder"
                } else {
                    &data_folder
                },
                game_path.display()
            )
        })?;

    if vanilla {
        for name in crate::models::loaders::SHIMLOADER_RUNTIME_FILES {
            let installed = binaries_dir.join(name);
            if installed.is_file() {
                fs::remove_file(&installed).map_err(|error| {
                    format!("Failed to remove {}: {}", installed.display(), error)
                })?;
            }
        }
        log::info!(
            "[sync_profile_to_game] Vanilla profile: removed the shimloader runtime from {:?}",
            binaries_dir
        );
        return Ok(());
    }

    if !profile_dir.join("dwmapi.dll").is_file() {
        return Err(
            "Shimloader is not installed in this profile. Install the loader and try again."
                .to_string(),
        );
    }

    fs::create_dir_all(&binaries_dir).map_err(|error| error.to_string())?;
    for name in crate::models::loaders::SHIMLOADER_RUNTIME_FILES {
        let source = profile_dir.join(name);
        if !source.is_file() {
            log::warn!("[sync_profile_to_game] Profile has no {name}; leaving the game's copy");
            continue;
        }
        fs::copy(&source, binaries_dir.join(name))
            .map_err(|error| format!("Failed to install {} into the game: {}", name, error))?;
    }

    // The loader is handed all four directories at launch and will not start
    // when one of them is missing, so a profile that predates them (or that a
    // user emptied) gets them back here.
    for route in ["mod", "pak", "cfg", "overlay"] {
        fs::create_dir_all(profile_dir.join("shimloader").join(route))
            .map_err(|error| error.to_string())?;
    }

    log::info!(
        "[sync_profile_to_game] Installed the shimloader runtime into {:?}",
        binaries_dir
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::link_shimloader_runtime;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-install-{}-{}-{}",
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

    #[test]
    fn applying_a_shimloader_profile_moves_only_the_runtime_into_the_game() {
        let root = temp_root("shimloader");
        let profile = root.join("profile");
        let game = root.join("game");
        let binaries = game.join("Pal/Binaries/Win64");
        std::fs::create_dir_all(&binaries).unwrap();
        std::fs::create_dir_all(profile.join("shimloader/mod/auth-test_mod")).unwrap();
        std::fs::write(profile.join("dwmapi.dll"), b"shim").unwrap();
        std::fs::write(profile.join("ue4ss.dll"), b"ue4ss").unwrap();
        std::fs::write(profile.join("UE4SS-settings.ini"), b"settings").unwrap();
        std::fs::write(
            profile.join("shimloader/mod/auth-test_mod/main.lua"),
            b"mod",
        )
        .unwrap();

        link_shimloader_runtime(&profile, &game, "palworld", false).unwrap();

        assert_eq!(
            std::fs::read_to_string(binaries.join("dwmapi.dll")).unwrap(),
            "shim"
        );
        assert!(binaries.join("ue4ss.dll").is_file());
        assert!(binaries.join("UE4SS-settings.ini").is_file());
        // Mods are handed to the loader by path, never copied into the game.
        assert!(!binaries.join("Mods").exists());
        assert!(!game.join("shimloader").exists());
        for route in ["mod", "pak", "cfg", "overlay"] {
            assert!(profile.join("shimloader").join(route).is_dir());
        }

        // Vanilla is the same operation in reverse.
        link_shimloader_runtime(&profile, &game, "palworld", true).unwrap();
        assert!(!binaries.join("dwmapi.dll").exists());
        assert!(!binaries.join("ue4ss.dll").exists());
        assert!(profile.join("dwmapi.dll").is_file());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn applying_without_the_loader_says_so_instead_of_half_installing() {
        let root = temp_root("shimloader-bare");
        let profile = root.join("profile");
        let game = root.join("game");
        std::fs::create_dir_all(profile.join("shimloader/mod")).unwrap();
        std::fs::create_dir_all(game.join("Pal/Binaries/Win64")).unwrap();

        let error = link_shimloader_runtime(&profile, &game, "palworld", false).unwrap_err();
        assert!(error.contains("not installed"), "{error}");

        // A game folder with no Unreal layout is a misconfigured path, and the
        // message has to name the folder that was searched.
        let empty_game = root.join("elsewhere");
        std::fs::create_dir_all(&empty_game).unwrap();
        std::fs::write(profile.join("dwmapi.dll"), b"shim").unwrap();
        let error = link_shimloader_runtime(&profile, &empty_game, "palworld", false).unwrap_err();
        assert!(error.contains("Binaries/Win64"), "{error}");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
