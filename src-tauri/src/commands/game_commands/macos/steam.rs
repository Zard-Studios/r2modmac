use super::*;

pub(crate) fn macos_steam_binary_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from(
        "/Applications/Steam.app/Contents/MacOS/steam_osx",
    )];

    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("Steam")
                .join("Steam.AppBundle")
                .join("Steam")
                .join("Contents")
                .join("MacOS")
                .join("steam_osx"),
        );
    }

    candidates
}

pub(crate) fn dispatch_macos_steam_run_url(app_id: &str) -> Result<std::process::Child, String> {
    let steam_url = format!("steam://run/{}", app_id);

    #[cfg(target_os = "macos")]
    {
        if let Ok(child) = std::process::Command::new("/usr/bin/open")
            .arg(&steam_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            return Ok(child);
        }

        if let Ok(child) = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.valvesoftware.steam"])
            .arg(&steam_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            return Ok(child);
        }

        for steam_binary in macos_steam_binary_candidates() {
            if !steam_binary.is_file() {
                continue;
            }
            if let Ok(child) = std::process::Command::new(&steam_binary)
                .arg(&steam_url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                return Ok(child);
            }
        }
    }

    std::process::Command::new("/usr/bin/open")
        .arg(&steam_url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to ask Steam to launch the game: {}", e))
}

pub(crate) fn launch_via_steam_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let launch_start = std::time::Instant::now();
    let app_id = find_steam_app_id_for_game_path_any(app, game_path, false)
        .ok_or_else(|| "Couldn't determine the Steam app ID for this game".to_string())?;
    let executable_path = find_macos_executable_path(game_path);
    eprintln!(
        "[launch_via_steam_for_game_path] start app_id={} game_path={}",
        app_id,
        game_path.display()
    );

    ensure_macos_steam_running_for_launch(app);
    let child = dispatch_macos_steam_run_url(&app_id)?;
    eprintln!(
        "[launch_via_steam_for_game_path] open_dispatched pid={} elapsed_ms={}",
        child.id(),
        launch_start.elapsed().as_millis()
    );

    if let Some(executable_path) = executable_path.as_ref() {
        if !wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
            eprintln!(
                "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                app_id
            );
        }
    }

    eprintln!(
        "[launch_via_steam_for_game_path] done app_id={} total_elapsed_ms={}",
        app_id,
        launch_start.elapsed().as_millis()
    );

    Ok(())
}
