use super::*;

const MACOS_STEAM_APP_PROCESS_NAME: &str = "steam_osx";
const MACOS_STEAM_HELPER_PROCESS_NAMES: &[&str] = &["Steam Helper", "steamwebhelper"];
const MACOS_STEAM_QUIT_PROCESS_NAMES: &[&str] = &[
    MACOS_STEAM_APP_PROCESS_NAME,
    "Steam Helper",
    "steamwebhelper",
];
const MACOS_STEAM_KILL_FALLBACK_PATTERNS: &[&str] = &[
    "steam_osx",
    "steamwebhelper",
    "Steam Helper",
    "Steam.AppBundle",
    "steam.sh",
    "steam_monitor.sh",
];

pub(crate) fn is_named_process_running_on_macos(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/pgrep")
            .args(["-x", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub(crate) fn is_steam_app_running_on_macos() -> bool {
    is_named_process_running_on_macos(MACOS_STEAM_APP_PROCESS_NAME)
}

pub(crate) fn is_steam_running_on_macos() -> bool {
    is_steam_app_running_on_macos()
        || MACOS_STEAM_HELPER_PROCESS_NAMES
            .iter()
            .any(|name| is_named_process_running_on_macos(name))
}

#[cfg(target_os = "macos")]
pub(crate) fn collect_macos_steam_process_ids(steam_roots: &[std::path::PathBuf]) -> HashSet<u32> {
    let mut pids = HashSet::new();

    for steam_root in steam_roots {
        let steam_app_root = steam_root.join("Steam.AppBundle").join("Steam");
        if !steam_app_root.exists() {
            continue;
        }

        let pattern = regex::escape(&steam_app_root.to_string_lossy());
        let Ok(output) = std::process::Command::new("/usr/bin/pgrep")
            .args(["-f", &pattern])
            .output()
        else {
            continue;
        };

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                pids.insert(pid);
            }
        }
    }

    pids
}

#[cfg(target_os = "macos")]
pub(crate) fn has_macos_steam_processes(steam_roots: &[std::path::PathBuf]) -> bool {
    is_steam_running_on_macos() || !collect_macos_steam_process_ids(steam_roots).is_empty()
}

#[cfg(target_os = "macos")]
pub(crate) fn collect_macos_steam_process_snapshot() -> Vec<String> {
    let Ok(output) = std::process::Command::new("/usr/bin/pgrep")
        .args([
            "-af",
            "steam_osx|steamwebhelper|Steam Helper|Steam.AppBundle|steam.sh|steam_monitor.sh",
        ])
        .output()
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn has_macos_steam_processes(_steam_roots: &[std::path::PathBuf]) -> bool {
    false
}

pub(crate) fn quit_steam_if_running(steam_roots: &[std::path::PathBuf]) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let steam_app_was_running = is_steam_app_running_on_macos();
        if !has_macos_steam_processes(steam_roots) {
            return Ok(false);
        }

        if steam_app_was_running {
            log::debug!(
                "[quit_steam_if_running] Steam is running — force killing Steam processes to apply launch option changes immediately..."
            );
        } else {
            log::debug!(
                "[quit_steam_if_running] Steam.app is not running, but helper processes are still alive — clearing stale Steam helpers before launch option update..."
            );
        }

        for pid in collect_macos_steam_process_ids(steam_roots) {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-9", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        for process_name in MACOS_STEAM_QUIT_PROCESS_NAMES {
            let _ = std::process::Command::new("/usr/bin/killall")
                .args(["-9", process_name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        for pattern in MACOS_STEAM_KILL_FALLBACK_PATTERNS {
            let _ = std::process::Command::new("/usr/bin/pkill")
                .args(["-9", "-f", pattern])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        for pid in collect_macos_steam_process_ids(steam_roots) {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-9", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        if has_macos_steam_processes(steam_roots) {
            let leftovers = collect_macos_steam_process_snapshot();
            log::debug!(
                "[quit_steam_if_running] Some Steam processes are still present after force-kill; proceeding anyway."
            );
            if !leftovers.is_empty() {
                log::debug!(
                    "[quit_steam_if_running] leftover_processes={}",
                    leftovers.join(" | ")
                );
            }
        } else {
            log::debug!("[quit_steam_if_running] Steam processes terminated.");
        }

        return Ok(steam_app_was_running);
    }

    #[allow(unreachable_code)]
    Ok(false)
}

pub(crate) fn emit_steam_launch_options_restart_event(app: &AppHandle) {
    let _ = app.emit(STEAM_LAUNCH_OPTIONS_RESTART_EVENT, true);
}

#[cfg(target_os = "macos")]
pub(crate) fn relaunch_macos_steam_if_needed(_steam_root: &std::path::Path) {
    let mut command = std::process::Command::new("/usr/bin/open");
    command.args(["-a", "Steam"]);

    let Ok(_) = command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        log::error!("[relaunch_macos_steam_if_needed] Failed to issue Steam relaunch request.");
        return;
    };
    log::debug!("[relaunch_macos_steam_if_needed] Steam relaunch requested.");
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn relaunch_macos_steam_if_needed(_steam_root: &std::path::Path) {}

#[cfg(target_os = "macos")]
pub(crate) fn ensure_macos_steam_running_for_launch(app: &AppHandle) {
    let steam_roots = get_steam_roots_for_platform(app, false);
    if is_steam_app_running_on_macos() && !collect_macos_steam_process_ids(&steam_roots).is_empty()
    {
        return;
    }

    let mut started = false;
    for args in [
        vec!["-b", "com.valvesoftware.steam"],
        vec!["-a", "/Applications/Steam.app"],
        vec!["-a", "Steam"],
    ] {
        match std::process::Command::new("/usr/bin/open")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                log::info!(
                    "[launch_via_steam_for_game_path] Steam startup requested via `/usr/bin/open {}` pid={}",
                    args.join(" "),
                    child.id()
                );
                started = true;
                break;
            }
            Err(error) => {
                log::warn!(
                    "[launch_via_steam_for_game_path] Steam startup attempt failed via `/usr/bin/open {}` error={}",
                    args.join(" "),
                    error
                );
            }
        }
    }

    if !started {
        for steam_binary in macos_steam_binary_candidates() {
            if !steam_binary.is_file() {
                continue;
            }
            match std::process::Command::new(&steam_binary)
                .arg("-silent")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    log::info!(
                        "[launch_via_steam_for_game_path] Steam startup requested via `{}` pid={}",
                        steam_binary.display(),
                        child.id()
                    );
                    started = true;
                    break;
                }
                Err(error) => {
                    log::warn!(
                        "[launch_via_steam_for_game_path] Steam binary launch failed path={} error={}",
                        steam_binary.display(),
                        error
                    );
                }
            }
        }
    }

    if !started {
        log::error!(
            "[launch_via_steam_for_game_path] Failed to start Steam.app before steam://run."
        );
        return;
    }

    let observe_started = std::time::Instant::now();
    while observe_started.elapsed().as_millis() < 10_000 {
        if is_steam_app_running_on_macos()
            && !collect_macos_steam_process_ids(&steam_roots).is_empty()
        {
            log::info!(
                "[launch_via_steam_for_game_path] Steam startup observed elapsed_ms={}",
                observe_started.elapsed().as_millis()
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    log::warn!(
        "[launch_via_steam_for_game_path] Steam startup not fully observed; continuing with steam://run dispatch."
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn ensure_macos_steam_running_for_launch(_app: &AppHandle) {}
