#!/bin/zsh
set -u
set -o pipefail
set +x
setopt NULL_GLOB

OUT_DIR="${1:-/tmp/r2modmac-matrix-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$OUT_DIR"

GAMES=("valheim" "silksong" "rounds" "muck")
RUNTIME_FILES=("BepInEx" "doorstop_libs" "libdoorstop.dylib" "doorstop_config.ini")

RESULTS_TSV="$OUT_DIR/results.tsv"
printf "game\tmode\tresult\treason\tprocess_started\tbootstrap_updated\tbepinex_updated\tcrash_signature\texec_path\tlauncher_log\n" > "$RESULTS_TSV"

now() {
    date "+%Y-%m-%d %H:%M:%S"
}

log() {
    printf "[%s] %s\n" "$(now)" "$*"
}

game_path_for() {
    case "$1" in
        valheim) printf "/Applications/Valheim_Steam.app/Contents/game" ;;
        silksong) printf "/Volumes/Feduzi/SteamLibrary/steamapps/common/Hollow Knight Silksong" ;;
        rounds) printf "/Users/federicofeduzi/Library/Application Support/Steam/steamapps/common/ROUNDS" ;;
        muck) printf "/Users/federicofeduzi/Library/Application Support/Steam/steamapps/common/Muck" ;;
        *) printf "" ;;
    esac
}

mtime_or_zero() {
    local target="$1"
    if [ -e "$target" ]; then
        stat -f %m "$target" 2>/dev/null || printf "0"
    else
        printf "0"
    fi
}

normalize_pids() {
    awk 'NF { print $1 }' | sort -u
}

collect_pids() {
    local exec_path="$1"
    local exec_name="$2"
    {
        pgrep -f "$exec_path" 2>/dev/null || true
        pgrep -x "$exec_name" 2>/dev/null || true
    } | normalize_pids
}

diff_pids() {
    local before="$1"
    local after="$2"
    local before_file="$OUT_DIR/.before_pids.$$"
    local after_file="$OUT_DIR/.after_pids.$$"
    printf "%s\n" "$before" | normalize_pids > "$before_file"
    printf "%s\n" "$after" | normalize_pids > "$after_file"
    comm -13 "$before_file" "$after_file" || true
    rm -f "$before_file" "$after_file"
}

kill_pid_list() {
    local pid_list="$1"
    local signal="${2:-TERM}"
    if [ -z "$pid_list" ]; then
        return 0
    fi
    while IFS= read -r pid; do
        [ -n "$pid" ] || continue
        kill "-$signal" "$pid" 2>/dev/null || true
    done <<< "$pid_list"
}

parse_executable_name() {
    local run_script="$1"
    sed -n 's/^executable_name="\(.*\)"$/\1/p' "$run_script" | head -n 1
}

resolve_exec_path() {
    local game_path="$1"
    local executable_name="$2"
    local candidate=""
    case "$executable_name" in
        /*) candidate="$executable_name" ;;
        *) candidate="$game_path/$executable_name" ;;
    esac

    if [[ "$candidate" == *.app ]]; then
        local bundle_exec
        bundle_exec=$(defaults read "$candidate/Contents/Info" CFBundleExecutable 2>/dev/null || defaults read "$candidate/Contents/Info.plist" CFBundleExecutable 2>/dev/null || true)
        if [ -n "$bundle_exec" ]; then
            candidate="$candidate/Contents/MacOS/$bundle_exec"
        fi
    fi
    printf "%s\n" "$candidate"
}

capture_initial_runtime_state() {
    local game="$1"
    local game_path="$2"
    local state_file="$OUT_DIR/state_${game}.tsv"
    : > "$state_file"
    local file=""
    for file in "${RUNTIME_FILES[@]}"; do
        local has_base="0"
        local has_disabled="0"
        [ -e "$game_path/$file" ] && has_base="1"
        [ -e "$game_path/${file}_DISABLED" ] && has_disabled="1"
        printf "%s\t%s\t%s\n" "$file" "$has_base" "$has_disabled" >> "$state_file"
    done
}

set_runtime_mode() {
    local game_path="$1"
    local mode="$2"
    local file=""
    if [ "$mode" = "enabled" ]; then
        for file in "${RUNTIME_FILES[@]}"; do
            if [ ! -e "$game_path/$file" ] && [ -e "$game_path/${file}_DISABLED" ]; then
                mv "$game_path/${file}_DISABLED" "$game_path/$file"
            fi
        done
    else
        for file in "${RUNTIME_FILES[@]}"; do
            if [ -e "$game_path/$file" ] && [ ! -e "$game_path/${file}_DISABLED" ]; then
                mv "$game_path/$file" "$game_path/${file}_DISABLED"
            fi
        done
    fi
}

restore_runtime_state() {
    local game="$1"
    local game_path="$2"
    local state_file="$OUT_DIR/state_${game}.tsv"
    [ -f "$state_file" ] || return 0

    while IFS=$'\t' read -r file want_base want_disabled; do
        [ -n "$file" ] || continue
        local has_base="0"
        local has_disabled="0"
        [ -e "$game_path/$file" ] && has_base="1"
        [ -e "$game_path/${file}_DISABLED" ] && has_disabled="1"

        if [ "$want_base" = "1" ] && [ "$has_base" = "0" ] && [ "$has_disabled" = "1" ]; then
            mv "$game_path/${file}_DISABLED" "$game_path/$file"
            has_base="1"
            has_disabled="0"
        fi
        if [ "$want_base" = "0" ] && [ "$want_disabled" = "1" ] && [ "$has_base" = "1" ] && [ "$has_disabled" = "0" ]; then
            mv "$game_path/$file" "$game_path/${file}_DISABLED"
        fi
    done < "$state_file"
}

detect_crash_signature() {
    local bootstrap_log="$1"
    local game_path="$2"
    local hit="0"

    if [ -f "$bootstrap_log" ]; then
        if tail -n 400 "$bootstrap_log" | grep -Eq "HarmonyInteropFix\\.Apply|ConsoleSetOutFix\\.Apply|DetourHelper\\.GetIdentifiable|HarmonyException: (Patching exception|IL Compile Error)|NullReferenceException"; then
            hit="1"
        fi
    fi

    local preloader_log=""
    local preloader_candidates=("$game_path"/preloader_*.log(N))
    if [ "${#preloader_candidates[@]}" -gt 0 ]; then
        preloader_log=$(ls -1t "${preloader_candidates[@]}" 2>/dev/null | head -n 1 || true)
    fi
    if [ -n "$preloader_log" ] && [ -f "$preloader_log" ]; then
        if tail -n 300 "$preloader_log" | grep -Eq "HarmonyInteropFix\\.Apply|ConsoleSetOutFix\\.Apply|DetourHelper\\.GetIdentifiable|HarmonyException: (Patching exception|IL Compile Error)|NullReferenceException"; then
            hit="1"
        fi
    fi

    printf "%s\n" "$hit"
}

run_case() {
    local game="$1"
    local mode="$2"
    local game_path
    game_path=$(game_path_for "$game")
    local run_script="$game_path/run_bepinex.sh"
    local launcher_log="$OUT_DIR/${game}_${mode}.launcher.log"
    local result="FAIL"
    local reason=""

    if [ ! -f "$run_script" ]; then
        reason="missing_run_bepinex.sh"
        printf "%s\t%s\t%s\t%s\t0\t0\t0\t0\t-\t%s\n" "$game" "$mode" "$result" "$reason" "$launcher_log" >> "$RESULTS_TSV"
        return
    fi

    local executable_name
    executable_name=$(parse_executable_name "$run_script")
    local exec_path
    exec_path=$(resolve_exec_path "$game_path" "$executable_name")
    local exec_name
    exec_name=$(basename "$exec_path")

    local bootstrap_log="$game_path/r2modmac_bootstrap.log"
    local bepinex_log="$game_path/BepInEx/LogOutput.log"
    local before_bootstrap_mtime
    local before_bepinex_mtime
    before_bootstrap_mtime=$(mtime_or_zero "$bootstrap_log")
    before_bepinex_mtime=$(mtime_or_zero "$bepinex_log")
    local before_pids
    before_pids=$(collect_pids "$exec_path" "$exec_name")

    if [ "$mode" = "modded" ]; then
        set_runtime_mode "$game_path" "enabled"
    else
        set_runtime_mode "$game_path" "disabled"
    fi

    log "Launching game=$game mode=$mode exec=$exec_path"
    : > "$launcher_log"
    (
        cd "$game_path" || exit 1
        /bin/bash "$run_script"
    ) > "$launcher_log" 2>&1 &
    local launcher_pid=$!

    local process_started="0"
    local probe=0
    local current_pids=""
    while [ "$probe" -lt 24 ]; do
        sleep 1
        current_pids=$(collect_pids "$exec_path" "$exec_name")
        local new_pids=""
        new_pids=$(diff_pids "$before_pids" "$current_pids")
        if [ -n "$new_pids" ]; then
            process_started="1"
            break
        fi
        probe=$((probe + 1))
    done

    if [ "$mode" = "modded" ]; then
        local settle=0
        while [ "$settle" -lt 18 ]; do
            if [ "$(mtime_or_zero "$bepinex_log")" -gt "$before_bepinex_mtime" ]; then
                break
            fi
            if [ "$(detect_crash_signature "$bootstrap_log" "$game_path")" = "1" ]; then
                break
            fi
            sleep 1
            settle=$((settle + 1))
        done
    else
        sleep 3
    fi

    local after_bootstrap_mtime
    local after_bepinex_mtime
    after_bootstrap_mtime=$(mtime_or_zero "$bootstrap_log")
    after_bepinex_mtime=$(mtime_or_zero "$bepinex_log")

    local bootstrap_updated="0"
    local bepinex_updated="0"
    [ "$after_bootstrap_mtime" -gt "$before_bootstrap_mtime" ] && bootstrap_updated="1"
    [ "$after_bepinex_mtime" -gt "$before_bepinex_mtime" ] && bepinex_updated="1"

    local crash_signature
    crash_signature=$(detect_crash_signature "$bootstrap_log" "$game_path")

    if [ "$mode" = "modded" ]; then
        if [ "$crash_signature" = "1" ]; then
            result="FAIL"
            reason="crash_signature_detected"
        elif [ "$bepinex_updated" = "1" ] && [ "$process_started" = "1" ]; then
            result="PASS"
            reason="bepinex_loaded_and_process_started"
        elif [ "$bepinex_updated" = "1" ]; then
            result="PASS"
            reason="bepinex_loaded_process_detached"
        else
            result="FAIL"
            reason="no_bepinex_activity"
        fi
    else
        if [ "$bepinex_updated" = "1" ]; then
            result="FAIL"
            reason="vanilla_triggered_bepinex"
        elif [ "$process_started" = "1" ] || [ "$bootstrap_updated" = "1" ]; then
            result="PASS"
            reason="vanilla_started_without_bepinex"
        else
            result="FAIL"
            reason="vanilla_not_started"
        fi
    fi

    current_pids=$(collect_pids "$exec_path" "$exec_name")
    local new_pids=""
    new_pids=$(diff_pids "$before_pids" "$current_pids")
    if kill -0 "$launcher_pid" 2>/dev/null; then
        kill "$launcher_pid" 2>/dev/null || true
    fi
    kill_pid_list "$new_pids" "TERM"
    sleep 2
    current_pids=$(collect_pids "$exec_path" "$exec_name")
    new_pids=$(diff_pids "$before_pids" "$current_pids")
    kill_pid_list "$new_pids" "KILL"

    printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
        "$game" "$mode" "$result" "$reason" "$process_started" "$bootstrap_updated" "$bepinex_updated" "$crash_signature" "$exec_path" "$launcher_log" \
        >> "$RESULTS_TSV"
}

main() {
    log "Output directory: $OUT_DIR"
    local game=""
    for game in "${GAMES[@]}"; do
        local game_path
        game_path=$(game_path_for "$game")
        if [ ! -d "$game_path" ]; then
            log "Skipping game=$game (path missing: $game_path)"
            printf "%s\tmodded\tFAIL\tmissing_game_path\t0\t0\t0\t0\t-\t-\n" "$game" >> "$RESULTS_TSV"
            printf "%s\tvanilla\tFAIL\tmissing_game_path\t0\t0\t0\t0\t-\t-\n" "$game" >> "$RESULTS_TSV"
            continue
        fi

        log "Preparing game=$game path=$game_path"
        capture_initial_runtime_state "$game" "$game_path"
        run_case "$game" "modded"
        run_case "$game" "vanilla"
        restore_runtime_state "$game" "$game_path"
        log "Finished game=$game"
    done

    log "Matrix completed. Results file: $RESULTS_TSV"
    cat "$RESULTS_TSV"
}

main "$@"
