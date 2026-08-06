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
                        if !wait_for_process_start_patterns(&wait_patterns, 30_000) {
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

    if !wait_for_process_start_patterns(&process_patterns, 20_000) {
        return Err("Game did not start in time.".to_string());
    }

    Ok(())
}

pub(crate) fn launch_windows_steam_game(
    app: &AppHandle,
    game_path: &std::path::Path,
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
                        if !wait_for_process_start_patterns(&process_patterns, 120_000) {
                            log::warn!(
                                "[launch_windows_steam_game] Wineskin accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                                app_id
                            );
                        }
                        return Ok(());
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

    if !wait_for_process_start_patterns(&process_patterns, 60_000) {
        log::warn!(
            "[launch_windows_steam_game] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
            app_id
        );
    }

    Ok(())
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
    let distribution = infer_distribution_from_game_path(app, game_path, true);
    if distribution == "steam" && can_launch_via_steam_for_game_path(app, game_path, true) {
        return launch_windows_steam_game(app, game_path);
    }

    launch_windows_direct_game(game_path)
}
