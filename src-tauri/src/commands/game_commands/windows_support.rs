use super::*;

pub(crate) fn is_windows_game_executable_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe")
        && !lower.contains("unitycrashhandler")
        && !lower.contains("crashhandler")
        && !lower.starts_with("unins")
        && !lower.contains("setup")
}

pub(crate) fn windows_executable_score(
    game_path: &std::path::Path,
    executable_path: &std::path::Path,
) -> i32 {
    let stem = executable_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let folder_name = game_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    let normalized_stem = normalize_for_matching(stem);
    let normalized_folder = normalize_for_matching(folder_name);
    let lower_stem = stem.to_lowercase();

    let mut score = 0;
    if !normalized_stem.is_empty() && !normalized_folder.is_empty() {
        if normalized_stem == normalized_folder {
            score += 120;
        } else if normalized_stem.contains(&normalized_folder)
            || normalized_folder.contains(&normalized_stem)
        {
            score += 70;
        }
    }

    if lower_stem.contains("launcher") || lower_stem.contains("bootstrap") {
        score -= 25;
    }
    if lower_stem.contains("eac") || lower_stem.contains("battleye") {
        score -= 20;
    }

    score - (stem.len() as i32)
}

pub(crate) fn find_pe_game_executable_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    if game_path.is_file() {
        let name = game_path.file_name()?.to_string_lossy().to_string();
        if is_windows_game_executable_name(&name) {
            return Some(game_path.to_path_buf());
        }
        return None;
    }

    let entries = fs::read_dir(game_path).ok()?;
    let mut candidates: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(is_windows_game_executable_name)
                .unwrap_or(false)
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by_key(|path| std::cmp::Reverse(windows_executable_score(game_path, path)));
    candidates.into_iter().next()
}

pub(crate) fn find_host_compat_runner_binary(
    prefix_root: Option<&std::path::Path>,
    executable_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        find_macos_compat_runner_binary(prefix_root, executable_path)
    }

    #[cfg(target_os = "linux")]
    {
        find_linux_compat_runner_binary(prefix_root)
    }

    #[cfg(target_os = "windows")]
    {
        let _ = prefix_root;
        let _ = executable_path;
        None
    }
}

pub(crate) fn configure_host_compat_runner_command(
    command: &mut std::process::Command,
    runner_path: &std::path::Path,
    prefix_root: Option<&std::path::Path>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        configure_macos_compat_runner_command(command, runner_path, prefix_root)
    }

    #[cfg(target_os = "linux")]
    {
        configure_linux_compat_runner_command(command, prefix_root)
    }

    #[cfg(target_os = "windows")]
    {
        let _ = command;
        let _ = runner_path;
        let _ = prefix_root;
        Ok(())
    }
}

pub(crate) fn ensure_windows_bepinex_console_enabled(
    game_path: &std::path::Path,
) -> Result<(), String> {
    let config_path = game_path.join("BepInEx").join("config").join("BepInEx.cfg");
    if !config_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read BepInEx.cfg: {}", e))?;

    if content.contains("[Logging.Console]") && content.contains("Enabled = true") {
        return Ok(());
    }

    let updated = if content.contains("[Logging.Console]") && content.contains("Enabled = false") {
        content.replacen("Enabled = false", "Enabled = true", 1)
    } else if content.contains("[Logging.Console]") {
        content
    } else {
        format!(
            "{}\n[Logging.Console]\nEnabled = true\n",
            content.trim_end()
        )
    };

    fs::write(&config_path, updated).map_err(|e| format!("Failed to update BepInEx.cfg: {}", e))?;
    Ok(())
}
