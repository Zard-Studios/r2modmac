
BEPINEX_LOG_PATH="__BEPINEX_ROOT__/LogOutput.log"
BEPINEX_LOG_MTIME_BEFORE=0
if [ -f "$BEPINEX_LOG_PATH" ]; then
    BEPINEX_LOG_MTIME_BEFORE=$(stat -f %m "$BEPINEX_LOG_PATH" 2>/dev/null || printf 0)
fi
R2MODMAC_LAUNCH_EPOCH=$(date +%s 2>/dev/null || printf 0)

maybe_retry_x64_after_arm64_failure() {{
    failed_mode="$1"
    failed_status="$2"
    shift 2
    bepinex_log="$BEPINEX_LOG_PATH"
    bepinex_log_mtime_before="${{BEPINEX_LOG_MTIME_BEFORE:-0}}"
    bepinex_log_mtime_after="$bepinex_log_mtime_before"
    if [ -f "$bepinex_log" ]; then
        bepinex_log_mtime_after=$(stat -f %m "$bepinex_log" 2>/dev/null || printf "$bepinex_log_mtime_before")
    fi
    bepinex_started=false
    if [ "$bepinex_log_mtime_after" -gt "$bepinex_log_mtime_before" ] 2>/dev/null; then
        bepinex_started=true
    fi

    if [ "$arch" != "arm64" ] || [ "$can_retry_x64" != true ]; then
        return 1
    fi

    arm64_preloader_crash=false
    latest_preloader_log=""
    retry_executable_dir=""
    if [ -n "${{executable_path:-}}" ]; then
        retry_executable_dir=$(dirname "${{executable_path}}")
    fi
    for preloader_candidate in "$BASEDIR"/preloader_*.log "$retry_executable_dir"/preloader_*.log "$modded_working_dir"/preloader_*.log; do
        [ -f "$preloader_candidate" ] || continue
        if [ -z "$latest_preloader_log" ] || [ "$preloader_candidate" -nt "$latest_preloader_log" ]; then
            latest_preloader_log="$preloader_candidate"
        fi
    done
    if [ -n "$latest_preloader_log" ] && [ -f "$latest_preloader_log" ]; then
        latest_preloader_mtime=$(stat -f %m "$latest_preloader_log" 2>/dev/null || printf 0)
        launch_epoch="${{R2MODMAC_LAUNCH_EPOCH:-0}}"
        if [ "$launch_epoch" = "0" ] || [ "$latest_preloader_mtime" -ge "$launch_epoch" ] 2>/dev/null; then
            if grep -Eq "HarmonyInteropFix\\.Apply|ConsoleSetOutFix\\.Apply|DetourHelper\\.GetIdentifiable|HarmonyException: (Patching exception|IL Compile Error)" "$latest_preloader_log"; then
                arm64_preloader_crash=true
                log_bootstrap "${{failed_mode}}_retry_preloader_crash_detected status=${{failed_status}} log=${{latest_preloader_log}}"
                if [ -n "${{force_x64_state_file:-}}" ] && [ -n "${{force_x64_state_key:-}}" ]; then
                    printf '%s\n' "$force_x64_state_key" > "$force_x64_state_file" 2>/dev/null || true
                    log_bootstrap "${{failed_mode}}_retry_force_x64_state_written key=${{force_x64_state_key}}"
                fi
            fi
        fi
    fi

    # Arm64 launch succeeded in initializing BepInEx; keep current runtime arch.
    if [ "$bepinex_started" = true ] && [ "$arm64_preloader_crash" != true ]; then
        return 1
    fi

    # User interrupted or terminated launch manually; don't auto-retry.
    if [ "$failed_status" = "130" ] || [ "$failed_status" = "143" ]; then
        return 1
    fi

    # A clean exit is not a launch failure; avoid spawning a second x64 run.
    # Some native macOS games can return 0 after handing off to child/runtime.
    if [ "$failed_status" = "0" ] && [ "$arm64_preloader_crash" != true ]; then
        log_bootstrap "${{failed_mode}}_retry_skipped_clean_exit status=${{failed_status}}"
        return 1
    elif [ "$failed_status" = "0" ] && [ "$arm64_preloader_crash" = true ]; then
        log_bootstrap "${{failed_mode}}_retrying_x64_after_preloader_crash status=${{failed_status}}"
    fi

    # Give arm64 launches a short grace period: some games detach quickly while
    # BepInEx/logging initializes. If the process is alive or logs start moving,
    # keep arm64 and do not force x64 fallback.
    retry_probe=0
    while [ $retry_probe -lt 6 ]; do
        if [ -f "$bepinex_log" ]; then
            bepinex_log_mtime_after=$(stat -f %m "$bepinex_log" 2>/dev/null || printf "$bepinex_log_mtime_before")
            if [ "$bepinex_log_mtime_after" -gt "$bepinex_log_mtime_before" ] 2>/dev/null; then
                log_bootstrap "${{failed_mode}}_retry_skipped_bepinex_started status=${{failed_status}}"
                return 1
            fi
        fi

        if [ -n "${{executable_path:-}}" ]; then
            retry_exec_name=$(basename "${{executable_path}}")
            retry_exec_dir=$(dirname "${{executable_path}}")
            if pgrep -f "${{executable_path}}" >/dev/null 2>&1 \
                || pgrep -x "$retry_exec_name" >/dev/null 2>&1 \
                || pgrep -f "$retry_exec_dir" >/dev/null 2>&1; then
                log_bootstrap "${{failed_mode}}_retry_skipped_process_alive status=${{failed_status}}"
                return 1
            fi
        fi

        retry_probe=$((retry_probe+1))
        sleep 0.5
    done

    # If arm64 exits without initializing BepInEx, retry once under x64.
    log_bootstrap "${{failed_mode}}_retrying_x64_fallback status=${{failed_status}}"
    printf '[%s] %s_retrying_x64_fallback status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$failed_mode" "$failed_status" >> "$exec_log"
    /usr/bin/arch -x86_64 \
        -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
        -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
        -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
        -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
        -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
        -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
        -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
        -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
        -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
        -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
        -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
        -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
        -e SteamAppId="${{SteamAppId:-}}" \
        -e SteamGameId="${{SteamGameId:-}}" \
        -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
        -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
        -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
        -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
        -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
        -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
        -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
        "$@" >> "$exec_log" 2>&1
    retry_status=$?
    log_bootstrap "${{failed_mode}}_x64_fallback_failed status=${{retry_status}}"
    printf '[%s] %s_x64_fallback_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$failed_mode" "$retry_status" >> "$exec_log"
    exit "$retry_status"
}}

if [ "$steam_launch_args_ready" = true ]; then
    if [ "$arch" = "arm64" ] && [ "$wrapper_arch" = "arm64" ] && [ "$wrapper_translated" = "0" ]; then
        log_bootstrap "steam_launch_exec_modded_arm64_direct argv=$*"
        "$@" >> "$exec_log" 2>&1
        exec_status=$?
        log_bootstrap "steam_launch_exec_modded_arm64_direct_failed status=$exec_status"
        printf '[%s] steam_launch_exec_modded_arm64_direct_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        maybe_retry_x64_after_arm64_failure "steam_launch_exec_modded_arm64_direct" "$exec_status" "$@"
        exit "$exec_status"
    fi
    if [ "$arch" = "arm64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
        log_bootstrap "steam_launch_exec_modded_arm64_env argv=$*"
        /usr/bin/arch -arm64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            "$@" >> "$exec_log" 2>&1
        exec_status=$?
        log_bootstrap "steam_launch_exec_modded_arm64_env_failed status=$exec_status"
        printf '[%s] steam_launch_exec_modded_arm64_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        maybe_retry_x64_after_arm64_failure "steam_launch_exec_modded_arm64_env" "$exec_status" "$@"
        exit "$exec_status"
    fi
    if [ "$arch" = "x64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
        log_bootstrap "steam_launch_exec_modded_arch_env argv=$*"
        /usr/bin/arch -x86_64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            "$@" >> "$exec_log" 2>&1
        exec_status=$?
        log_bootstrap "steam_launch_exec_modded_arch_env_failed status=$exec_status"
        printf '[%s] steam_launch_exec_modded_arch_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        exit "$exec_status"
    fi
    log_bootstrap "steam_launch_exec_modded argv=$*"
    "$@" >> "$exec_log" 2>&1
    exec_status=$?
    log_bootstrap "steam_launch_exec_modded_failed status=$exec_status"
    printf '[%s] steam_launch_exec_modded_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    exit "$exec_status"
fi

if [ "$arch" = "arm64" ] && [ "$wrapper_arch" = "arm64" ] && [ "$wrapper_translated" = "0" ]; then
    log_bootstrap "exec_modded_arm64_direct target=$modded_target_path wrapper=$modded_target_is_wrapper"
    if [ "$modded_target_is_wrapper" = true ]; then
        /bin/bash "${{modded_target_path}}" >> "$exec_log" 2>&1
    else
        cd "$modded_working_dir" || exit 1
        "${{modded_target_path}}" >> "$exec_log" 2>&1
    fi
    exec_status=$?
    log_bootstrap "exec_modded_arm64_direct_failed status=$exec_status"
    printf '[%s] exec_modded_arm64_direct_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    if [ "$modded_target_is_wrapper" = true ]; then
        maybe_retry_x64_after_arm64_failure "exec_modded_arm64_direct" "$exec_status" /bin/bash "${{modded_target_path}}"
    else
        maybe_retry_x64_after_arm64_failure "exec_modded_arm64_direct" "$exec_status" "${{modded_target_path}}"
    fi
    exit "$exec_status"
fi

if [ "$arch" = "arm64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
    log_bootstrap "exec_modded_arm64_env target=$modded_target_path wrapper=$modded_target_is_wrapper"
    if [ "$modded_target_is_wrapper" = true ]; then
        /usr/bin/arch -arm64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            /bin/bash "${{modded_target_path}}" >> "$exec_log" 2>&1
    else
        cd "$modded_working_dir" || exit 1
        /usr/bin/arch -arm64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            "${{modded_target_path}}" >> "$exec_log" 2>&1
    fi
    exec_status=$?
    log_bootstrap "exec_modded_arm64_env_failed status=$exec_status"
    printf '[%s] exec_modded_arm64_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    if [ "$modded_target_is_wrapper" = true ]; then
        maybe_retry_x64_after_arm64_failure "exec_modded_arm64_env" "$exec_status" /bin/bash "${{modded_target_path}}"
    else
        maybe_retry_x64_after_arm64_failure "exec_modded_arm64_env" "$exec_status" "${{modded_target_path}}"
    fi
    exit "$exec_status"
fi

if [ "$arch" = "x64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
    log_bootstrap "exec_modded_arch_env target=$modded_target_path wrapper=$modded_target_is_wrapper"
    if [ "$modded_target_is_wrapper" = true ]; then
        /usr/bin/arch -x86_64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            /bin/bash "${{modded_target_path}}" >> "$exec_log" 2>&1
    else
        cd "$modded_working_dir" || exit 1
        /usr/bin/arch -x86_64 \
            -e DOORSTOP_ENABLE="${{DOORSTOP_ENABLE}}" \
            -e DOORSTOP_ENABLED="${{DOORSTOP_ENABLED}}" \
            -e DOORSTOP_INVOKE_DLL_PATH="${{DOORSTOP_INVOKE_DLL_PATH}}" \
            -e DOORSTOP_TARGET_ASSEMBLY="${{DOORSTOP_TARGET_ASSEMBLY}}" \
            -e DOORSTOP_BOOT_CONFIG_OVERRIDE="${{DOORSTOP_BOOT_CONFIG_OVERRIDE}}" \
            -e DOORSTOP_IGNORE_DISABLED_ENV="${{DOORSTOP_IGNORE_DISABLED_ENV}}" \
            -e DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="${{DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE}}" \
            -e DOORSTOP_MONO_DEBUG_ENABLED="${{DOORSTOP_MONO_DEBUG_ENABLED}}" \
            -e DOORSTOP_MONO_DEBUG_ADDRESS="${{DOORSTOP_MONO_DEBUG_ADDRESS}}" \
            -e DOORSTOP_MONO_DEBUG_SUSPEND="${{DOORSTOP_MONO_DEBUG_SUSPEND}}" \
            -e DOORSTOP_CORLIB_OVERRIDE_PATH="${{DOORSTOP_CORLIB_OVERRIDE_PATH}}" \
            -e DOORSTOP_REDIRECT_OUTPUT_LOG="${{DOORSTOP_REDIRECT_OUTPUT_LOG}}" \
            -e SteamAppId="${{SteamAppId:-}}" \
            -e SteamGameId="${{SteamGameId:-}}" \
            -e STEAMEMU_SETTINGS_DIR="${{STEAMEMU_SETTINGS_DIR:-}}" \
            -e LD_LIBRARY_PATH="${{LD_LIBRARY_PATH:-}}" \
            -e LD_PRELOAD="${{LD_PRELOAD:-}}" \
            -e DYLD_LIBRARY_PATH="${{DYLD_LIBRARY_PATH:-}}" \
            -e DYLD_INSERT_LIBRARIES="${{DYLD_INSERT_LIBRARIES:-}}" \
            -e DYLD_PRINT_LIBRARIES="${{DYLD_PRINT_LIBRARIES:-}}" \
            -e DYLD_PRINT_TO_FILE="${{DYLD_PRINT_TO_FILE:-}}" \
            "${{modded_target_path}}" >> "$exec_log" 2>&1
    fi
    exec_status=$?
    log_bootstrap "exec_modded_arch_env_failed status=$exec_status"
    printf '[%s] exec_modded_arch_env_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
    exit "$exec_status"
fi

log_bootstrap "exec_modded target=$modded_target_path wrapper=$modded_target_is_wrapper"
if [ "$modded_target_is_wrapper" = true ]; then
    /bin/bash "${{modded_target_path}}" >> "$exec_log" 2>&1
else
    cd "$modded_working_dir" || exit 1
    "${{modded_target_path}}" >> "$exec_log" 2>&1
fi
exec_status=$?
log_bootstrap "exec_modded_failed status=$exec_status"
printf '[%s] exec_modded_failed status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
exit "$exec_status"
