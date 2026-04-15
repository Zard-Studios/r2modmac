use super::*;

pub(crate) fn configure_macos_bepinex_script(
    script_path: &std::path::Path,
    game_path: &std::path::Path,
    write_debug_logs_to_game: bool,
) -> Result<(), String> {
    if !script_path.exists() {
        return Ok(());
    }

    let executable_path = resolve_macos_executable_path(game_path)?;
    let launch_entry_path = resolve_macos_launch_entry_path(game_path)?;
    // Use the real executable path instead of the outer `.app` bundle so the
    // SteamLaunch branch can match and relaunch the exact binary Steam passes
    // in `%command%`. Using the bundle path here makes the matcher too vague
    // for native macOS Steam launches and can send Doorstop through the wrong
    // entrypoint.
    let relative_executable = executable_path
        .strip_prefix(game_path)
        .ok()
        .unwrap_or(executable_path.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    let relative_launch_entry = launch_entry_path
        .strip_prefix(game_path)
        .ok()
        .unwrap_or(launch_entry_path.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    let launch_entry_uses_wrapper = launch_entry_path != executable_path;

    let mut script = fs::read_to_string(script_path).map_err(|e| e.to_string())?;
    let original = script.clone();
    script = script.replace("\r\n", "\n");

    // CRITICAL: detect if the script has a working runtime_disabled early-exit BEFORE
    // Doorstop validation and DYLD env injection. Original BepInEx scripts set the
    // runtime_disabled variable but never skip loading Doorstop when it's disabled.
    // This causes startup failures in vanilla mode when the payload is renamed
    // to *_DISABLED. Always regenerate if the early-exit is absent or misplaced.
    let has_early_exit = script.contains("if [ \"$runtime_disabled\" = true ]");
    let (early_exit_before_doorstop_validation, early_exit_before_dyld) = if has_early_exit {
        let exit_pos = script
            .find("if [ \"$runtime_disabled\" = true ]")
            .unwrap_or(usize::MAX);
        let doorstop_check_pos = script.find("doorstop_dylib_missing").unwrap_or(usize::MAX);
        let dyld_pos = script.find("DYLD_INSERT_LIBRARIES=").unwrap_or(usize::MAX);
        (exit_pos < doorstop_check_pos, exit_pos < dyld_pos)
    } else {
        (false, false)
    };

    let has_root_doorstop_fallback =
        script.contains("doorstop_dylib=") && script.contains("$BASEDIR/libdoorstop.dylib");
    let has_steam_arg_helper = script.contains("steam_arg_helper()");
    let has_root_bootstrap_log =
        script.contains("bootstrap_log=\"$BASEDIR/r2modmac_bootstrap.log\"");
    let has_expected_debug_log_setting = script.contains(&format!(
        "write_debug_logs={}",
        if write_debug_logs_to_game { 1 } else { 0 }
    ));
    let removes_codesign_signature = script.contains("codesign_adhoc_sign_attempt")
        && script.contains("codesign_adhoc_sign_skipped_valid")
        && script.contains("codesign_remove_signature_skipped_runtime_disabled");
    let has_codesign_cache_guard = script.contains(".r2modmac_codesign_state")
        && script.contains("codesign_adhoc_sign_skipped_cached")
        && script.contains("codesign_state_key");
    let logs_loader_environment =
        script.contains("wrapper_arch=") && script.contains("loader_env LD_LIBRARY_PATH=");
    let has_root_loader_mode_env = script.contains("root_loader_mode=false")
        && script.contains("DOORSTOP_IGNORE_DISABLED_ENV=0")
        && script.contains("DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=\"$BASEDIR/BepInEx/core\"")
        && script.contains("-e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=")
        && script.contains("root_loader_mode=$root_loader_mode")
        && script.contains("export DYLD_INSERT_LIBRARIES=\"libdoorstop.dylib");
    let has_arch_env_exec = script.contains("exec_modded_arch_env target=")
        && script.contains("exec_modded_arm64_env target=")
        && script.contains("native_macos_arch=")
        && script.contains("/usr/bin/arch -arm64")
        && script.contains("-e DYLD_INSERT_LIBRARIES=");
    let has_dyld_loader_logging = script.contains("DYLD_PRINT_LIBRARIES=1")
        && script.contains("dyld_log=\"$BASEDIR/r2modmac_dyld.log\"");
    let has_exec_failure_logging = script.contains("exec_log=\"$BASEDIR/r2modmac_exec.log\"")
        && script.contains("exec_modded_arm64_env_failed status=$exec_status")
        && script.contains("steam_launch_exec_modded_arch_env_failed status=$exec_status");
    let has_native_arm64_direct_exec = script
        .contains("steam_launch_exec_modded_arm64_direct argv=$*")
        && script.contains("steam_launch_exec_modded_arm64_direct_failed status=$exec_status")
        && script.contains(
            "exec_modded_arm64_direct target=$modded_target_path wrapper=$modded_target_is_wrapper",
        )
        && script.contains("exec_modded_arm64_direct_failed status=$exec_status")
        && script.contains("[ \"$wrapper_arch\" = \"arm64\" ]")
        && script.contains("[ \"$wrapper_translated\" = \"0\" ]");
    let has_wrapper_modded_exec_support = script.contains("modded_target_path=")
        && script.contains("modded_target_is_wrapper=false")
        && script.contains("modded_working_dir=\"$BASEDIR\"")
        && script.contains("if [ \"$modded_target_is_wrapper\" = true ]; then")
        && script.contains("/bin/bash \"${modded_target_path}\"")
        && script.contains("cd \"$modded_working_dir\" || exit 1")
        // GOG/Steam-emu bypass: modded target must be the real executable, not the wrapper script
        && script.contains("gog_steamemu_wrapper_bypass")
        && script.contains("steamemu_dir=$steamemu_macos_dir");
    let has_arm64_x64_fallback_retry = script.contains("maybe_retry_x64_after_arm64_failure()")
        && script.contains("can_retry_x64=true")
        && script.contains("BEPINEX_LOG_MTIME_BEFORE")
        && script.contains("bepinex_started=true")
        && script.contains("retry_skipped_clean_exit status=")
        && script.contains("retry_skipped_process_alive status=")
        && script.contains("retrying_x64_fallback status=")
        && script.contains("x64_fallback_failed status=$retry_status");
    let has_preloader_crash_arch_recovery = script.contains("R2MODMAC_LAUNCH_EPOCH")
        && script.contains("arm64_preloader_crash=false")
        && script.contains("retry_preloader_crash_detected status=")
        && script.contains("retrying_x64_after_preloader_crash status=")
        && script.contains("recent_preloader_log=\"\"")
        && script.contains("forcing_x64_due_recent_preloader_crash log=");
    let has_persistent_x64_state = script.contains(".r2modmac_force_x64_state")
        && script.contains("force_x64_persisted=false")
        && script.contains("force_x64_state_present key=")
        && script.contains("forcing_x64_due_persisted_state key=")
        && script.contains("force_x64_state_written key=")
        && script.contains("retry_force_x64_state_written key=");
    let has_cross_generation_doorstop_bool_flags = script.contains("DOORSTOP_ENABLE=TRUE")
        && script.contains("DOORSTOP_ENABLED=1")
        && script.contains("DOORSTOP_TARGET_ASSEMBLY=")
        && script.contains("DOORSTOP_BOOT_CONFIG_OVERRIDE=")
        && script.contains("DOORSTOP_IGNORE_DISABLED_ENV=0")
        && script.contains("DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=\"$BASEDIR/BepInEx/core\"")
        && script.contains("DOORSTOP_REDIRECT_OUTPUT_LOG=1")
        && script.contains("-e DOORSTOP_TARGET_ASSEMBLY=")
        && script.contains("-e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=");
    let has_launch_entry_support = script.contains("launch_entry_name=")
        && script.contains("launch_entry_uses_wrapper=")
        && script.contains("resolved_launch_entry_path=");
    let has_steamemu_runtime_prep =
        script.contains("steamemu_runtime_prepared")
            && script.contains("export SteamAppId=")
            && script.contains("export SteamGameId=")
            && script.contains("steamemu_macos_dir=\"$BASEDIR/MacOS\"")
            && script.contains("STEAMEMU_SETTINGS_DIR=\"$steamemu_config_dir\"")
            && script.contains("-e SteamAppId=")
            // GOG + Steam-emu: ipcserver needs STEAM_PATH to self-locate steamclient.dylib
            && script.contains("export STEAM_PATH=\"$steamemu_macos_dir\"")
            && script.contains("export SteamPath=\"$steamemu_macos_dir\"")
            && script.contains("steamemu_runtime_dir_unreachable dir=$steamemu_macos_dir")
            && script.contains("steamemu_previous_dir=$(pwd)")
            && script.contains("if cd \"$steamemu_macos_dir\" 2>/dev/null; then")
            && script.contains(
                "prepare_steamemu_runtime_files \"$BASEDIR\" \"$steamemu_macos_dir\"",
            )
            && script.contains(
                "steamemu_runtime_files runtime_root=$runtime_root steam_dir=$steamemu_dir bundled_steamclient=$steamclient_ready bundled_appid=$appid_ready stale_root_steamclient_removed=$stale_root_steamclient_removed",
            )
            && script.contains("kill_steamemu_ipcserver_for_dir \"$steamemu_macos_dir\"")
            && script.contains("pause_real_steam_for_steamemu")
            && script.contains("env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES \"$steamemu_macos_dir/ipcserver\"")
            // Use local ipcserver, but pause global Steam ipctool while wrapper is running.
            && script.contains("steamemu_launchctl_untouched using_local_ipcserver=1")
            && script.contains("pause_steam_ipctool_for_steamemu")
            && script.contains("steamemu_launchctl_removed label=$steamemu_ipctool_label uid=$steamemu_ipctool_uid")
            && script.contains("steamemu_launchctl_not_present label=$steamemu_ipctool_label uid=$steamemu_ipctool_uid")
            // ipcserver readiness poll before game launch
            && script.contains("steamemu_ipcserver_failed_to_start")
            && script.contains("ipc_ready=$_ipc_ready")
            && script.contains("exit_status=$ipc_exit_status");
    let has_vanilla_exec_arch_branching = (script
        .contains("if [ \"$vanilla_exec_arch\" = \"arm64\" ]")
        && script.contains("elif [ \"$vanilla_exec_arch\" = \"x64\" ]"))
        || (script.contains("if [ \"$vanilla_exec_arch\" = \"x64\" ]")
            && script.contains("elif [ \"$vanilla_exec_arch\" = \"arm64\" ]"));
    let has_vanilla_steamemu_direct_launch =
        script.contains("exec_vanilla_steamemu_direct")
            && script.contains("vanilla_steamemu_runtime_prepare")
            && script.contains("vanilla_working_dir=\"$BASEDIR\"")
            && script.contains("if [ \"$launch_entry_uses_wrapper\" = \"1\" ] && [ -x \"$steamemu_macos_dir/ipcserver\" ]")
            && script.contains("env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES \"$steamemu_macos_dir/reset\"")
            && script.contains("exec_vanilla_steamemu_direct_done status=$exec_status")
            && script.contains("cwd=$vanilla_working_dir steamemu_dir=$steamemu_macos_dir")
            && script.contains("cd \"$vanilla_working_dir\" || exit 1")
            && script.contains("selected_vanilla_exec_arch=$vanilla_exec_arch")
            && has_vanilla_exec_arch_branching
            && script.contains("/usr/bin/arch -x86_64 \"${executable_path}\"")
            && script.contains("/usr/bin/arch -arm64 \"${executable_path}\"")
            && script.contains("vanilla_exec_name=$(basename \"${executable_path}\")")
            && script.contains("vanilla_exec_dir=$(dirname \"${executable_path}\")")
            && script.contains("vanilla_alive=0")
            && script.contains("vanilla_runtime_pid=$vanilla_runtime_pid flags=$vanilla_runtime_flags translated=$vanilla_runtime_translated")
            && script.contains(
                "exec_vanilla_arm64_detached_running status=$vanilla_arm_status keep_arm64=true",
            )
            && script.contains(
                "exec_vanilla_arm64_exec_error status=$vanilla_arm_status fallback_to_x64=true",
            )
            && script.contains(
                "exec_vanilla_arm64_killed status=$vanilla_arm_status fallback_to_x64=true",
            )
            && script.contains(
                "exec_vanilla_arm64_early_exit status=$vanilla_arm_status keep_arm64=true fallback_to_x64=false",
            );
    let has_steamemu_runtime_cleanup =
        script.contains("cleanup_steamemu_runtime()")
            && script.contains("kill_steamemu_ipcserver_for_dir()")
            && script.contains("pause_real_steam_for_steamemu()")
            && script.contains("pause_steam_ipctool_for_steamemu()")
            && script.contains("steamemu_ipctool_paused=0")
            && script.contains("steamemu_ipctool_label=\"com.valvesoftware.steam.ipctool\"")
            && script.contains("steamemu_ipctool_uid=$(id -u 2>/dev/null || printf \"\")")
            && script.contains("steamemu_ipctool_plist=\"$HOME/Library/Application Support/Steam/com.valvesoftware.steam.ipctool.plist\"")
            && script.contains("restore_real_steam_after_steamemu()")
            && script.contains("restore_steam_ipctool_after_steamemu()")
            && script.contains("is_real_steam_running()")
            && script.contains("trap cleanup_steamemu_runtime EXIT INT TERM")
            && script.contains("steamemu_runtime_cleanup ipcpid=$steamemu_ipc_pid")
            && script.contains("steamemu_real_steam_pause_requested")
            && script.contains("steamemu_real_steam_restore_requested")
            && script.contains("steamemu_real_steam_pause_failed still_running=1")
            && script.contains("steamemu_launchctl_restore_ok plist=$steamemu_ipctool_plist")
            && script.contains("steamemu_launchctl_restore_failed plist=$steamemu_ipctool_plist")
            && script.contains("steamemu_launchctl_restore_skipped_missing_plist plist=$steamemu_ipctool_plist")
            && script.contains("lsof -t \"$ipcserver_path\"")
            && script.contains("pgrep -f \"$ipcserver_path\"");
    let preserves_steam_dyld_hooks = script
        .contains("r2modmac: preserve Steam-provided DYLD hooks")
        && script.contains("DYLD_INSERT_LIBRARIES=\"${doorstop_dylib}:${DYLD_INSERT_LIBRARIES}\"");
    let steam_launch_exec_deferred = script
        .contains("steam_launch_args_ready source=bootstrap_relay")
        && script.contains("steam_launch_args_ready source=separator")
        && script.contains("steam_launch_exec_modded argv=$*");
    let has_legacy_bepinex_bootstrap_log = script
        .contains("bootstrap_log=\"$BASEDIR/BepInEx/r2modmac_bootstrap.log\"")
        || script.contains("mkdir -p \"$BASEDIR/BepInEx\"");
    let steam_launch_pos = script.find("for a in \"$@\"").unwrap_or(usize::MAX);
    let doorstop_export_pos = script
        .find("DOORSTOP_INVOKE_DLL_PATH=")
        .unwrap_or(usize::MAX);
    let steam_launch_order_ok = has_steam_arg_helper && doorstop_export_pos < steam_launch_pos;
    let has_unexpanded_template_braces =
        script.contains("${{") || script.contains("() {{") || script.contains("{{");
    let needs_regeneration = !has_macos_doorstop_support(&script)
        || !early_exit_before_doorstop_validation
        || !early_exit_before_dyld
        || !has_root_doorstop_fallback
        || !steam_launch_order_ok
        || !has_root_bootstrap_log
        || !removes_codesign_signature
        || !has_codesign_cache_guard
        || !logs_loader_environment
        || !has_root_loader_mode_env
        || !has_arch_env_exec
        || !has_dyld_loader_logging
        || !has_exec_failure_logging
        || !has_native_arm64_direct_exec
        || !has_wrapper_modded_exec_support
        || !has_arm64_x64_fallback_retry
        || !has_preloader_crash_arch_recovery
        || !has_persistent_x64_state
        || !has_cross_generation_doorstop_bool_flags
        || !has_launch_entry_support
        || !has_steamemu_runtime_prep
        || !has_vanilla_steamemu_direct_launch
        || !has_steamemu_runtime_cleanup
        || !preserves_steam_dyld_hooks
        || !steam_launch_exec_deferred
        || !has_expected_debug_log_setting
        || has_unexpanded_template_braces
        || has_legacy_bepinex_bootstrap_log;
    if needs_regeneration {
        eprintln!(
            "[configure_macos_bepinex_script] Regenerating script (has_doorstop={} early_exit_before_doorstop_ok={} early_exit_before_dyld_ok={} root_fallback_ok={} steam_launch_order_ok={} root_bootstrap_log_ok={} removes_codesign_signature={} has_codesign_cache_guard={} logs_loader_environment={} has_arch_env_exec={} has_dyld_loader_logging={} has_exec_failure_logging={} wrapper_modded_exec_support={} arm64_x64_fallback_retry={} preloader_crash_arch_recovery={} persistent_x64_state={} cross_generation_doorstop_bool_flags={} launch_entry_support={} steamemu_runtime_prep={} vanilla_steamemu_direct_launch={} steamemu_runtime_cleanup={} preserves_steam_dyld_hooks={} steam_launch_exec_deferred={} legacy_bepinex_bootstrap_log={}).",
            has_macos_doorstop_support(&script),
            early_exit_before_doorstop_validation,
            early_exit_before_dyld,
            has_root_doorstop_fallback,
            steam_launch_order_ok,
            has_root_bootstrap_log,
            removes_codesign_signature,
            has_codesign_cache_guard,
            logs_loader_environment,
            has_arch_env_exec,
            has_dyld_loader_logging,
            has_exec_failure_logging,
            has_wrapper_modded_exec_support,
            has_arm64_x64_fallback_retry,
            has_preloader_crash_arch_recovery,
            has_persistent_x64_state,
            has_cross_generation_doorstop_bool_flags,
            has_launch_entry_support,
            has_steamemu_runtime_prep,
            has_vanilla_steamemu_direct_launch,
            has_steamemu_runtime_cleanup,
            preserves_steam_dyld_hooks,
            steam_launch_exec_deferred,
            has_legacy_bepinex_bootstrap_log
        );
        script = build_generated_macos_bepinex_script(
            &relative_executable,
            &relative_launch_entry,
            launch_entry_uses_wrapper,
            write_debug_logs_to_game,
        );
    } else {
        if let Ok(executable_re) = regex::Regex::new(r#"(?m)^executable_name=".*"$"#) {
            script = executable_re
                .replace(
                    &script,
                    format!("executable_name=\"{}\"", relative_executable),
                )
                .into_owned();
        }
        if let Ok(launch_entry_re) = regex::Regex::new(r#"(?m)^launch_entry_name=".*"$"#) {
            script = launch_entry_re
                .replace(
                    &script,
                    format!("launch_entry_name=\"{}\"", relative_launch_entry),
                )
                .into_owned();
        }
        if let Ok(wrapper_flag_re) = regex::Regex::new(r#"(?m)^launch_entry_uses_wrapper=.*$"#) {
            script = wrapper_flag_re
                .replace(
                    &script,
                    format!(
                        "launch_entry_uses_wrapper={}",
                        if launch_entry_uses_wrapper { 1 } else { 0 }
                    ),
                )
                .into_owned();
        }
    }

    if let Some(idx) = script.find("BASEDIR=") {
        let insert_after = script[idx..]
            .find('\n')
            .map(|offset| idx + offset + 1)
            .unwrap_or(script.len());

        if !script.contains("cd \"$BASEDIR\"") {
            script.insert_str(
                insert_after,
                "cd \"$BASEDIR\" # r2modmac: run from game directory for macOS compatibility\n",
            );
        }
    }

    if !script.contains("r2modmac: if the runtime is marked disabled") {
        let runtime_disabled_block = "\n# r2modmac: if the runtime is marked disabled, launch the game without Doorstop.\nruntime_disabled=false\nif [ -e \"$BASEDIR/BepInEx_DISABLED\" ] || [ -e \"$BASEDIR/doorstop_libs_DISABLED\" ] || [ -e \"$BASEDIR/libdoorstop.dylib_DISABLED\" ] || [ -e \"$BASEDIR/doorstop_config.ini_DISABLED\" ]; then\n    runtime_disabled=true\nfi\n\nif [ \"$runtime_disabled\" = true ]; then\n    exec \"${executable_path}\"\nfi\n";
        if let Some(idx) = script.find("BASEDIR=") {
            let insert_at = script[idx..]
                .find('\n')
                .map(|offset| idx + offset + 1)
                .unwrap_or(script.len());
            script.insert_str(insert_at, runtime_disabled_block);
        } else {
            script.push_str(runtime_disabled_block);
        }
    }

    script = script.replace(
        r#"if ! echo "$real_executable_name" | grep "^.*\.app/Contents/MacOS/.*";"#,
        r#"if ! echo "$real_executable_name" | grep "^.*/Contents/MacOS/.*";"#,
    );

    if !script.contains("com.apple.quarantine") {
        let dequarantine_block = "\n# r2modmac: best-effort de-quarantine for Doorstop/BepInEx payloads\nif command -v xattr >/dev/null 2>&1; then\n  /usr/bin/xattr -d com.apple.quarantine \"$BASEDIR/run_bepinex.sh\" \"$BASEDIR/doorstop_libs\" \"$BASEDIR/BepInEx\" \"$BASEDIR\"/*.dylib 2>/dev/null || true\nfi\n";
        if let Some(idx) = script.find("BASEDIR=") {
            let insert_at = script[idx..]
                .find('\n')
                .map(|offset| idx + offset + 1)
                .unwrap_or(script.len());
            script.insert_str(insert_at, dequarantine_block);
        } else {
            script.push_str(dequarantine_block);
        }
    }

    if script != original {
        fs::write(script_path, script).map_err(|e| e.to_string())?;
    }

    let legacy_bootstrap_log = game_path.join("BepInEx").join("r2modmac_bootstrap.log");
    if legacy_bootstrap_log.exists() {
        let _ = fs::remove_file(&legacy_bootstrap_log);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(script_path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(script_path, perms).map_err(|e| e.to_string())?;
    }

    Ok(())
}
