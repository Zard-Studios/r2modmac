use super::super::*;

pub(crate) fn is_game_running_for_windows(game_path: &std::path::Path) -> Result<bool, String> {
    let Some(executable_path) = find_pe_game_executable_path(game_path) else {
        return Ok(false);
    };

    Ok(is_process_running_for_patterns(
        &build_windows_process_match_patterns(&executable_path),
    ))
}

pub(crate) fn stop_game_for_windows(game_path: &std::path::Path) -> Result<(), String> {
    let executable_path = find_pe_game_executable_path(game_path)
        .ok_or_else(|| "Could not determine the Windows game executable.".to_string())?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if !is_process_running_for_patterns(&process_patterns) {
        return Ok(());
    }

    for pattern in &process_patterns {
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("/usr/bin/pkill")
                .args(["-TERM", "-f", pattern])
                .status()
                .map_err(|e| format!("Failed to stop the game: {}", e))?;
        }

        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/IM", &pattern.replace("\\", "")])
                .status()
                .map_err(|e| format!("Failed to stop the game: {}", e))?;
        }
    }

    if wait_for_process_exit_patterns(&process_patterns, 5_000) {
        return Ok(());
    }

    for pattern in &process_patterns {
        let _ = std::process::Command::new("/usr/bin/pkill")
            .args(["-KILL", "-f", pattern])
            .status()
            .map_err(|e| format!("Failed to force stop the game: {}", e))?;
    }

    if !wait_for_process_exit_patterns(&process_patterns, 3_000) {
        return Err("Game did not stop in time.".to_string());
    }

    Ok(())
}
