mod launch;
mod process;

pub(crate) use self::launch::*;
pub(crate) use self::process::*;

use super::*;

fn configure_native_version_dll_override(
    command: &mut std::process::Command,
    game_path: &std::path::Path,
) {
    if game_path.join("version.dll").is_file() {
        command.env("WINEDLLOVERRIDES", "version=n,b");
    }
}

pub(crate) fn launch_windows_direct_game(game_path: &std::path::Path) -> Result<(), String> {
    launch_windows_direct_game_with_working_dir(game_path, None)
}

pub(crate) fn launch_windows_direct_game_with_working_dir(
    game_path: &std::path::Path,
    working_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    let executable_path = find_pe_game_executable_path(game_path).ok_or_else(|| {
        "Could not find a Windows game executable in the selected folder.".to_string()
    })?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if is_process_running_for_patterns(&process_patterns) {
        log::warn!(
            "[launch_windows_direct_game] Blocked: 'already running'. Patterns: {:?}",
            process_patterns
        );
        return Err("Game is already running.".to_string());
    }

    let executable_dir =
        working_dir.unwrap_or_else(|| executable_path.parent().unwrap_or(game_path));

    #[cfg(unix)]
    {
        let prefix_root =
            find_wine_prefix_root(&executable_path).or_else(|| find_wine_prefix_root(game_path));

        #[cfg(target_os = "macos")]
        if let Some(prefix_root_path) = prefix_root.as_deref() {
            if let Some(bundle_path) =
                find_macos_wineskin_launcher_binary(Some(prefix_root_path), &executable_path)
            {
                // For Wineskin/PortingKit bundles, Wine renames all processes to `wine64`.
                // The wineserver of the specific bundle is the reliable sentinel that the
                // bundle (and therefore the game) has started. We use it for the post-
                // launch wait, but NOT for the pre-launch check (to avoid false positives
                // when Steam is already running inside the same bundle).
                let wait_patterns = build_windows_wineskin_bundle_patterns(&executable_path)
                    .unwrap_or_else(|| process_patterns.clone());

                match launch_macos_wineskin_program(
                    &bundle_path,
                    prefix_root_path,
                    &executable_path,
                    &[],
                    Some(executable_dir),
                    "launch_windows_direct_game",
                ) {
                    Ok(()) => {
                        let observed = wait_for_process_start_patterns(&wait_patterns, 30_000);
                        observed.ok_unless_cancelled()?;
                        if !observed.started() {
                            log::warn!(
                                "[launch_windows_direct_game] Wineskin bundle started but game process not observed in time. Continuing optimistically."
                            );
                        }
                        return Ok(());
                    }
                    Err(error) => {
                        log::warn!(
                            "[launch_windows_direct_game] Sikarugir/Wineskin launch failed ({}); falling back to direct Wine.",
                            error
                        );
                    }
                }
            }
        }

        if let Some(runner_path) =
            find_host_compat_runner_binary(prefix_root.as_deref(), &executable_path)
        {
            let mut command = std::process::Command::new(&runner_path);
            configure_host_compat_runner_command(
                &mut command,
                &runner_path,
                prefix_root.as_deref(),
            )?;
            configure_native_version_dll_override(&mut command, game_path);
            log::info!(
                "[launch_windows_direct_game] Launching Windows executable directly: {:?}",
                executable_path
            );
            command
                .arg(&executable_path)
                .current_dir(executable_dir)
                .spawn()
                .map_err(|e| {
                    format!(
                        "Failed to launch the Windows game via {}: {}",
                        runner_path.display(),
                        e
                    )
                })?;
        } else if let Some(app_bundle) = find_enclosing_app_bundle(game_path) {
            open::that(&app_bundle)
                .map_err(|e| format!("Failed to launch the Windows wrapper app: {}", e))?;
        } else {
            return Err(
	            "No compatible runner was found for this platform. Install a supported compatibility tool or point the game path inside its prefix."
	                .to_string(),
	        );
        }
    }

    #[cfg(windows)]
    {
        log::info!(
            "[launch_windows_direct_game] Launching Windows executable directly: {:?}",
            executable_path
        );
        std::process::Command::new(&executable_path)
            //.arg("-applaunch")
            .arg(&executable_path)
            .current_dir(executable_dir)
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to launch the Windows game via {}: {}",
                    executable_path.display(),
                    e
                )
            })?;
    }

    let observed = wait_for_process_start_patterns(&process_patterns, 20_000);
    observed.ok_unless_cancelled()?;
    if !observed.started() {
        return Err("Game did not start in time.".to_string());
    }

    Ok(())
}

pub(super) fn launch_windows_steam_game(
    game_path: &std::path::Path,
    target: &SteamLaunchTarget,
) -> Result<(), String> {
    let executable_path = find_pe_game_executable_path(game_path).ok_or_else(|| {
        "Could not find a Windows game executable in the selected folder.".to_string()
    })?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if is_process_running_for_patterns(&process_patterns) {
        log::warn!(
            "[launch_windows_steam_game] Blocked: 'already running'. Patterns: {:?}",
            process_patterns
        );
        return Err("Game is already running.".to_string());
    }

    let steam_root = target.client_root.clone();
    let library_root = target.library_root.clone();
    let app_id = target.app_id.clone();
    let steam_executable = steam_root.join("steam.exe");
    if !steam_executable.exists() {
        return Err(format!(
            "Steam executable not found at {}. Check the Windows Steam directory in Settings.",
            steam_executable.display()
        ));
    }

    // Steam accepts a launch request even when it has no intention of starting
    // the game — a pending update or a paused download parks it indefinitely.
    // Report that up front rather than letting the caller wait for a timeout.
    if let Some(flags) =
        crate::commands::game_commands::steam_state::read_state_flags(&library_root, &app_id)
    {
        if let Some(blocker) =
            crate::commands::game_commands::steam_state::describe_state_blocker(flags)
        {
            log::warn!(
                "[launch_windows_steam_game] Refusing to launch app {}: StateFlags={} ({})",
                app_id,
                flags,
                blocker
            );
            return Err(blocker);
        }
    }

    // A cold Steam has to boot and sign in before it acts on the request, which
    // is much slower than a warm one and needs a different deadline (and a
    // different explanation if it runs out).
    let steam_was_running =
        is_process_running_for_patterns(&build_windows_process_match_patterns(&steam_executable));
    log::info!(
        "[launch_windows_steam_game] Steam already running: {}",
        steam_was_running
    );

    #[cfg(unix)]
    {
        let prefix_root = find_wine_prefix_root(&steam_executable)
            .or_else(|| find_wine_prefix_root(&executable_path))
            .or_else(|| find_wine_prefix_root(game_path));

        #[cfg(target_os = "macos")]
        if let Some(prefix_root_path) = prefix_root.as_deref() {
            if let Some(bundle_path) =
                find_macos_wineskin_launcher_binary(Some(prefix_root_path), &steam_executable)
            {
                let args = vec!["-applaunch".to_string(), app_id.clone()];
                match launch_macos_wineskin_program(
                    &bundle_path,
                    prefix_root_path,
                    &steam_executable,
                    &args,
                    Some(game_path),
                    "launch_windows_steam_game",
                ) {
                    Ok(()) => {
                        let timeout_ms = if steam_was_running { 60_000 } else { 180_000 };
                        match crate::commands::game_commands::steam_state::wait_for_launch_or_blocker(
                            &steam_root,
                            &library_root,
                            &app_id,
                            timeout_ms,
                            || is_process_running_for_patterns(&process_patterns),
                            crate::commands::game_commands::launch_cancel::launch_cancelled,
                        ) {
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::Started => return Ok(()),
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::Blocked(reason) => {
                                log::warn!(
                                    "[launch_windows_steam_game] Steam stalled the launch of app {}: {}",
                                    app_id,
                                    reason
                                );
                                return Err(reason);
                            }
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::Cancelled => {
                                log::info!(
                                    "[launch_windows_steam_game] Stopped waiting for app {} because the user cancelled the launch.",
                                    app_id
                                );
                                return Err(crate::commands::game_commands::launch_cancel::LAUNCH_CANCELLED_MESSAGE.to_string());
                            }
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::TimedOut => {
                                log::warn!(
                                    "[launch_windows_steam_game] Wineskin accepted the launch request for app {}, but the game process was not observed in time.",
                                    app_id
                                );
                                return Err(if steam_was_running {
                                    "Steam accepted the launch but the game did not start. Open Steam to check for a prompt or an error waiting for you there.".to_string()
                                } else {
                                    "Steam was not running, so r2modmac started it first — but the game did not start in time. Check the Steam window and try again.".to_string()
                                });
                            }
                        }
                    }
                    Err(error) => {
                        log::warn!(
                            "[launch_windows_steam_game] Wineskin launch failed ({}); falling back to direct Wine.",
                            error
                        );
                    }
                }
            }
        }

        let runner_path = find_host_compat_runner_binary(prefix_root.as_deref(), &steam_executable)
			.ok_or_else(|| {
				"No compatible runner was found for this Steam installation. Set the game path inside a supported compatibility-tool prefix and try again."
					.to_string()
			})?;

        let mut command = std::process::Command::new(&runner_path);
        configure_host_compat_runner_command(&mut command, &runner_path, prefix_root.as_deref())?;
        configure_native_version_dll_override(&mut command, game_path);
        log::info!(
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

    #[cfg(windows)]
    {
        log::info!(
            "[launch_windows_steam_game] Launching Steam app {} via {:?}",
            app_id,
            steam_executable
        );
        std::process::Command::new(&steam_executable)
            .arg("-applaunch")
            .arg(&app_id)
            .current_dir(&steam_root)
            .spawn()
            .map_err(|e| format!("Failed to launch Steam app {}: {}", app_id, e))?;
    }

    // Steam can take the request and never create the process — parked on a
    // prompt the user cannot see while looking at r2modmac (a Steam Cloud
    // conflict, most often), or on an update it decided to fetch first. Watch
    // Steam's own state while waiting so the reason is reported as soon as it
    // is recorded, rather than after the full timeout.
    //
    // A cold Steam has to boot before it can even consider the request, which
    // takes far longer than a warm one, so the deadline accounts for that.
    let timeout_ms = if steam_was_running { 60_000 } else { 180_000 };
    match crate::commands::game_commands::steam_state::wait_for_launch_or_blocker(
        &steam_root,
        &library_root,
        &app_id,
        timeout_ms,
        || is_process_running_for_patterns(&process_patterns),
        crate::commands::game_commands::launch_cancel::launch_cancelled,
    ) {
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::Started => Ok(()),
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::Blocked(reason) => {
            log::warn!(
                "[launch_windows_steam_game] Steam stalled the launch of app {}: {}",
                app_id,
                reason
            );
            Err(reason)
        }
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::Cancelled => {
            log::info!(
                "[launch_windows_steam_game] Stopped waiting for app {} because the user cancelled the launch.",
                app_id
            );
            Err(crate::commands::game_commands::launch_cancel::LAUNCH_CANCELLED_MESSAGE.to_string())
        }
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::TimedOut => {
            log::warn!(
                "[launch_windows_steam_game] Steam accepted the launch request for app {} but the game did not start within {}ms.",
                app_id,
                timeout_ms
            );
            Err(if steam_was_running {
                "Steam accepted the launch but the game did not start. Open Steam to check for a prompt or an error waiting for you there.".to_string()
            } else {
                "Steam was not running, so r2modmac started it first — but the game did not start in time. Steam may still be signing in; check the Steam window, then press Play again.".to_string()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::configure_native_version_dll_override;
    use std::ffi::OsStr;

    #[test]
    fn return_of_modding_loader_enables_native_version_dll_under_wine() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-rom-wine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("version.dll"), b"loader").unwrap();
        let mut command = std::process::Command::new("wine");
        configure_native_version_dll_override(&mut command, &root);
        assert!(command.get_envs().any(|(key, value)| {
            key == OsStr::new("WINEDLLOVERRIDES") && value == Some(OsStr::new("version=n,b"))
        }));
        std::fs::remove_dir_all(root).unwrap();
    }
}

pub(crate) fn launch_windows_game(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let steam_roots = get_steam_roots_for_platform(app, true);
    log::info!(
        "[launch_windows_game] Planning launch of {:?}. Known Windows Steam roots: {:?}",
        game_path,
        steam_roots
    );

    match plan_windows_launch(&steam_roots, game_path) {
        WindowsLaunchPlan::ViaSteam(target) => launch_windows_steam_game(game_path, &target),
        WindowsLaunchPlan::Direct => launch_windows_direct_game(game_path),
        WindowsLaunchPlan::SteamClientMissing => Err(
            "This game is installed through Steam, but r2modmac could not find the Windows Steam client that owns it. Set the Windows Steam directory (the folder that contains steam.exe, inside your CrossOver/Wine bottle) in Settings and try again."
                .to_string(),
        ),
    }
}
