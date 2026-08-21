use super::super::owml_patcher;
use super::super::*;

/// How long to wait for BepInEx to announce itself after the game is up.
const BEPINEX_LOG_GRACE_MS: u64 = 20_000;

/// Did BepInEx actually take over this launch?
///
/// Doorstop is injected by the launch script and reports its own progress, but
/// nothing downstream checks whether it reached BepInEx: the game starts either
/// way, so a loader that silently failed to hook the Mono runtime looks exactly
/// like a successful modded launch.
///
/// Two signals, because neither alone is enough. `LogOutput.log` is missing on
/// games whose disk logging is off — Muck runs BepInEx and never writes one —
/// while `cache/` is written by the preloader as it patches assemblies. Either
/// one appearing after the launch means BepInEx got control.
fn macos_bepinex_took_over(
    game_path: &std::path::Path,
    launched_at: std::time::SystemTime,
) -> bool {
    let bepinex = game_path.join("BepInEx");
    let signals = [bepinex.join("LogOutput.log"), bepinex.join("cache")];
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(BEPINEX_LOG_GRACE_MS);

    while std::time::Instant::now() < deadline {
        // The user can stop waiting here too: this grace period runs after the
        // game is already up, so there is nothing to gain by making them sit
        // through it.
        if super::super::launch_cancel::launch_cancelled() {
            return false;
        }

        for signal in &signals {
            // Anything left over from an earlier session proves nothing.
            if std::fs::metadata(signal)
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified >= launched_at)
            {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    false
}

/// The message shown when the game started but the mods did not load.
fn macos_mods_not_loaded_error() -> String {
    "The game started, but BepInEx never loaded, so it is running unmodded. The loader could not attach to the game, this is not a problem with your mods. The r2modmac logs in the game folder record what Doorstop reported."
        .to_string()
}

pub(crate) async fn launch_game_with_mods_for_macos(
    app: &AppHandle,
    game_identifier: &str,
    profile_id: &str,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let settings = load_settings_impl(app);
    let launch_mode = get_profile_launch_mode(app, profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    let runtime_game_path = if is_balatro_identifier(game_identifier)
        || is_balatro_game_path(game_path)
        || is_outerwilds_identifier(game_identifier)
        || is_outerwilds_game_path(game_path)
    {
        game_path.to_path_buf()
    } else {
        let resolved = resolve_macos_runtime_root(game_path);
        if resolved != game_path {
            log::info!(
                "[launch_game_with_mods] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    };
    let executable_path = find_macos_executable_path(&runtime_game_path);

    // Outer Wilds: run socket-free patcher then launch OuterWilds.exe directly via Wine/CrossOver
    if is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(game_path) {
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if owml_disabled.exists() && !owml_folder.exists() {
            let _ = fs::rename(&owml_disabled, &owml_folder);
            log::info!("[launch_game_with_mods] Restored OWML_DISABLED -> OWML");
        }

        if !owml_folder.exists() {
            return Err("OWML folder not found. Please install OWML first.".to_string());
        }

        // Restore modded DLLs (Assembly-CSharp + mscorlib)
        let _ = restore_outerwilds_modded(game_path);

        // Run socket-free patcher — no sockets, no crashing
        log::info!("[launch_game_with_mods] Running OWMLPatcher.exe via Wine");
        if let Err(e) = owml_patcher::run_owml_patcher(game_path) {
            log::warn!(
                "[launch_game_with_mods] OWMLPatcher failed (non-fatal, continuing): {}",
                e
            );
        }

        // Launch OuterWilds.exe directly — mods injected via patched Assembly-CSharp.dll
        log::info!("[launch_game_with_mods] Launching OuterWilds.exe directly");
        return launch_windows_game(app, game_path, None);
    }

    if is_balatro_identifier(game_identifier) || is_balatro_game_path(game_path) {
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
        dequarantine_recursive(game_path);

        std::process::Command::new("/bin/sh")
            .arg(&run_script)
            .current_dir(game_path)
            .spawn()
            .map_err(|e| format!("Failed to launch run_lovely_macos.sh: {}", e))?;

        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, 60_000);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_with_mods] run_lovely_macos.sh launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }

        return Ok(());
    }

    validate_macos_bepinex_support(&runtime_game_path)?;
    sync_macos_runtime_disabled_state(&runtime_game_path, false).map_err(|error| {
        format!(
            "Failed to enable macOS runtime before modded launch ({}): {}",
            runtime_game_path.display(),
            error
        )
    })?;

    let dist = infer_distribution_from_game_path(app, game_path, false);
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

        if let Ok(false) = macos_steam_launch_option_matches_desired(app, game_path) {
            log::info!(
                "[launch_game_with_mods] Steam launch option differs (arch/script mismatch) — reconciling managed option before Steam launch."
            );
            ensure_macos_steam_launch_options(app, game_path, true, true)?;
        }
        let launched_at = std::time::SystemTime::now();
        launch_via_steam_for_game_path(app, game_path)?;
        if !macos_bepinex_took_over(&runtime_game_path, launched_at) {
            // A cancelled wait proves nothing about the loader, so it must not
            // be reported as mods that failed to load.
            super::super::launch_cancel::ensure_not_cancelled()?;
            return Err(macos_mods_not_loaded_error());
        }
        return Ok(());
    }

    if launch_macos_bepinex_wrapper(
        app,
        &runtime_game_path,
        executable_path.as_ref(),
        "launch_game_with_mods",
    )? {
        return Ok(());
    }

    let app_bundle = find_macos_launch_bundle(game_path);
    if let Some(bundle) = app_bundle {
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        let _ = open::that(&bundle);
        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_with_mods] App bundle launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    Err("run_bepinex.sh not found and no .app bundle found either".to_string())
}

pub(crate) async fn launch_game_vanilla_for_macos(
    app: &AppHandle,
    game_identifier: &str,
    profile_id: &str,
    game_path_str: &str,
) -> Result<(), String> {
    let game_path = std::path::PathBuf::from(game_path_str);
    let settings = load_settings_impl(app);
    let launch_mode = get_profile_launch_mode(app, profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    let runtime_game_path = if is_balatro_identifier(game_identifier)
        || is_balatro_game_path(&game_path)
        || is_outerwilds_identifier(game_identifier)
        || is_outerwilds_game_path(&game_path)
    {
        game_path.clone()
    } else {
        let resolved = resolve_macos_runtime_root(&game_path);
        if resolved != game_path {
            log::info!(
                "[launch_game_vanilla] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    };

    let is_outerwilds =
        is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(&game_path);
    if is_outerwilds {
        // Disable OWML by renaming folder
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if owml_folder.exists() {
            if owml_disabled.exists() {
                let _ = fs::remove_dir_all(&owml_disabled);
            }
            let _ = fs::rename(&owml_folder, &owml_disabled);
            log::info!("[launch_game_vanilla] Renamed OWML -> OWML_DISABLED");
        }

        // Restore vanilla DLLs (Assembly-CSharp + mscorlib)
        let _ = restore_outerwilds_vanilla(&game_path);
        let _ = restore_mscorlib_vanilla(&game_path, false);

        // Launch OuterWilds.exe directly — vanilla, no mods
        log::info!("[launch_game_vanilla] Launching OuterWilds.exe directly (vanilla)");
        return launch_windows_game(app, &game_path, None);
    }

    let executable_path = find_macos_executable_path(&runtime_game_path);

    #[cfg(target_os = "macos")]
    if let Some(app_bundle) = find_macos_launch_bundle(&game_path) {
        if is_steam_bundle_path(&app_bundle) {
            log::info!(
                "[launch_game_vanilla] Skipping signature/quarantine changes for Steam bundle: {}",
                app_bundle.display()
            );
        } else {
            let is_bundled_steam_emu = find_macos_wrapper_launcher_path(&game_path).is_some();
            let needs_resign = !is_bundled_steam_emu
                && std::process::Command::new("codesign")
                    .args(["-v", "--strict", &app_bundle.to_string_lossy()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| !s.success())
                    .unwrap_or(false);

            if needs_resign {
                log::warn!(
                    "[launch_game_vanilla] App bundle has invalid/no signature — re-signing with ad-hoc"
                );
                let _ = std::process::Command::new("codesign")
                    .args([
                        "--force",
                        "--deep",
                        "-s",
                        "-",
                        &app_bundle.to_string_lossy(),
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else if is_bundled_steam_emu {
                log::info!(
                    "[launch_game_vanilla] Skipping re-sign for bundled Steam-emu wrapper (GOG-style): {}",
                    app_bundle.display()
                );
            }

            let dequarantine_recursive = std::process::Command::new("xattr")
                .args(["-dr", "com.apple.quarantine", &app_bundle.to_string_lossy()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let recursive_ok = dequarantine_recursive
                .as_ref()
                .map(|s| s.success())
                .unwrap_or(false);
            if !recursive_ok {
                let _ = std::process::Command::new("xattr")
                    .args(["-c", &app_bundle.to_string_lossy()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }

    let dist = infer_distribution_from_game_path(app, &game_path, false);
    if dist == "steam" && !use_direct_launch {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        if let Err(error) = sync_macos_runtime_disabled_state(&runtime_game_path, true) {
            log::warn!(
                "[launch_game_vanilla] Failed to enforce runtime_disabled state before vanilla Steam launch: {}",
                error
            );
        }
        if let Ok(true) = macos_steam_launch_option_is_managed(app, &game_path) {
            log::info!(
                "[launch_game_vanilla] Managed BepInEx launch option detected — clearing it before vanilla Steam launch so native macOS games start outside the mod wrapper."
            );
            ensure_macos_steam_launch_options(app, &game_path, false, false)?;
        }

        return launch_via_steam_for_game_path(app, &game_path);
    }

    if find_macos_wrapper_launcher_path(&game_path).is_some() {
        if let Some(inner_executable_path) = executable_path.as_ref() {
            if let Some(inner_app_bundle) = find_enclosing_app_bundle(inner_executable_path) {
                if !is_macos_bundle_signature_valid(&inner_app_bundle) {
                    log::warn!(
                        "[launch_game_vanilla] Inner game app has invalid/no signature — re-signing with ad-hoc: {}",
                        inner_app_bundle.display()
                    );
                    if !ad_hoc_sign_macos_bundle(&inner_app_bundle) {
                        log::warn!(
                            "[launch_game_vanilla] Failed to ad-hoc sign inner game app bundle: {}",
                            inner_app_bundle.display()
                        );
                    }
                }

                clear_macos_bundle_quarantine(&inner_app_bundle);
            }
        }

        sync_macos_runtime_disabled_state(&runtime_game_path, true)?;
        if launch_macos_bepinex_wrapper(
            app,
            &runtime_game_path,
            executable_path.as_ref(),
            "launch_game_vanilla",
        )? {
            return Ok(());
        }
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
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_vanilla] App bundle launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    if let Ok(Some(executable)) = find_game_executable(game_path_str.to_string()).await {
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        open::that(executable).map_err(|e| format!("Failed to launch game executable: {}", e))?;
        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
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
        let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
        observed.ok_unless_cancelled()?;
        if !observed.started() {
            return Err("Game did not start in time.".to_string());
        }
    }
    Ok(())
}

pub(crate) fn is_game_running_for_macos(game_path: &std::path::Path) -> Result<bool, String> {
    let Some(executable_path) = find_macos_executable_path(game_path) else {
        return Ok(false);
    };

    Ok(is_process_running_for_executable(&executable_path))
}

#[cfg(target_os = "macos")]
pub(crate) fn stop_game_for_macos(game_path: &std::path::Path) -> Result<(), String> {
    let executable_path = find_macos_executable_path(game_path)
        .ok_or_else(|| "Could not determine the game executable.".to_string())?;
    let exec_patterns = build_macos_process_kill_patterns(&executable_path);

    if !is_process_running_for_executable(&executable_path) {
        return Ok(());
    }

    for pattern in &exec_patterns {
        let _ = std::process::Command::new("/usr/bin/pkill")
            .args(["-TERM", "-f", pattern])
            .status()
            .map_err(|e| format!("Failed to stop the game: {}", e))?;
    }

    if wait_for_process_exit_patterns(&exec_patterns, 5_000) {
        return Ok(());
    }

    for pattern in &exec_patterns {
        let _ = std::process::Command::new("/usr/bin/pkill")
            .args(["-KILL", "-f", pattern])
            .status()
            .map_err(|e| format!("Failed to force stop the game: {}", e))?;
    }

    if !wait_for_process_exit_patterns(&exec_patterns, 3_000) {
        return Err("Game did not stop in time.".to_string());
    }

    Ok(())
}
