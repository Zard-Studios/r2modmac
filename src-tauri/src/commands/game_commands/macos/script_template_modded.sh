
maybe_retry_x64_after_arm64_failure() {{
    failed_mode="$1"
    failed_status="$2"
    shift 2
    bepinex_log="$BASEDIR/BepInEx/LogOutput.log"

    if [ "$failed_status" = "0" ]; then
        return 1
    fi

    if [ "$arch" != "arm64" ] || [ "$can_retry_x64" != true ]; then
        return 1
    fi

    if [ -s "$dyld_log" ] || [ -f "$bepinex_log" ]; then
        return 1
    fi

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
