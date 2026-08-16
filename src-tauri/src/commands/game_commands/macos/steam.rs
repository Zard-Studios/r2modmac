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

/// Watch for the game after Steam has been asked to start it.
///
/// Kept separate from the launch itself, and taking the "is it up?" answer as a
/// closure, so the whole decision can be exercised in tests without a Steam.
#[cfg(target_os = "macos")]
fn observe_macos_steam_launch(
    app_id: &str,
    timeout_ms: u64,
    console_offsets: &[(std::path::PathBuf, u64)],
    is_started: impl Fn() -> bool,
    mut retry_dispatch: impl FnMut(),
    should_cancel: impl Fn() -> bool,
) -> Result<(), String> {
    let observe_started = std::time::Instant::now();
    let mut retried_dispatch = false;
    let mut saw_app_error_18 = false;
    let mut saw_logon_failure = false;
    let mut next_log_check = observe_started;
    let mut confirmation = StartConfirmation::new();

    while observe_started.elapsed().as_millis() < u128::from(timeout_ms) {
        // Checked first, and before any retry: once the user has called the
        // launch off there is nothing left worth dispatching or explaining.
        if should_cancel() {
            log::info!(
                "[launch_via_steam_for_game_path] Stopped waiting for app {} because the user cancelled the launch.",
                app_id
            );
            return Err(super::super::launch_cancel::LAUNCH_CANCELLED_MESSAGE.to_string());
        }

        let was_pending = confirmation.is_pending();
        if confirmation.observe(is_started()) {
            return Ok(());
        }

        if !confirmation.is_pending() {
            if was_pending {
                log::debug!(
                    "[launch_via_steam_for_game_path] A process matching app {} disappeared before it could be confirmed; it was not the game. Still waiting.",
                    app_id
                );
            }

            let now = std::time::Instant::now();
            if now >= next_log_check {
                saw_app_error_18 |=
                    console_log_contains_launch_app_error_18(console_offsets, app_id);
                saw_logon_failure |= console_log_contains_logon_failure(console_offsets);

                if saw_app_error_18
                    && !retried_dispatch
                    && observe_started.elapsed().as_millis()
                        >= u128::from(STEAM_LAUNCH_RETRY_TRIGGER_DELAY_MS)
                {
                    retry_dispatch();
                    retried_dispatch = true;
                }

                next_log_check =
                    now + std::time::Duration::from_millis(STEAM_LAUNCH_LOG_CHECK_INTERVAL_MS);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(
            STEAM_LAUNCH_POLL_INTERVAL_MS,
        ));
    }

    if saw_app_error_18 && saw_logon_failure {
        log::error!(
            "[launch_via_steam_for_game_path] Steam reported LaunchApp failure (AppError_18) and `LogonFailure No Connection` while launching app {}.",
            app_id
        );
        return Err(
            "Steam could not start the game because it is not signed in. Open Steam, sign in, then press Play again."
                .to_string(),
        );
    }

    if saw_app_error_18 {
        log::error!(
            "[launch_via_steam_for_game_path] Steam reported LaunchApp failure (AppError_18) for app {} even after one retry.",
            app_id
        );
        return Err(
            "Steam refused to start the game. Open Steam to check for a prompt or an error waiting for you there, then press Play again."
                .to_string(),
        );
    }

    log::warn!(
        "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time.",
        app_id
    );
    Err(
        "Steam accepted the launch but the game did not start in time. Check the Steam window, then press Play again."
            .to_string(),
    )
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

    let steam_was_running = ensure_macos_steam_running_for_launch(app);
    let child = dispatch_macos_steam_run_url(&app_id)?;
    log::info!(
        "[launch_via_steam_for_game_path] open_dispatched pid={} elapsed_ms={}",
        child.id(),
        launch_start.elapsed().as_millis()
    );

    if let Some(executable_path) = executable_path.as_ref() {
        // Wait here rather than in a detached thread whose verdict nobody
        // reads. The caller — and through it the Play button — needs to know
        // when the game is actually up: a cold Steam can take half a minute to
        // get there, and returning early is what left the button flipping back
        // to "play" while the launch was still in flight.
        #[cfg(target_os = "macos")]
        {
            let observe_timeout_ms = if steam_was_running {
                MACOS_LAUNCH_OBSERVE_TIMEOUT_MS
            } else {
                MACOS_COLD_STEAM_LAUNCH_OBSERVE_TIMEOUT_MS
            };
            log::info!(
                "[launch_via_steam_for_game_path] observing app_id={} steam_was_running={} timeout_ms={}",
                app_id,
                steam_was_running,
                observe_timeout_ms
            );

            observe_macos_steam_launch(
                &app_id,
                observe_timeout_ms,
                &console_offsets,
                || is_process_running_for_executable(executable_path),
                || {
                    match dispatch_macos_steam_run_url(&app_id) {
                    Ok(retry_child) => log::warn!(
                        "[launch_via_steam_for_game_path] observed LaunchApp failure for app_id={} in Steam logs; retrying steam://run once pid={}",
                        app_id,
                        retry_child.id()
                    ),
                    Err(error) => log::error!(
                        "[launch_via_steam_for_game_path] retry steam://run failed for app_id={} error={}",
                        app_id,
                        error
                    ),
                }
                },
                crate::commands::game_commands::launch_cancel::launch_cancelled,
            )?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_via_steam_for_game_path] Steam accepted the launch request for app {}, but the game process was not observed in time. Continuing optimistically.",
                    app_id
                );
            }
        }
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

    /// The Muck launch from the log: Steam is asked to run the game, and the
    /// game only appears twenty-two seconds later. The wait has to still be
    /// going when it does, or the Play button flips back before the game is up.
    #[test]
    fn a_game_that_takes_a_while_is_still_waited_for() {
        let started_at = std::time::Instant::now();
        let appears_after = std::time::Duration::from_millis(1_200);

        let outcome = observe_macos_steam_launch(
            "1625450",
            10_000,
            &[],
            || started_at.elapsed() >= appears_after,
            || panic!("no Steam error was recorded, so no retry should be dispatched"),
            || false,
        );

        assert!(outcome.is_ok(), "{outcome:?}");
        assert!(
            started_at.elapsed() >= appears_after,
            "the wait must not end before the game exists"
        );
    }

    /// A process that shows up and disappears is one of the helpers Steam
    /// spawns while booting, not the game.
    #[test]
    fn a_phantom_process_does_not_end_the_wait() {
        let started_at = std::time::Instant::now();

        let outcome = observe_macos_steam_launch(
            "1625450",
            2_000,
            &[],
            || {
                let elapsed = started_at.elapsed().as_millis();
                // Present briefly, then gone for good.
                (300..600).contains(&elapsed)
            },
            || {},
            || false,
        );

        assert!(
            outcome.is_err(),
            "a phantom must not be reported as a started game"
        );
    }

    /// A launch nobody can see the end of has to say so, rather than reporting
    /// success and leaving the UI to guess.
    #[test]
    fn a_game_that_never_appears_is_reported_as_a_failure() {
        let outcome = observe_macos_steam_launch("1625450", 1_000, &[], || false, || {}, || false);

        let message = outcome.expect_err("a launch that never started is not a success");
        assert!(message.contains("did not start"), "{message}");
    }

    /// Steam's own log explains the refusal, so the user gets that instead of a
    /// bare timeout.
    #[test]
    fn a_steam_side_refusal_is_explained() {
        let log_path = unique_temp_log_path("apperror18");
        std::fs::write(
            &log_path,
            "GameAction [AppID 1625450] : LaunchApp failed with AppError_18\n",
        )
        .unwrap();
        let offsets = vec![(log_path.clone(), 0u64)];
        let retries = std::cell::Cell::new(0u32);

        let outcome = observe_macos_steam_launch(
            "1625450",
            5_000,
            &offsets,
            || false,
            || retries.set(retries.get() + 1),
            || false,
        );

        let message = outcome.expect_err("Steam refused the launch");
        assert!(message.contains("Steam refused"), "{message}");
        assert_eq!(retries.get(), 1, "the launch is retried exactly once");
        std::fs::remove_file(log_path).unwrap();
    }

    /// Issue #36: the launch that hangs. Steam is not running, the game never
    /// appears, and the wait is three minutes long — pressing the button again
    /// has to end it in a moment, and say so rather than blaming the game.
    #[test]
    fn a_cancelled_launch_stops_waiting_instead_of_running_the_deadline_out() {
        let started_at = std::time::Instant::now();
        let cancelled_after = std::time::Duration::from_millis(600);

        let outcome = observe_macos_steam_launch(
            "1625450",
            180_000,
            &[],
            || false,
            || panic!("a cancelled launch must not be retried"),
            || started_at.elapsed() >= cancelled_after,
        );

        let message = outcome.expect_err("a cancelled launch is not a started game");
        assert_eq!(
            message,
            crate::commands::game_commands::launch_cancel::LAUNCH_CANCELLED_MESSAGE
        );
        assert!(
            started_at.elapsed() < std::time::Duration::from_secs(5),
            "cancelling must be near-instant, took {:?}",
            started_at.elapsed()
        );
    }

    /// Cancelling is the user's decision, not Steam's: even with a refusal in
    /// the log, the message must be the cancellation, and no retry may fire.
    #[test]
    fn cancelling_wins_over_a_steam_side_refusal_and_skips_the_retry() {
        let log_path = unique_temp_log_path("cancelled_apperror18");
        std::fs::write(
            &log_path,
            "GameAction [AppID 1625450] : LaunchApp failed with AppError_18\n",
        )
        .unwrap();
        let offsets = vec![(log_path.clone(), 0u64)];

        let outcome = observe_macos_steam_launch(
            "1625450",
            180_000,
            &offsets,
            || false,
            || panic!("a cancelled launch must not be retried"),
            || true,
        );

        assert_eq!(
            outcome.expect_err("cancelled"),
            crate::commands::game_commands::launch_cancel::LAUNCH_CANCELLED_MESSAGE
        );
        std::fs::remove_file(log_path).unwrap();
    }

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
