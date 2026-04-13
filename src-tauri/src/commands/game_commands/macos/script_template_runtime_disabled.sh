if [ "$runtime_disabled" = true ]; then
    if [ "$steam_launch_args_ready" = true ]; then
        log_bootstrap "steam_launch_exec_vanilla argv=$*"
        exec "$@"
    fi

    vanilla_working_dir="$BASEDIR"

    if [ "$launch_entry_uses_wrapper" = "1" ]; then
        steamemu_macos_dir=$(dirname "$launch_entry_path")
    else
        steamemu_macos_dir="$BASEDIR/MacOS"
    fi
    steamemu_appid_file="$steamemu_macos_dir/steam_appid.txt"

    # r2modmac: for bundled Steam-emu wrappers (GOG-style), avoid routing vanilla
    # through the `load` script because it may depend on unsupported xattr flags.
    # Prepare Steam env/ipc here and launch the real executable directly.
    if [ "$launch_entry_uses_wrapper" = "1" ] && [ -x "$steamemu_macos_dir/ipcserver" ] && [ -f "$steamemu_appid_file" ]; then
        steamemu_app_id=$(tr -d '[:space:]' < "$steamemu_appid_file" 2>/dev/null)
        if [ -n "$steamemu_app_id" ]; then
            export SteamAppId="$steamemu_app_id"
            export SteamGameId="$steamemu_app_id"
        fi
        export STEAM_PATH="$steamemu_macos_dir"
        export SteamPath="$steamemu_macos_dir"

        steamemu_config_dir="$steamemu_macos_dir/../Config"
        if [ ! -d "$steamemu_config_dir" ]; then
            steamemu_config_dir="$steamemu_macos_dir"
        fi
        export STEAMEMU_SETTINGS_DIR="$steamemu_config_dir"

        if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
            export DYLD_LIBRARY_PATH="$steamemu_macos_dir:${{DYLD_LIBRARY_PATH}}"
        else
            export DYLD_LIBRARY_PATH="$steamemu_macos_dir"
        fi
        prepare_steamemu_runtime_files "$BASEDIR" "$steamemu_macos_dir"
        log_bootstrap "vanilla_steamemu_runtime_prepare app_id=$steamemu_app_id steam_path=$steamemu_macos_dir settings_dir=${{STEAMEMU_SETTINGS_DIR:-}}"

        log_bootstrap "steamemu_launchctl_untouched using_local_ipcserver=1"
        pause_real_steam_for_steamemu
        pause_steam_ipctool_for_steamemu

        steamemu_previous_dir=$(pwd)
        if cd "$steamemu_macos_dir" 2>/dev/null; then
            kill_steamemu_ipcserver_for_dir "$steamemu_macos_dir"
            sleep 0.2
            if [ -x "$steamemu_macos_dir/reset" ]; then
                env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES "$steamemu_macos_dir/reset" >> "$reset_log" 2>&1 || log_bootstrap "steamemu_reset_failed status=$?"
                sleep 0.1
            fi
            env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES "$steamemu_macos_dir/ipcserver" >> "$ipc_log" 2>&1 &
            steamemu_ipc_pid=$!
            cd "$steamemu_previous_dir" >/dev/null 2>&1 || true
        else
            log_bootstrap "steamemu_runtime_dir_unreachable dir=$steamemu_macos_dir"
        fi

        _ipc_ready=0
        for _i in 1 2 3 4 5 6 7 8 9 10; do
            sleep 0.3
            if kill -0 "$steamemu_ipc_pid" 2>/dev/null; then
                _ipc_ready=1
                break
            fi
        done
        if [ "$_ipc_ready" = "0" ]; then
            ipc_exit_status=0
            if [ -n "$steamemu_ipc_pid" ]; then
                wait "$steamemu_ipc_pid" >/dev/null 2>&1 || ipc_exit_status=$?
            fi
            log_bootstrap "steamemu_ipcserver_failed_to_start ipcpid=$steamemu_ipc_pid exit_status=$ipc_exit_status"
            steamemu_ipc_pid=""
        fi

        _game_app_bundle=$(dirname "$(dirname "$executable_path")")
        if [ -d "$_game_app_bundle" ]; then
            if ! /usr/bin/xattr -cr "$_game_app_bundle" >/dev/null 2>&1; then
                /usr/bin/xattr -c "$_game_app_bundle" >/dev/null 2>&1 || true
            fi
        fi

        log_bootstrap "exec_vanilla_steamemu_direct target=$executable_path cwd=$vanilla_working_dir steamemu_dir=$steamemu_macos_dir arch=$vanilla_exec_arch ipc_ready=$_ipc_ready app_id=$steamemu_app_id settings_dir=${{STEAMEMU_SETTINGS_DIR:-}}"
        cd "$vanilla_working_dir" || exit 1
        if [ "$vanilla_exec_arch" = "arm64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
            /usr/bin/arch -arm64 "${{executable_path}}" >> "$exec_log" 2>&1 &
            vanilla_arm_pid=$!
            sleep 4
            if kill -0 "$vanilla_arm_pid" 2>/dev/null; then
                wait "$vanilla_arm_pid" >/dev/null 2>&1
                exec_status=$?
            else
                wait "$vanilla_arm_pid" >/dev/null 2>&1
                vanilla_arm_status=$?
                vanilla_exec_name=$(basename "${{executable_path}}")
                vanilla_exec_dir=$(dirname "${{executable_path}}")
                vanilla_alive=0
                if pgrep -x "$vanilla_exec_name" >/dev/null 2>&1 \
                    || pgrep -f "${{executable_path}}" >/dev/null 2>&1 \
                    || pgrep -f "$vanilla_exec_dir" >/dev/null 2>&1; then
                    vanilla_alive=1
                fi

                # Some Unity/Steam-emu launch paths can detach/re-parent quickly.
                # If a live game process is already present, keep ARM and do not force x64 fallback.
                if [ "$vanilla_alive" = "1" ]; then
                    vanilla_runtime_pid=$(pgrep -n -x "$vanilla_exec_name" 2>/dev/null || true)
                    if [ -z "$vanilla_runtime_pid" ]; then
                        vanilla_runtime_pid=$(pgrep -n -f "${{executable_path}}" 2>/dev/null || true)
                    fi
                    vanilla_runtime_flags=""
                    vanilla_runtime_translated=0
                    if [ -n "$vanilla_runtime_pid" ]; then
                        vanilla_runtime_flags=$(ps -o flags= -p "$vanilla_runtime_pid" 2>/dev/null | tr -d '[:space:]')
                        if printf '%s' "$vanilla_runtime_flags" | grep -Eq '^[0-9A-Fa-f]+$'; then
                            if [ $((16#$vanilla_runtime_flags & 0x20000)) -ne 0 ]; then
                                vanilla_runtime_translated=1
                            fi
                        fi
                    fi
                    log_bootstrap "vanilla_runtime_pid=$vanilla_runtime_pid flags=$vanilla_runtime_flags translated=$vanilla_runtime_translated"
                    log_bootstrap "exec_vanilla_arm64_detached_running status=$vanilla_arm_status keep_arm64=true"
                    printf '[%s] exec_vanilla_arm64_detached_running status=%s keep_arm64=true\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$vanilla_arm_status" >> "$exec_log"
                    exec_status=0
                elif [ "$vanilla_arm_status" = "126" ] || [ "$vanilla_arm_status" = "127" ] || [ "$vanilla_arm_status" = "132" ]; then
                    # Fallback to Intel for hard exec errors (missing exec / bad arch / illegal instruction).
                    log_bootstrap "exec_vanilla_arm64_exec_error status=$vanilla_arm_status fallback_to_x64=true"
                    printf '[%s] exec_vanilla_arm64_exec_error status=%s fallback_to_x64=true\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$vanilla_arm_status" >> "$exec_log"
                    /usr/bin/arch -x86_64 "${{executable_path}}" >> "$exec_log" 2>&1
                    exec_status=$?
                elif [ "$vanilla_arm_status" = "137" ] || [ "$vanilla_arm_status" = "143" ]; then
                    # Observed on some GOG Steam-emu wrappers: native ARM launch is killed quickly.
                    # Keep compatibility by falling back to x86_64 for vanilla.
                    log_bootstrap "exec_vanilla_arm64_killed status=$vanilla_arm_status fallback_to_x64=true"
                    printf '[%s] exec_vanilla_arm64_killed status=%s fallback_to_x64=true\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$vanilla_arm_status" >> "$exec_log"
                    /usr/bin/arch -x86_64 "${{executable_path}}" >> "$exec_log" 2>&1
                    exec_status=$?
                else
                    # Keep native-ARM decision and avoid forcing Rosetta for transient early exits.
                    log_bootstrap "exec_vanilla_arm64_early_exit status=$vanilla_arm_status keep_arm64=true fallback_to_x64=false"
                    printf '[%s] exec_vanilla_arm64_early_exit status=%s keep_arm64=true fallback_to_x64=false\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$vanilla_arm_status" >> "$exec_log"
                    exec_status=$vanilla_arm_status
                fi
            fi
        elif [ "$vanilla_exec_arch" = "x64" ] && command -v /usr/bin/arch >/dev/null 2>&1; then
            /usr/bin/arch -x86_64 "${{executable_path}}" >> "$exec_log" 2>&1
            exec_status=$?
        else
            "${{executable_path}}" >> "$exec_log" 2>&1
            exec_status=$?
        fi
        log_bootstrap "exec_vanilla_steamemu_direct_done status=$exec_status"
        printf '[%s] exec_vanilla_steamemu_direct_done status=%s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$exec_status" >> "$exec_log"
        exit "$exec_status"
    fi

    if [ "$launch_entry_uses_wrapper" = "1" ]; then
        log_bootstrap "exec_vanilla_wrapper=$launch_entry_path"
        exec /bin/bash "${{launch_entry_path}}"
    fi
    log_bootstrap "exec_vanilla=$executable_path"
    cd "$vanilla_working_dir" || exit 1
    exec "${{executable_path}}"
fi

if [ ! -f "$doorstop_dylib" ]; then
    log_bootstrap "doorstop_dylib_missing"
    echo "Cannot find Doorstop library: $doorstop_dylib"
    exit 1
fi

if [ "$root_loader_mode" = true ]; then
    export LD_LIBRARY_PATH="$BASEDIR:${{LD_LIBRARY_PATH}}"
    if [ -z "${{LD_PRELOAD:-}}" ]; then
        export LD_PRELOAD="libdoorstop.dylib"
    else
        export LD_PRELOAD="libdoorstop.dylib:${{LD_PRELOAD}}"
    fi

    if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
        export DYLD_LIBRARY_PATH="$BASEDIR:${{DYLD_LIBRARY_PATH}}"
    else
        export DYLD_LIBRARY_PATH="$BASEDIR"
    fi

    if [ -n "${{DYLD_INSERT_LIBRARIES:-}}" ]; then
        export DYLD_INSERT_LIBRARIES="libdoorstop.dylib:${{DYLD_INSERT_LIBRARIES}}"
    else
        export DYLD_INSERT_LIBRARIES="libdoorstop.dylib"
    fi
else
    export LD_LIBRARY_PATH="${{doorstop_libs}}:${{LD_LIBRARY_PATH}}"
    export LD_PRELOAD="${{doorstop_dylib}}:${{LD_PRELOAD}}"

    # r2modmac: preserve Steam-provided DYLD hooks so the Steam Overlay keeps working.
    if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
        export DYLD_LIBRARY_PATH="${{doorstop_libs}}:${{DYLD_LIBRARY_PATH}}"
    else
        export DYLD_LIBRARY_PATH="${{doorstop_libs}}"
    fi

    if [ -n "${{DYLD_INSERT_LIBRARIES:-}}" ]; then
        export DYLD_INSERT_LIBRARIES="${{doorstop_dylib}}:${{DYLD_INSERT_LIBRARIES}}"
    else
        export DYLD_INSERT_LIBRARIES="${{doorstop_dylib}}"
    fi
fi

if [ "$write_debug_logs" = "1" ]; then
    export DYLD_PRINT_LIBRARIES=1
    export DYLD_PRINT_TO_FILE="$dyld_log"
fi

log_bootstrap "loader_env LD_LIBRARY_PATH=${{LD_LIBRARY_PATH:-}} LD_PRELOAD=${{LD_PRELOAD:-}} DYLD_LIBRARY_PATH=${{DYLD_LIBRARY_PATH:-}} DYLD_INSERT_LIBRARIES=${{DYLD_INSERT_LIBRARIES:-}} DYLD_PRINT_TO_FILE=${{DYLD_PRINT_TO_FILE:-}}"

# r2modmac: prepare Steam runtime emulation wrappers used by some manual macOS builds
# (for example launchers that ship steam_appid.txt + ipcserver in Contents/MacOS).
if [ "$launch_entry_uses_wrapper" = "1" ]; then
    steamemu_macos_dir=$(dirname "$launch_entry_path")
else
    steamemu_macos_dir="$BASEDIR/MacOS"
fi
modded_working_dir="$BASEDIR"
steamemu_appid_file="$steamemu_macos_dir/steam_appid.txt"
if [ -x "$steamemu_macos_dir/ipcserver" ] && [ -f "$steamemu_appid_file" ]; then
    steamemu_app_id=$(tr -d '[:space:]' < "$steamemu_appid_file" 2>/dev/null)
    if [ -n "$steamemu_app_id" ]; then
        export SteamAppId="$steamemu_app_id"
        export SteamGameId="$steamemu_app_id"
    fi

    # r2modmac: export STEAM_PATH / SteamPath so the bundled ipcserver can self-locate
    # steamclient.dylib without needing a real Steam installation (GOG + Steam-emu bundles).
    export STEAM_PATH="$steamemu_macos_dir"
    export SteamPath="$steamemu_macos_dir"

    steamemu_config_dir="$steamemu_macos_dir/../Config"
    if [ ! -d "$steamemu_config_dir" ]; then
        steamemu_config_dir="$steamemu_macos_dir"
    fi
    export STEAMEMU_SETTINGS_DIR="$steamemu_config_dir"

    if [ -n "${{DYLD_LIBRARY_PATH:-}}" ]; then
        export DYLD_LIBRARY_PATH="$steamemu_macos_dir:${{DYLD_LIBRARY_PATH}}"
    else
        export DYLD_LIBRARY_PATH="$steamemu_macos_dir"
    fi
    prepare_steamemu_runtime_files "$BASEDIR" "$steamemu_macos_dir"

    log_bootstrap "steamemu_launchctl_untouched using_local_ipcserver=1"
    pause_real_steam_for_steamemu
    pause_steam_ipctool_for_steamemu

    steamemu_previous_dir=$(pwd)
    if cd "$steamemu_macos_dir" 2>/dev/null; then
        kill_steamemu_ipcserver_for_dir "$steamemu_macos_dir"
        sleep 0.2
        if [ -x "$steamemu_macos_dir/reset" ]; then
            env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES "$steamemu_macos_dir/reset" >> "$reset_log" 2>&1 || log_bootstrap "steamemu_reset_failed status=$?"
            sleep 0.1
        fi
        env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES "$steamemu_macos_dir/ipcserver" >> "$ipc_log" 2>&1 &
        steamemu_ipc_pid=$!
        cd "$steamemu_previous_dir" >/dev/null 2>&1 || true
    else
        log_bootstrap "steamemu_runtime_dir_unreachable dir=$steamemu_macos_dir"
    fi

    # r2modmac: wait for ipcserver to register its IPC socket before launching.
    # Without this delay SteamAPI_Init() fails because the socket is not ready yet.
    _ipc_ready=0
    for _i in 1 2 3 4 5 6 7 8 9 10; do
        sleep 0.3
        if kill -0 "$steamemu_ipc_pid" 2>/dev/null; then
            _ipc_ready=1
            break
        fi
    done
    if [ "$_ipc_ready" = "0" ]; then
        ipc_exit_status=0
        if [ -n "$steamemu_ipc_pid" ]; then
            wait "$steamemu_ipc_pid" >/dev/null 2>&1 || ipc_exit_status=$?
        fi
        log_bootstrap "steamemu_ipcserver_failed_to_start ipcpid=$steamemu_ipc_pid exit_status=$ipc_exit_status"
        steamemu_ipc_pid=""
    fi

    # Remove quarantine from the inner game bundle (mirrors what the load script does via xattr -cr).
    _game_app_bundle=$(dirname "$(dirname "$executable_path")")
    if [ -d "$_game_app_bundle" ]; then
        if ! /usr/bin/xattr -cr "$_game_app_bundle" >/dev/null 2>&1; then
            /usr/bin/xattr -c "$_game_app_bundle" >/dev/null 2>&1 || true
        fi
    fi

    log_bootstrap "steamemu_runtime_prepared app_id=$steamemu_app_id ipcpid=$steamemu_ipc_pid ipc_ready=$_ipc_ready steam_path=$steamemu_macos_dir settings_dir=${{STEAMEMU_SETTINGS_DIR:-}}"
fi

# r2modmac: resolve the actual modded launch target AFTER steamemu_macos_dir is defined.
# For GOG/Steam-emu wrappers (load + ipcserver + steamclient.dylib), bypass the wrapper
# script: inject Doorstop directly into the real executable so our ipcserver (started
# above) stays alive with the correct env instead of being killed by the load script.
modded_target_path="$executable_path"
modded_target_is_wrapper=false
if [ "$launch_entry_uses_wrapper" = "1" ]; then
    if [ -x "$steamemu_macos_dir/ipcserver" ] && [ -f "$steamemu_macos_dir/steam_appid.txt" ]; then
        log_bootstrap "gog_steamemu_wrapper_bypass target=$executable_path cwd=$modded_working_dir steamemu_dir=$steamemu_macos_dir ipcserver=$steamemu_macos_dir/ipcserver"
        modded_target_path="$executable_path"
        modded_target_is_wrapper=false
    else
        modded_target_path="$launch_entry_path"
        modded_target_is_wrapper=true
    fi
fi
