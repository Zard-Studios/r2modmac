#!/bin/sh
# r2modmac generated macOS BepInEx launcher
executable_name="__RELATIVE_EXEC__"
launch_entry_name="__RELATIVE_LAUNCH_ENTRY__"
launch_entry_uses_wrapper=__LAUNCH_ENTRY_USES_WRAPPER__
write_debug_logs=__WRITE_DEBUG_LOGS__

a="/$0"; a=${{a%/*}}; a=${{a#/}}; a=${{a:-.}}; BASEDIR=$(cd "$a"; pwd -P)
cd "$BASEDIR"

if [ "$write_debug_logs" = "1" ]; then
    bootstrap_log="$BASEDIR/r2modmac_bootstrap.log"
    if [ -z "${{R2MODMAC_BOOTSTRAP_LOG_READY:-}}" ]; then
        touch "$bootstrap_log"
        export R2MODMAC_BOOTSTRAP_LOG_READY=1
    fi

    dyld_log="$BASEDIR/r2modmac_dyld.log"
    if [ -z "${{R2MODMAC_DYLD_LOG_READY:-}}" ]; then
        touch "$dyld_log"
        export R2MODMAC_DYLD_LOG_READY=1
    fi

    exec_log="$BASEDIR/r2modmac_exec.log"
    if [ -z "${{R2MODMAC_EXEC_LOG_READY:-}}" ]; then
        touch "$exec_log"
        export R2MODMAC_EXEC_LOG_READY=1
    fi

    ipc_log="$BASEDIR/r2modmac_ipcserver.log"
    if [ -z "${{R2MODMAC_IPC_LOG_READY:-}}" ]; then
        touch "$ipc_log"
        export R2MODMAC_IPC_LOG_READY=1
    fi

    reset_log="$BASEDIR/r2modmac_reset.log"
    if [ -z "${{R2MODMAC_RESET_LOG_READY:-}}" ]; then
        touch "$reset_log"
        export R2MODMAC_RESET_LOG_READY=1
    fi

    printf '\n[%s] ---- r2modmac session pid=%s ----\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$$" >> "$bootstrap_log"

    log_bootstrap() {{
        printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >> "$bootstrap_log"
    }}
else
    bootstrap_log="/dev/null"
    dyld_log="/dev/null"
    exec_log="/dev/null"
    ipc_log="/dev/null"
    reset_log="/dev/null"
    log_bootstrap() {{
        :
    }}
fi

log_bootstrap "wrapper_start pid=$$ ppid=$PPID argv=$*"
wrapper_arch=$(/usr/bin/arch 2>/dev/null || printf unknown)
wrapper_translated=$(/usr/sbin/sysctl -in sysctl.proc_translated 2>/dev/null || printf 0)
log_bootstrap "wrapper_arch=$wrapper_arch translated=$wrapper_translated"

# r2modmac: if the runtime is marked disabled, launch the game without Doorstop.
runtime_disabled=false
if [ -e "$BASEDIR/BepInEx_DISABLED" ] || [ -e "$BASEDIR/doorstop_libs_DISABLED" ] || [ -e "$BASEDIR/libdoorstop.dylib_DISABLED" ] || [ -e "$BASEDIR/doorstop_config.ini_DISABLED" ]; then
    runtime_disabled=true
    log_bootstrap "runtime_disabled=true"
else
    log_bootstrap "runtime_disabled=false"
fi

if command -v xattr >/dev/null 2>&1; then
  /usr/bin/xattr -d com.apple.quarantine "$BASEDIR/run_bepinex.sh" "$BASEDIR/doorstop_libs" "$BASEDIR/BepInEx" "$BASEDIR"/*.dylib 2>/dev/null || true
fi

# UnityDoorstop bool parsing is strict on some builds; use numeric flags.
export DOORSTOP_ENABLE=1
export DOORSTOP_ENABLED=1
export DOORSTOP_INVOKE_DLL_PATH="$BASEDIR/BepInEx/core/BepInEx.Preloader.dll"
export DOORSTOP_TARGET_ASSEMBLY="$DOORSTOP_INVOKE_DLL_PATH"
export DOORSTOP_BOOT_CONFIG_OVERRIDE=""
export DOORSTOP_IGNORE_DISABLED_ENV=0
export DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE="$BASEDIR/BepInEx/core"
export DOORSTOP_MONO_DEBUG_ENABLED=0
export DOORSTOP_MONO_DEBUG_START_SERVER=0
export DOORSTOP_MONO_DEBUG_ADDRESS="127.0.0.1:10000"
export DOORSTOP_MONO_DEBUG_SUSPEND=0
export DOORSTOP_CLR_RUNTIME_CORECLR_PATH=""
export DOORSTOP_CLR_CORLIB_DIR=""
export DOORSTOP_CORLIB_OVERRIDE_PATH=""
export DOORSTOP_REDIRECT_OUTPUT_LOG=1

steam_arg_helper() {{
    if [ "$executable_name" != "" ] && [ "$1" != "${{1%"$executable_name"}}" ]; then
        return 0
    elif [ "$executable_name" = "" ] && [ "$1" != "${{1%.x86_64}}" ]; then
        return 0
    elif [ "$executable_name" = "" ] && [ "$1" != "${{1%.x86}}" ]; then
        return 0
    else
        return 1
    fi
}}

steam_launch_args_ready=false
steam_launch_seen=false
steam_launch_args_list=""
for a in "$@"; do
    steam_launch_args_list="${{steam_launch_args_list}}
${{a}}"
    if [ "$a" = "SteamLaunch" ]; then
        steam_launch_seen=true
    fi
done

if [ "$steam_launch_seen" = true ]; then
    log_bootstrap "steam_launch_branch_entered"
    # Upstream BepInEx behavior for Steam bootstrapper relaunch:
    # preserve both old and new Steam layouts where `--` can be interleaved.
    if [ "$2" = "SteamLaunch" ]; then
        to_rotate=4
        rotated=0
        while [ $((to_rotate-=1)) -ge 0 ]; do
            while [ "z$1" = "z--" ]; do
                set -- "$@" "$1"
                shift
                rotated=$((rotated+1))
            done
            set -- "$@" "$1"
            shift
            rotated=$((rotated+1))
        done
        to_rotate=$(($# - rotated))
        set -- "$@" "$0"
        while [ $((to_rotate-=1)) -ge 0 ]; do
            set -- "$@" "$1"
            shift
        done
        steam_launch_args_ready=true
        log_bootstrap "steam_launch_args_ready source=bootstrap_relay argv=$*"
        # Keep Steam bootstrap chain intact (overlay + SteamAPI context), then
        # continue in the relaunched script process.
        exec "$@"
    fi

    if [ "$steam_launch_args_ready" != true ]; then
        steam_launch_after_separator=false
        steam_launch_exec_args=""
        old_ifs=$IFS
        IFS='
'
        for a in $steam_launch_args_list; do
            if [ "$steam_launch_after_separator" = true ]; then
                steam_launch_exec_args="${{steam_launch_exec_args}}
${{a}}"
            elif [ "$a" = "--" ]; then
                steam_launch_after_separator=true
            fi
        done
        IFS=$old_ifs

        if [ -n "$steam_launch_exec_args" ]; then
            set --
            old_ifs=$IFS
            IFS='
'
            for a in $steam_launch_exec_args; do
                set -- "$@" "$a"
            done
            IFS=$old_ifs
            steam_launch_args_ready=true
            log_bootstrap "steam_launch_args_ready source=separator argv=$*"
        fi
    fi

    if [ "$steam_launch_args_ready" != true ]; then
        steam_launch_collect=false
        steam_launch_exec_args=""
        old_ifs=$IFS
        IFS='
'
        for a in $steam_launch_args_list; do
            if [ "$steam_launch_collect" = true ]; then
                steam_launch_exec_args="${{steam_launch_exec_args}}
${{a}}"
            elif steam_arg_helper "$a"; then
                steam_launch_collect=true
                steam_launch_exec_args="${{steam_launch_exec_args}}
${{a}}"
            fi
        done
        IFS=$old_ifs

        if [ -n "$steam_launch_exec_args" ]; then
            set --
            old_ifs=$IFS
            IFS='
'
            for a in $steam_launch_exec_args; do
                set -- "$@" "$a"
            done
            IFS=$old_ifs
            steam_launch_args_ready=true
            log_bootstrap "steam_launch_args_ready source=executable_match argv=$*"
        fi
    fi

    if [ "$steam_launch_args_ready" != true ]; then
        log_bootstrap "steam_launch_branch_failed_to_match_executable"
        echo "Please set executable_name to a valid name in a text editor"
        exit 1
    fi
fi

case "$executable_name" in
    *.app|/*.app)
        real_executable_name="$executable_name"
        case "$real_executable_name" in
            /*) ;;
            *) real_executable_name="$BASEDIR/$real_executable_name" ;;
        esac
        inner_executable_name=$(defaults read "${{real_executable_name}}/Contents/Info" CFBundleExecutable 2>/dev/null || defaults read "${{real_executable_name}}/Contents/Info.plist" CFBundleExecutable 2>/dev/null)
        executable_path="${{real_executable_name}}/Contents/MacOS/${{inner_executable_name}}"
        ;;
    *.app/Contents/MacOS/*|/*.app/Contents/MacOS/*)
        case "$executable_name" in
            /*) executable_path="$executable_name" ;;
            *) executable_path="$BASEDIR/$executable_name" ;;
        esac
        ;;
    /*) executable_path="$executable_name" ;;
    *) executable_path="$BASEDIR/$executable_name" ;;
esac

case "$launch_entry_name" in
    /*) launch_entry_path="$launch_entry_name" ;;
    *) launch_entry_path="$BASEDIR/$launch_entry_name" ;;
esac

if [ -n "$1" ]; then
    case "$1" in
        *.app)
            real_executable_name=$(defaults read "$1/Contents/Info" CFBundleExecutable)
            executable_path="$1/Contents/MacOS/${{real_executable_name}}"
            ;;
        *.app/Contents/MacOS/*)
            executable_path="$1"
            ;;
    esac
fi

abs_path() {{
    echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
}}

_readlink() {{
    ab_path="$(abs_path "$1")"
    link="$(readlink "${{ab_path}}")"
    case $link in
        /*) ;;
        *) link="$(dirname "$ab_path")/$link" ;;
    esac
    echo "$link"
}}

resolve_executable_path() {{
    e_path="$(abs_path "$1")"
    while [ -L "${{e_path}}" ]; do
        e_path=$(_readlink "${{e_path}}")
    done
    echo "${{e_path}}"
}}

executable_path=$(resolve_executable_path "${{executable_path}}")
launch_entry_path=$(resolve_executable_path "${{launch_entry_path}}")
log_bootstrap "resolved_executable_path=$executable_path"
log_bootstrap "resolved_launch_entry_path=$launch_entry_path wrapper=$launch_entry_uses_wrapper"

steamemu_ipc_pid=""
steamemu_paused_real_steam=0
steamemu_ipctool_paused=0
steamemu_ipctool_label="com.valvesoftware.steam.ipctool"
steamemu_ipctool_uid=$(id -u 2>/dev/null || printf "")
steamemu_ipctool_plist="$HOME/Library/Application Support/Steam/com.valvesoftware.steam.ipctool.plist"
cleanup_steamemu_runtime() {{
    if [ -n "$steamemu_ipc_pid" ]; then
        kill "$steamemu_ipc_pid" >/dev/null 2>&1 || true
        wait "$steamemu_ipc_pid" >/dev/null 2>&1 || true
        log_bootstrap "steamemu_runtime_cleanup ipcpid=$steamemu_ipc_pid"
        steamemu_ipc_pid=""
    fi
    if [ -n "${{steamemu_macos_dir:-}}" ]; then
        kill_steamemu_ipcserver_for_dir "$steamemu_macos_dir"
    fi
    restore_steam_ipctool_after_steamemu
    restore_real_steam_after_steamemu
}}
trap cleanup_steamemu_runtime EXIT INT TERM

is_real_steam_running() {{
    pgrep -f '/Steam\\.app/Contents/MacOS/steam_osx|/Steam\\.AppBundle/Steam/Contents/MacOS/steam_osx' >/dev/null 2>&1
}}

pause_real_steam_for_steamemu() {{
    if ! is_real_steam_running; then
        return 0
    fi

    steamemu_paused_real_steam=1
    log_bootstrap "steamemu_real_steam_pause_requested"
    env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES osascript -e 'tell application "Steam" to quit' >/dev/null 2>&1 || true

    for _steam_wait in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
        if ! is_real_steam_running; then
            log_bootstrap "steamemu_real_steam_paused"
            return 0
        fi
        sleep 0.5
    done

    log_bootstrap "steamemu_real_steam_pause_failed still_running=1"
    echo "Steam is still running; cannot start bundled Steam emulator safely." >&2
    exit 1
}}

restore_real_steam_after_steamemu() {{
    if [ "$steamemu_paused_real_steam" = "1" ]; then
        steamemu_paused_real_steam=0
        env -u LD_PRELOAD -u DYLD_INSERT_LIBRARIES open -a Steam >/dev/null 2>&1 || true
        log_bootstrap "steamemu_real_steam_restore_requested"
    fi
}}

pause_steam_ipctool_for_steamemu() {{
    if ! command -v launchctl >/dev/null 2>&1; then
        log_bootstrap "steamemu_launchctl_unavailable"
        return 0
    fi
    if [ -z "$steamemu_ipctool_uid" ]; then
        log_bootstrap "steamemu_launchctl_skip_missing_uid"
        return 0
    fi

    if launchctl asuser "$steamemu_ipctool_uid" launchctl remove "$steamemu_ipctool_label" >/dev/null 2>&1; then
        steamemu_ipctool_paused=1
        log_bootstrap "steamemu_launchctl_removed label=$steamemu_ipctool_label uid=$steamemu_ipctool_uid"
        return 0
    fi
    if launchctl remove "$steamemu_ipctool_label" >/dev/null 2>&1; then
        steamemu_ipctool_paused=1
        log_bootstrap "steamemu_launchctl_removed label=$steamemu_ipctool_label uid=$steamemu_ipctool_uid"
        return 0
    fi

    log_bootstrap "steamemu_launchctl_not_present label=$steamemu_ipctool_label uid=$steamemu_ipctool_uid"
    return 0
}}

restore_steam_ipctool_after_steamemu() {{
    if [ "$steamemu_ipctool_paused" != "1" ]; then
        return 0
    fi
    steamemu_ipctool_paused=0

    if [ ! -f "$steamemu_ipctool_plist" ]; then
        log_bootstrap "steamemu_launchctl_restore_skipped_missing_plist plist=$steamemu_ipctool_plist"
        return 0
    fi

    if launchctl asuser "$steamemu_ipctool_uid" launchctl load -S Background "$steamemu_ipctool_plist" >/dev/null 2>&1; then
        log_bootstrap "steamemu_launchctl_restore_ok plist=$steamemu_ipctool_plist"
        return 0
    fi
    if launchctl load -S Background "$steamemu_ipctool_plist" >/dev/null 2>&1; then
        log_bootstrap "steamemu_launchctl_restore_ok plist=$steamemu_ipctool_plist"
        return 0
    fi

    log_bootstrap "steamemu_launchctl_restore_failed plist=$steamemu_ipctool_plist"
}}

kill_steamemu_ipcserver_for_dir() {{
    ipcserver_path="$1/ipcserver"
    if [ ! -x "$ipcserver_path" ]; then
        return 0
    fi

    if command -v lsof >/dev/null 2>&1; then
        lsof -t "$ipcserver_path" 2>/dev/null | while read -r existing_ipc_pid; do
            if [ -n "$existing_ipc_pid" ]; then
                kill "$existing_ipc_pid" >/dev/null 2>&1 && log_bootstrap "steamemu_local_ipcserver_killed pid=$existing_ipc_pid path=$ipcserver_path"
            fi
        done
    fi

    pgrep -f "$ipcserver_path" 2>/dev/null | while read -r existing_ipc_pid; do
        if [ -n "$existing_ipc_pid" ]; then
            kill "$existing_ipc_pid" >/dev/null 2>&1 && log_bootstrap "steamemu_local_ipcserver_killed pid=$existing_ipc_pid path=$ipcserver_path"
        fi
    done
}}

prepare_steamemu_runtime_files() {{
    runtime_root="$1"
    steamemu_dir="$2"
    steamclient_file="$steamemu_dir/steamclient.dylib"
    root_steamclient_link="$runtime_root/steamclient.dylib"
    steamclient_ready=0
    stale_root_steamclient_removed=0

    if [ -f "$steamclient_file" ]; then
        steamclient_ready=1
    fi

    # Older r2modmac scripts created a root symlink into Contents/MacOS. Do not
    # create or copy Steam libraries; remove only that exact managed symlink.
    if [ -L "$root_steamclient_link" ]; then
        root_steamclient_target=$(readlink "$root_steamclient_link" 2>/dev/null || true)
        if [ "$root_steamclient_target" = "../MacOS/steamclient.dylib" ] || [ "$root_steamclient_target" = "$steamclient_file" ]; then
            if rm -f "$root_steamclient_link" >/dev/null 2>&1; then
                stale_root_steamclient_removed=1
            fi
        fi
    fi

    appid_file="$steamemu_dir/steam_appid.txt"
    appid_ready=0
    if [ -f "$appid_file" ]; then
        appid_ready=1
    fi

    log_bootstrap "steamemu_runtime_files runtime_root=$runtime_root steam_dir=$steamemu_dir bundled_steamclient=$steamclient_ready bundled_appid=$appid_ready stale_root_steamclient_removed=$stale_root_steamclient_removed"
}}

app_path="${{executable_path%/Contents/MacOS*}}"
app_path_lower=$(printf "%s" "$app_path" | tr '[:upper:]' '[:lower:]')
if echo "$app_path_lower" | grep -Eq '/steam\.app(/|$)|/steam\.appbundle/steam(/|$)'; then
    log_bootstrap "codesign_remove_signature_skipped_steam app_path=$app_path"
elif [ "$runtime_disabled" = true ]; then
    log_bootstrap "codesign_remove_signature_skipped_runtime_disabled app_path=$app_path"
elif command -v codesign >/dev/null 2>&1 && [ -d "$app_path" ]; then
    if codesign -v --strict "$app_path" >/dev/null 2>&1; then
        log_bootstrap "codesign_adhoc_sign_skipped_valid app_path=$app_path"
    elif codesign -d "$app_path" >/dev/null 2>&1; then
        log_bootstrap "codesign_adhoc_sign_attempt app_path=$app_path"
        if codesign --force --deep --sign - "$app_path" >/dev/null 2>&1; then
            log_bootstrap "codesign_adhoc_sign_ok"
        else
            log_bootstrap "codesign_adhoc_sign_failed"
        fi
    else
        log_bootstrap "codesign_adhoc_sign_skipped_no_signature app_path=$app_path"
    fi
fi

executable_type=$(LD_PRELOAD="" file -b "${{executable_path}}")
log_bootstrap "executable_type=$executable_type"
if [ "$wrapper_translated" = "1" ]; then
    native_macos_arch="arm64"
else
    native_macos_arch=$(/usr/bin/uname -m 2>/dev/null || printf unknown)
fi
log_bootstrap "native_macos_arch=$native_macos_arch"

root_doorstop_dylib="$BASEDIR/libdoorstop.dylib"
root_doorstop_type=""
if [ -f "$root_doorstop_dylib" ]; then
    root_doorstop_type=$(LD_PRELOAD="" file -b "$root_doorstop_dylib")
    log_bootstrap "root_doorstop_type=$root_doorstop_type"
fi

root_loader_mode=false
if [ -f "$root_doorstop_dylib" ] && [ ! -d "$BASEDIR/doorstop_libs" ]; then
    root_loader_mode=true
    export DOORSTOP_ENABLE=1
    export DOORSTOP_ENABLED=1
    export DOORSTOP_TARGET_ASSEMBLY="$DOORSTOP_INVOKE_DLL_PATH"
    export DOORSTOP_CLR_RUNTIME_CORECLR_PATH=""
    export DOORSTOP_CLR_CORLIB_DIR=""
    export DOORSTOP_REDIRECT_OUTPUT_LOG=1
fi
log_bootstrap "doorstop_env_prepared root_loader_mode=$root_loader_mode DOORSTOP_ENABLE=$DOORSTOP_ENABLE DOORSTOP_ENABLED=$DOORSTOP_ENABLED DOORSTOP_INVOKE_DLL_PATH=$DOORSTOP_INVOKE_DLL_PATH DOORSTOP_TARGET_ASSEMBLY=$DOORSTOP_TARGET_ASSEMBLY DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE=$DOORSTOP_MONO_DLL_SEARCH_PATH_OVERRIDE DOORSTOP_REDIRECT_OUTPUT_LOG=$DOORSTOP_REDIRECT_OUTPUT_LOG"

can_retry_x64=false
if echo "$executable_type" | grep -q "x86_64" && echo "$root_doorstop_type" | grep -q "x86_64"; then
    can_retry_x64=true
fi

case $executable_type in
    *arm64*)
        if [ "$native_macos_arch" = "arm64" ] && [ -f "$root_doorstop_dylib" ] && echo "$root_doorstop_type" | grep -q "arm64"; then
            arch="arm64"
        else
            arch="x64"
        fi
        ;;
    *64-bit*)
        arch="x64"
        ;;
    *32-bit*|*i386*)
        arch="x86"
        ;;
    *)
        log_bootstrap "unsupported_executable_type=$executable_type"
        echo "Cannot identify executable type: $executable_type"
        exit 1
        ;;
esac

if [ "$launch_entry_uses_wrapper" = "1" ] && [ "$native_macos_arch" = "arm64" ]; then
    arch="x64"
    log_bootstrap "forcing_x64_for_wrapper_game launch_entry=$launch_entry_path"
fi

doorstop_libname="libdoorstop_${{arch}}.dylib"
doorstop_dylib="$BASEDIR/doorstop_libs/${{doorstop_libname}}"
doorstop_libs="$BASEDIR/doorstop_libs"

if [ "$arch" = "arm64" ] && [ -f "$root_doorstop_dylib" ]; then
    doorstop_dylib="$root_doorstop_dylib"
    doorstop_libs="$BASEDIR"
elif [ ! -f "$doorstop_dylib" ] && [ -f "$root_doorstop_dylib" ]; then
    doorstop_dylib="$root_doorstop_dylib"
    doorstop_libs="$BASEDIR"
fi

log_bootstrap "selected_runtime_arch=$arch doorstop_dylib=$doorstop_dylib"

# r2modmac: keep vanilla launch arch independent from Doorstop arch decisions.
# On Apple Silicon, prefer native arm64 for vanilla when the game binary supports it.
vanilla_exec_arch="$arch"
if [ "$native_macos_arch" = "arm64" ] && echo "$executable_type" | grep -q "arm64"; then
    vanilla_exec_arch="arm64"
fi
log_bootstrap "selected_vanilla_exec_arch=$vanilla_exec_arch"
