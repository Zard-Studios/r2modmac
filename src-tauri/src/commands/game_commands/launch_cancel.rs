//! Cancelling a launch that is waiting on Steam (issue #36).
//! See `docs/stopping-a-launch.md`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Matched by the frontend (`src/utils/launchIssue.ts`) to keep an error dialog
/// off the screen for something the user asked for.
pub(crate) const LAUNCH_CANCELLED_MESSAGE: &str = "Launch cancelled.";

static LAUNCH_CANCELLED: AtomicBool = AtomicBool::new(false);

static LAUNCH_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Pressing Play after cancelling must not abort the new launch instantly.
pub(crate) fn begin_launch() {
    LAUNCH_CANCELLED.store(false, Ordering::Release);
    LAUNCH_GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub(crate) fn launch_generation() -> u64 {
    LAUNCH_GENERATION.load(Ordering::Acquire)
}

/// Guards the shutdown watch, which outlives the launch: it must not close the
/// Steam a new launch is starting.
pub(crate) fn launch_superseded(generation: u64) -> bool {
    launch_generation() != generation
}

pub(crate) fn launch_cancelled() -> bool {
    LAUNCH_CANCELLED.load(Ordering::Acquire)
}

pub(crate) fn ensure_not_cancelled() -> Result<(), String> {
    if launch_cancelled() {
        Err(LAUNCH_CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

/// Only the Steam r2modmac started is closed; one the user already had open may
/// be mid-download.
pub(crate) fn cancelled_launch_should_close_steam(steam_was_running: bool) -> bool {
    !steam_was_running
}

/// Matches the cold-Steam launch deadline: a booting client answers nothing, so
/// the watch has to outlive its boot.
const STEAM_QUIT_TOTAL_MS: u64 = 180_000;
const STEAM_QUIT_ROUND_MS: u64 = 3_000;
const STEAM_QUIT_POLL_MS: u64 = 500;

/// Waits for Steam to appear, then alternates the AppleScript quit (by bundle
/// id, so it finds the copy under `Steam.AppBundle`) with `steam://exit`, which
/// a booting client ignores. Success is judged by Steam's processes: the
/// AppleScript quit reports -128 while quitting cleanly. Nothing is killed.
#[cfg(target_os = "macos")]
pub(crate) fn shut_down_steam_after_cancel() {
    let generation = launch_generation();

    std::thread::spawn(move || {
        log::info!(
            "[cancel_game_launch] Watching for the Steam this launch started, to close it again."
        );

        let started = std::time::Instant::now();
        let mut seen_running = false;
        let mut rounds = 0u32;

        while started.elapsed() < std::time::Duration::from_millis(STEAM_QUIT_TOTAL_MS) {
            if launch_superseded(generation) {
                log::info!(
                    "[cancel_game_launch] A new launch started; leaving Steam alone after {} round(s).",
                    rounds
                );
                return;
            }

            if !super::is_steam_running_on_macos() {
                if seen_running {
                    log::info!(
                        "[cancel_game_launch] Steam closed after {} round(s), {}ms.",
                        rounds,
                        started.elapsed().as_millis()
                    );
                    return;
                }
                // `steam://exit` would start Steam through the URL handler.
                std::thread::sleep(std::time::Duration::from_millis(STEAM_QUIT_POLL_MS));
                continue;
            }

            seen_running = true;
            rounds += 1;
            if rounds % 2 == 1 {
                let _ = std::process::Command::new("/usr/bin/osascript")
                    .args([
                        "-e",
                        "tell application id \"com.valvesoftware.steam\" to quit",
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg("steam://exit")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }

            std::thread::sleep(std::time::Duration::from_millis(STEAM_QUIT_ROUND_MS));
        }

        if seen_running && !super::is_steam_running_on_macos() {
            log::info!(
                "[cancel_game_launch] Steam closed after {} round(s).",
                rounds
            );
        } else if seen_running {
            log::warn!(
                "[cancel_game_launch] Steam did not close after {} round(s). Leaving it running rather than killing it.",
                rounds
            );
        } else {
            log::info!("[cancel_game_launch] Steam never came up; nothing to close.");
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn shut_down_steam_after_cancel() {}

/// Stop waiting on the launch in flight.
///
/// Safe to call when nothing is launching: the flag is cleared by the next
/// launch before any wait reads it.
#[tauri::command]
pub async fn cancel_game_launch() -> Result<bool, String> {
    log::info!("[cancel_game_launch] The user asked to stop waiting for the launch.");
    LAUNCH_CANCELLED.store(true, Ordering::Release);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    // One process-wide flag, so this is a single test rather than a race.
    #[test]
    fn a_launch_starts_uncancelled_stays_cancelled_and_is_reset_by_the_next_launch() {
        begin_launch();
        assert!(!launch_cancelled());
        assert!(ensure_not_cancelled().is_ok());

        LAUNCH_CANCELLED.store(true, Ordering::Release);
        assert!(launch_cancelled());
        assert_eq!(
            ensure_not_cancelled().unwrap_err(),
            LAUNCH_CANCELLED_MESSAGE
        );

        begin_launch();
        assert!(!launch_cancelled());
        assert!(ensure_not_cancelled().is_ok());
    }

    /// Starts and closes the real Steam, so it is not part of a normal run.
    ///
    /// ```text
    /// cargo test --lib shuts_a_booting_steam_down_for_real -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "starts and closes the real Steam"]
    #[cfg(target_os = "macos")]
    fn shuts_a_booting_steam_down_for_real() {
        let _ = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.valvesoftware.steam"])
            .status();
        std::thread::sleep(std::time::Duration::from_secs(3));
        assert!(
            super::super::is_steam_running_on_macos(),
            "Steam did not start, so there is nothing to shut down"
        );

        let started = std::time::Instant::now();
        shut_down_steam_after_cancel();

        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(STEAM_QUIT_TOTAL_MS + 5_000);
        let mut closed = false;
        while std::time::Instant::now() < deadline {
            if !super::super::is_steam_running_on_macos() {
                closed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        println!("Steam closed: {} after {:?}", closed, started.elapsed());
        assert!(closed, "Steam was still running after the whole watch");
    }

    /// Stop pressed one second after Play, before Steam exists.
    #[test]
    #[ignore = "starts and closes the real Steam"]
    #[cfg(target_os = "macos")]
    fn shuts_steam_down_even_when_cancelled_before_it_appears() {
        begin_launch();
        let _ = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.valvesoftware.steam"])
            .status();

        std::thread::sleep(std::time::Duration::from_secs(1));
        let started = std::time::Instant::now();
        shut_down_steam_after_cancel();

        let deadline = started + std::time::Duration::from_millis(STEAM_QUIT_TOTAL_MS + 5_000);
        let mut ever_seen = false;
        let mut closed = false;
        while std::time::Instant::now() < deadline {
            if super::super::is_steam_running_on_macos() {
                ever_seen = true;
            } else if ever_seen {
                closed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        println!(
            "Steam came up: {}, then closed: {} after {:?}",
            ever_seen,
            closed,
            started.elapsed()
        );
        assert!(ever_seen, "Steam never started, so nothing was proven");
        assert!(
            closed,
            "Steam was left running after a cancel it did not see"
        );
    }

    #[test]
    fn a_new_launch_stands_the_leftover_shutdown_down() {
        begin_launch();
        let generation = launch_generation();
        assert!(!launch_superseded(generation));

        begin_launch();
        assert!(
            launch_superseded(generation),
            "the leftover watch would have kept closing Steam under the new launch"
        );
    }

    #[test]
    fn cancelling_closes_only_the_steam_r2modmac_started_itself() {
        assert!(cancelled_launch_should_close_steam(false));

        assert!(!cancelled_launch_should_close_steam(true));
    }
}
