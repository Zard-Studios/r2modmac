use super::*;

#[cfg(target_os = "macos")]
const STEAM_LAUNCH_RETRY_TRIGGER_DELAY_MS: u64 = 3_500;
#[cfg(target_os = "macos")]
const STEAM_LAUNCH_LOG_CHECK_INTERVAL_MS: u64 = 1_200;
#[cfg(target_os = "macos")]
const STEAM_LAUNCH_POLL_INTERVAL_MS: u64 = 250;

pub(crate) fn macos_steam_binary_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from(
        "/Applications/Steam.app/Contents/MacOS/steam_osx",
    )];

    if let Some(home) = dirs::home_dir() {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("Steam")
                .join("Steam.AppBundle")
                .join("Steam")
                .join("Contents")
                .join("MacOS")
                .join("steam_osx"),
        );
    }

    candidates
}

pub(crate) fn dispatch_macos_steam_run_url(app_id: &str) -> Result<std::process::Child, String> {
    let steam_url = format!("steam://run/{}", app_id);

    #[cfg(target_os = "macos")]
    {
        if let Ok(child) = std::process::Command::new("/usr/bin/open")
            .arg(&steam_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            return Ok(child);
        }

        if let Ok(child) = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.valvesoftware.steam"])
            .arg(&steam_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            return Ok(child);
        }

        for steam_binary in macos_steam_binary_candidates() {
            if !steam_binary.is_file() {
                continue;
            }
            if let Ok(child) = std::process::Command::new(&steam_binary)
                .arg(&steam_url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                return Ok(child);
            }
        }
    }

    std::process::Command::new("/usr/bin/open")
        .arg(&steam_url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to ask Steam to launch the game: {}", e))
}

#[cfg(target_os = "macos")]
fn collect_macos_console_log_offsets(app: &AppHandle) -> Vec<(std::path::PathBuf, u64)> {
    let mut offsets = Vec::new();
    let mut seen = HashSet::new();
    for steam_root in get_steam_roots_for_platform(app, false) {
        let path = steam_root.join("logs").join("console_log.txt");
        if !path.exists() {
            continue;
        }

        let canonical = canonicalize_or_original(&path);
        if !seen.insert(canonical) {
            continue;
        }

        let size = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        offsets.push((path, size));
    }
    offsets
}

#[cfg(target_os = "macos")]
fn read_console_log_tail_from_offset(path: &std::path::Path, offset: u64) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let start = std::cmp::min(offset as usize, bytes.len());
    Some(String::from_utf8_lossy(&bytes[start..]).to_string())
}

#[cfg(target_os = "macos")]
fn console_log_contains_launch_app_error_18(
    offsets: &[(std::path::PathBuf, u64)],
    app_id: &str,
) -> bool {
    let app_marker = format!("AppID {}", app_id);
    offsets.iter().any(|(path, offset)| {
        read_console_log_tail_from_offset(path, *offset)
            .map(|tail| {
                tail.contains(&app_marker) && tail.contains("LaunchApp failed with AppError_18")
            })
            .unwrap_or(false)
    })
}

#[cfg(target_os = "macos")]
fn console_log_contains_logon_failure(offsets: &[(std::path::PathBuf, u64)]) -> bool {
    offsets.iter().any(|(path, offset)| {
        read_console_log_tail_from_offset(path, *offset)
            .map(|tail| tail.contains("LogonFailure No Connection"))
            .unwrap_or(false)
    })
}

pub(crate) fn launch_via_steam_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let launch_start = std::time::Instant::now();
    let app_id = find_steam_app_id_for_game_path_any(app, game_path, false)
        .ok_or_else(|| "Couldn't determine the Steam app ID for this game".to_string())?;
    let executable_path = find_macos_executable_path(game_path);
    log::info!(
        "[launch_via_steam_for_game_path] start app_id={} game_path={}",
        app_id,
        game_path.display()
    );

    #[cfg(target_os = "macos")]
    let console_offsets = collect_macos_console_log_offsets(app);

    ensure_macos_steam_running_for_launch(app);
    let child = dispatch_macos_steam_run_url(&app_id)?;
    log::info!(
        "[launch_via_steam_for_game_path] open_dispatched pid={} elapsed_ms={}",
        child.id(),
        launch_start.elapsed().as_millis()
    );

    if let Some(executable_path) = executable_path.as_ref() {
        let observed_executable_path = executable_path.clone();
        let observed_app_id = app_id.clone();
        #[cfg(target_os = "macos")]
        let observed_console_offsets = console_offsets.clone();
        std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            {
                let observe_started = std::time::Instant::now();
                let mut retried_dispatch = false;
                let mut saw_app_error_18 = false;
                let mut saw_logon_failure = false;
                let mut next_log_check = observe_started;

                while observe_started.elapsed().as_millis()
                    < u128::from(MACOS_LAUNCH_OBSERVE_TIMEOUT_MS)
                {
                    if is_process_running_for_executable(&observed_executable_path) {
                        return;
                    }

                    let now = std::time::Instant::now();
                    if now >= next_log_check {
                        saw_app_error_18 |= console_log_contains_launch_app_error_18(
                            &observed_console_offsets,
                            &observed_app_id,
                        );
                        saw_logon_failure |=
                            console_log_contains_logon_failure(&observed_console_offsets);

                        if saw_app_error_18
                            && !retried_dispatch
                            && observe_started.elapsed().as_millis()
                                >= u128::from(STEAM_LAUNCH_RETRY_TRIGGER_DELAY_MS)
                        {
                            match dispatch_macos_steam_run_url(&observed_app_id) {
                                Ok(retry_child) => {
                                    log::warn!(
                                        "[launch_via_steam_for_game_path] observed LaunchApp failure for app_id={} in Steam logs; retrying steam://run once pid={}",
                                        observed_app_id,
                                        retry_child.id()
                                    );
                                }
                                Err(error) => {
                                    log::error!(
                                        "[launch_via_steam_for_game_path] retry steam://run failed for app_id={} error={}",
                                        observed_app_id, error
                                    );
                                }
                            }
                            retried_dispatch = true;
                        }

                        next_log_check = now
                            + std::time::Duration::from_millis(STEAM_LAUNCH_LOG_CHECK_INTERVAL_MS);
                    }

                    std::thread::sleep(std::time::Duration::from_millis(
                        STEAM_LAUNCH_POLL_INTERVAL_MS,
                    ));
                }

                if saw_app_error_18 {
                    if saw_logon_failure {
                        log::error!(
                            "[launch_via_steam_for_game_path] Steam reported LaunchApp failure (AppError_18) and `LogonFailure No Connection` while launching app {}. Steam connectivity/login state likely blocked the modded launch.",
                            observed_app_id
                        );
                    } else {
                        log::error!(
                            "[launch_via_steam_for_game_path] Steam reported LaunchApp failure (AppError_18) for app {} even after one retry.",
                            observed_app_id
                        );
                    }
                } else {
                    log::warn!(
                        "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                        observed_app_id
                    );
                }
            }

            #[cfg(not(target_os = "macos"))]
            if !wait_for_process_start(&observed_executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS) {
                log::warn!(
                    "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                    observed_app_id
                );
            }
        });
    }

    log::info!(
        "[launch_via_steam_for_game_path] done app_id={} total_elapsed_ms={}",
        app_id,
        launch_start.elapsed().as_millis()
    );

    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn unique_temp_log_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "r2modmac_steam_test_{}_{}_{}.log",
            name,
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn detects_launch_app_error_18_from_new_log_tail() {
        let path = unique_temp_log_path("apperror18");
        let prefix = "[2026-04-15 19:37:14] ExecuteSteamURL: \"steam://run/1030300\"\n";
        fs::write(&path, prefix).expect("write prefix");
        let offset = fs::metadata(&path).expect("metadata").len();
        let suffix = "[2026-04-15 19:37:20] GameAction [AppID 1030300, ActionID 1] : LaunchApp failed with AppError_18 with \"\"\n";
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, suffix.as_bytes()))
            .expect("append suffix");

        let offsets = vec![(path.clone(), offset)];
        assert!(console_log_contains_launch_app_error_18(
            &offsets, "1030300"
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn detects_logon_failure_from_new_log_tail() {
        let path = unique_temp_log_path("logonfailure");
        let prefix = "[2026-04-15 19:37:14] ExecuteSteamURL: \"steam://run/1030300\"\n";
        fs::write(&path, prefix).expect("write prefix");
        let offset = fs::metadata(&path).expect("metadata").len();
        let suffix = "[2026-04-15 19:37:21] LogonFailure No Connection\n";
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, suffix.as_bytes()))
            .expect("append suffix");

        let offsets = vec![(path.clone(), offset)];
        assert!(console_log_contains_logon_failure(&offsets));
        let _ = fs::remove_file(path);
    }
}
