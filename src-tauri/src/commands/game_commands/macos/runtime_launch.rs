use super::*;

pub(crate) fn remove_r2modmac_debug_logs(game_path: &std::path::Path) {
    let runtime_root = resolve_macos_runtime_root(game_path);
    for log_name in [
        "r2modmac_bootstrap.log",
        "r2modmac_dyld.log",
        "r2modmac_exec.log",
    ] {
        let log_path = runtime_root.join(log_name);
        if log_path.exists() {
            let _ = fs::remove_file(&log_path);
        }
    }

    let legacy_bootstrap_log = runtime_root.join("BepInEx").join("r2modmac_bootstrap.log");
    if legacy_bootstrap_log.exists() {
        let _ = fs::remove_file(&legacy_bootstrap_log);
    }
}

pub(crate) fn launch_macos_bepinex_wrapper(
    app: &AppHandle,
    game_path: &std::path::Path,
    executable_path: Option<&std::path::PathBuf>,
    context: &str,
) -> Result<bool, String> {
    let runtime_root = resolve_macos_runtime_root(game_path);

    if find_bepinex_script_in_dir(&runtime_root).is_none() {
        return Ok(false);
    }

    let run_script = canonicalize_macos_bepinex_script(&runtime_root)?;

    if let Some(executable_path) = executable_path.as_ref() {
        if is_process_running_for_executable(executable_path) {
            return Err("Game is already running.".to_string());
        }
    }

    let write_debug_logs_to_game = load_settings_impl(app).write_debug_logs_to_game;
    if !write_debug_logs_to_game {
        remove_r2modmac_debug_logs(&runtime_root);
    }

    configure_macos_bepinex_script(&run_script, &runtime_root, write_debug_logs_to_game, None)?;
    dequarantine_recursive(&runtime_root);

    log::info!(
        "[{}] Launching via run_bepinex.sh at {:?}",
        context,
        run_script
    );

    std::process::Command::new("/bin/bash")
        .arg(&run_script)
        .current_dir(&runtime_root)
        .spawn()
        .map_err(|e| format!("Failed to launch run_bepinex.sh: {}", e))?;

    if let Some(executable_path) = executable_path.as_ref() {
        let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
        observed.ok_unless_cancelled()?;
        if !observed.started() {
            log::debug!(
                "[{}] run_bepinex.sh launch request succeeded, but the game process was not observed in time. Continuing optimistically.",
                context
            );
        }
    }

    Ok(true)
}

pub(crate) fn find_bepinex_script_in_dir(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    if !dir.exists() {
        return None;
    }

    let canonical = dir.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    if canonical.exists() {
        return Some(canonical);
    }

    fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                && is_bepinex_shell_script_name(&entry.file_name().to_string_lossy())
        })
        .map(|entry| entry.path())
}

pub(crate) fn canonicalize_macos_bepinex_script(
    dir: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let canonical = dir.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    if canonical.exists() {
        return Ok(canonical);
    }

    let Some(found) = find_bepinex_script_in_dir(dir) else {
        return Err("No macOS BepInEx startup script found".to_string());
    };

    if found == canonical {
        return Ok(canonical);
    }

    if let Err(rename_err) = fs::rename(&found, &canonical) {
        fs::copy(&found, &canonical).map_err(|copy_err| {
            format!(
                "Failed to normalize macOS BepInEx startup script: rename failed ({}), copy failed ({})",
                rename_err, copy_err
            )
        })?;
        let _ = fs::remove_file(&found);
    }

    Ok(canonical)
}

pub(crate) fn has_complete_macos_bepinex_runtime(game_path: &std::path::Path) -> bool {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let has_core = runtime_root.join("BepInEx").join("core").is_dir();
    let has_doorstop_payload = runtime_root.join("doorstop_libs").is_dir()
        || runtime_root.join("libdoorstop.dylib").exists();
    let has_script = find_bepinex_script_in_dir(&runtime_root).is_some();

    has_core && has_doorstop_payload && has_script
}

pub(crate) fn has_complete_disabled_macos_bepinex_runtime(game_path: &std::path::Path) -> bool {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let has_core = runtime_root.join("BepInEx_DISABLED").join("core").is_dir();
    let has_doorstop_payload = runtime_root.join("doorstop_libs_DISABLED").is_dir()
        || runtime_root.join("libdoorstop.dylib_DISABLED").exists();
    let has_script = find_bepinex_script_in_dir(&runtime_root).is_some();

    has_core && has_doorstop_payload && has_script
}
