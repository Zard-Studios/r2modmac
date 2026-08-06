use super::*;

pub(crate) fn macos_steam_launch_option_is_managed(
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
            "Couldn't locate Steam's localconfig.vdf for macOS launch option inspection."
                .to_string(),
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

pub(crate) fn managed_macos_launch_option_for_game(
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

pub(crate) fn macos_steam_launch_option_matches_desired(
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
            "Couldn't locate Steam's localconfig.vdf for macOS launch option inspection."
                .to_string(),
        );
    }

    let mut saw_managed_option_for_game = false;
    let mut saw_managed_arch_mismatch = false;

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

        if managed_macos_launch_option_semantically_matches_desired(&current, &desired, game_path) {
            log::debug!(
                "[macos_steam_launch_option_matches_desired] semantically equivalent managed option for app_id={} localconfig={} current={:?} desired={:?}",
                app_id,
                localconfig_path.display(),
                current,
                desired
            );
            return Ok(true);
        }

        if is_managed_macos_launch_option_for_game(&current, game_path) {
            saw_managed_option_for_game = true;
            if extract_macos_launch_option_arch(&current)
                != extract_macos_launch_option_arch(&desired)
            {
                saw_managed_arch_mismatch = true;
            }
        }
    }

    if saw_managed_option_for_game {
        // Managed launch option exists for this game, but it does not match the
        // current desired one (commonly after runtime arch/script changes).
        // Let caller reconcile the option.
        if saw_managed_arch_mismatch {
            log::warn!(
                "[macos_steam_launch_option_matches_desired] managed option arch mismatch for app_id={} game_path={} desired={:?}",
                app_id,
                game_path.display(),
                desired
            );
        } else {
            log::warn!(
                "[macos_steam_launch_option_matches_desired] managed option mismatch for app_id={} game_path={} desired={:?}",
                app_id,
                game_path.display(),
                desired
            );
        }
        return Ok(false);
    }

    log::debug!(
        "[macos_steam_launch_option_matches_desired] no matching launch option for app_id={} game_path={} desired={:?}",
        app_id,
        game_path.display(),
        desired
    );
    Ok(false)
}

pub(crate) fn get_latest_localconfig_path(
    steam_root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    get_all_localconfig_paths(steam_root).into_iter().next()
}

pub(crate) fn get_all_localconfig_paths(steam_root: &std::path::Path) -> Vec<std::path::PathBuf> {
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
            if let Ok(block_re) = regex::Regex::new(r#"(?s)"(?P<id>\d+)"\s*\{(?P<body>.*?)\n\s*\}"#)
            {
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

pub(crate) fn is_managed_macos_launch_option(value: &str) -> bool {
    let lower = value.to_lowercase();
    let has_script = lower.contains("run_bepinex.sh") || lower.contains("bepinex.sh");
    has_script && lower.contains("%command%")
}

pub(crate) fn extract_macos_launch_script_path(value: &str) -> Option<std::path::PathBuf> {
    let re = regex::Regex::new(r#""([^"]*(?:run_bepinex\.sh|bepinex\.sh))""#).ok()?;
    re.captures(value).and_then(|captures| {
        captures
            .get(1)
            .map(|path| std::path::PathBuf::from(path.as_str()))
    })
}

fn extract_macos_launch_option_arch(value: &str) -> Option<&'static str> {
    if value.contains("/usr/bin/arch -arm64") {
        return Some("arm64");
    }
    if value.contains("/usr/bin/arch -x86_64") {
        return Some("x86_64");
    }
    None
}

fn normalize_launch_script_path_for_compare(path: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        canonicalize_or_original(path)
    } else {
        path.to_path_buf()
    }
}

fn managed_macos_launch_option_semantically_matches_desired(
    current: &str,
    desired: &str,
    game_path: &std::path::Path,
) -> bool {
    if !is_managed_macos_launch_option_for_game(current, game_path) {
        return false;
    }

    let current_arch = extract_macos_launch_option_arch(current);
    let desired_arch = extract_macos_launch_option_arch(desired);
    if current_arch.is_some() && desired_arch.is_some() && current_arch != desired_arch {
        return false;
    }

    let Some(current_script_path) = extract_macos_launch_script_path(current) else {
        return false;
    };
    let Some(desired_script_path) = extract_macos_launch_script_path(desired) else {
        return false;
    };

    normalize_launch_script_path_for_compare(&current_script_path)
        == normalize_launch_script_path_for_compare(&desired_script_path)
}

pub(crate) fn is_managed_macos_launch_option_for_game(
    value: &str,
    game_path: &std::path::Path,
) -> bool {
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

pub(crate) fn ensure_macos_steam_launch_options(
    app: &AppHandle,
    game_path: &std::path::Path,
    enable_mods: bool,
    relaunch_steam_after_update: bool,
) -> Result<(), String> {
    let ensure_started = std::time::Instant::now();
    log::debug!(
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
        .ok_or_else(|| {
            "No Steam installation found to configure macOS launch options".to_string()
        })?;

    let localconfig_paths = get_all_localconfig_paths(&steam_root_for_config);
    if localconfig_paths.is_empty() {
        return Err(
            "Couldn't locate Steam's localconfig.vdf for automatic macOS launch option setup."
                .to_string(),
        );
    }

    log::debug!(
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
            log::debug!(
                "[ensure_macos_steam_launch_options] ensure_steam_stopped elapsed_ms={} steam_was_running={}",
                stop_started.elapsed().as_millis(),
                steam_was_running.unwrap_or(false)
            );
        }
        Ok(())
    };
    let mut write_localconfig_with_optional_steam_stop = |localconfig_path: &std::path::Path,
                                                          updated_text: &str,
                                                          action: &str|
     -> Result<(), String> {
        #[cfg(target_os = "macos")]
        let steam_app_running = is_steam_app_running_on_macos();
        #[cfg(not(target_os = "macos"))]
        let steam_app_running = false;

        if steam_app_running {
            ensure_steam_stopped()?;
            return fs::write(localconfig_path, updated_text)
                .map_err(|e| format!("Failed to {} Steam launch options: {}", action, e));
        }

        match fs::write(localconfig_path, updated_text) {
            Ok(()) => Ok(()),
            Err(first_error) => {
                log::warn!(
                        "[ensure_macos_steam_launch_options] initial {} write failed with Steam.app not running (localconfig={}): {}. Retrying after clearing stale Steam helpers...",
                        action,
                        localconfig_path.display(),
                        first_error
                    );
                ensure_steam_stopped()?;
                fs::write(localconfig_path, updated_text)
                    .map_err(|e| format!("Failed to {} Steam launch options: {}", action, e))
            }
        }
    };

    for localconfig_path in localconfig_paths {
        let localconfig_started = std::time::Instant::now();
        let localconfig = match fs::read_to_string(&localconfig_path) {
            Ok(localconfig) => localconfig,
            Err(error) => {
                log::warn!(
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
        let (updated_text, current_launch_options) =
            match update_launch_options_in_localconfig(&localconfig, &app_id, desired) {
                Ok(result) => result,
                Err(error) => {
                    log::warn!(
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
            let staged_value =
                get_launch_options_for_app(&updated_text, &app_id).ok_or_else(|| {
                    format!("Failed to stage Steam launch options for app {}", app_id)
                })?;
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
                write_localconfig_with_optional_steam_stop(
                    &localconfig_path,
                    &updated_text,
                    "update",
                )?;
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
                log::debug!(
                    "[ensure_macos_steam_launch_options] updated localconfig={} elapsed_ms={}",
                    localconfig_path.display(),
                    write_started.elapsed().as_millis()
                );
            }
        } else if let Some(previous) = settings
            .steam_launch_option_backups
            .remove(&scoped_backup_key)
            .or_else(|| {
                settings
                    .steam_launch_option_backups
                    .remove(&legacy_backup_key)
            })
        {
            let (restored_text, _) =
                update_launch_options_in_localconfig(&localconfig, &app_id, Some(&previous))?;
            if restored_text != localconfig {
                let write_started = std::time::Instant::now();
                write_localconfig_with_optional_steam_stop(
                    &localconfig_path,
                    &restored_text,
                    "restore",
                )?;
                let persisted = fs::read_to_string(&localconfig_path).map_err(|e| {
                    format!("Failed to verify restored Steam launch options: {}", e)
                })?;
                if get_launch_options_for_app(&persisted, &app_id).as_deref()
                    != Some(previous.as_str())
                {
                    return Err(format!(
                        "Steam launch options were not restored correctly for app {} in {}",
                        app_id,
                        localconfig_path.display()
                    ));
                }
                log::debug!(
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
            write_localconfig_with_optional_steam_stop(&localconfig_path, &updated_text, "clear")?;
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
            log::debug!(
                "[ensure_macos_steam_launch_options] cleared localconfig={} elapsed_ms={}",
                localconfig_path.display(),
                write_started.elapsed().as_millis()
            );
        }

        log::debug!(
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
            log::info!(
                "[ensure_macos_steam_launch_options] Steam was closed to update launch options; relaunching Steam now because no immediate steam://run launch follows."
            );
            let relaunch_started = std::time::Instant::now();
            relaunch_macos_steam_if_needed(&steam_root_for_config);
            log::debug!(
                "[ensure_macos_steam_launch_options] relaunch_requested elapsed_ms={}",
                relaunch_started.elapsed().as_millis()
            );
        } else {
            log::debug!(
                "[ensure_macos_steam_launch_options] Steam was closed to update launch options; leaving it closed so the upcoming steam://run launch starts Steam and the game together."
            );
        }
    }

    log::debug!(
        "[ensure_macos_steam_launch_options] done app_id={} total_elapsed_ms={}",
        app_id,
        ensure_started.elapsed().as_millis()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_macos_launch_option_arch_parses_known_flags() {
        assert_eq!(
            extract_macos_launch_option_arch(
                "/usr/bin/arch -arm64 /bin/bash \"/tmp/run_bepinex.sh\" %command%"
            ),
            Some("arm64")
        );
        assert_eq!(
            extract_macos_launch_option_arch(
                "/usr/bin/arch -x86_64 /bin/bash \"/tmp/run_bepinex.sh\" %command%"
            ),
            Some("x86_64")
        );
        assert_eq!(
            extract_macos_launch_option_arch("/bin/bash \"/tmp/run_bepinex.sh\" %command%"),
            None
        );
    }

    #[test]
    fn semantic_match_accepts_same_script_and_arch() {
        let game_path = std::path::Path::new("/tmp/SampleGame");
        let current = "/usr/bin/arch -arm64 /bin/bash \"/tmp/SampleGame/run_bepinex.sh\" %command%";
        let desired = "/usr/bin/arch -arm64 /bin/bash \"/tmp/SampleGame/run_bepinex.sh\" %command%";
        assert!(managed_macos_launch_option_semantically_matches_desired(
            current, desired, game_path
        ));
    }

    #[test]
    fn semantic_match_rejects_arch_mismatch() {
        let game_path = std::path::Path::new("/tmp/SampleGame");
        let current =
            "/usr/bin/arch -x86_64 /bin/bash \"/tmp/SampleGame/run_bepinex.sh\" %command%";
        let desired = "/usr/bin/arch -arm64 /bin/bash \"/tmp/SampleGame/run_bepinex.sh\" %command%";
        assert!(!managed_macos_launch_option_semantically_matches_desired(
            current, desired, game_path
        ));
    }
}
