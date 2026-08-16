//! Letting the user call off a launch that is going nowhere.
//!
//! Asking Steam to start a game is not a request that fails quickly. A cold
//! Steam has to boot and sign in first, so the launch paths wait up to three
//! minutes before deciding nothing happened — and for all that time the Play
//! button was disabled, spinning, with no way back (issue #36). If Steam is not
//! running at all, or is sitting on a prompt the user has already dealt with,
//! that wait is pure dead time.
//!
//! So every launch wait consults a cancellation flag alongside its own
//! deadline, and pressing the button again sets it. The flag only ends the
//! *waiting*: nothing is killed. A game that starts a moment later is still the
//! user's game, and the running-state poll picks it up as usual.
//!
//! The waits themselves take "should I stop?" as a closure rather than reading
//! the flag directly, which is what keeps them testable without a real Steam —
//! the same shape the "is it running yet?" checks already use.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Reported when a wait ended because the user asked for it.
///
/// The frontend matches on this to tell a cancellation apart from a failure:
/// one deserves an explanation on screen, the other is the user getting what
/// they asked for.
pub(crate) const LAUNCH_CANCELLED_MESSAGE: &str = "Launch cancelled.";

static LAUNCH_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Bumped by every launch, so work left over from a cancelled one can tell that
/// it has been superseded and stand down.
static LAUNCH_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Clear any cancellation left over from a previous attempt.
///
/// Called as a launch begins, so pressing Play after cancelling starts from a
/// clean slate rather than aborting instantly.
pub(crate) fn begin_launch() {
    LAUNCH_CANCELLED.store(false, Ordering::Release);
    LAUNCH_GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// Which launch this is. Taken by anything that keeps working after a launch
/// has returned, and handed back to [`launch_superseded`].
pub(crate) fn launch_generation() -> u64 {
    LAUNCH_GENERATION.load(Ordering::Acquire)
}

/// Has another launch started since `generation` was taken?
///
/// Shutting Steam down after a cancellation takes a while — Steam has to exist
/// before it can be closed. If the user presses Play again in the meantime, that
/// work must stop immediately: closing Steam under a launch that is starting it
/// would be a far worse bug than the one being fixed.
pub(crate) fn launch_superseded(generation: u64) -> bool {
    launch_generation() != generation
}

/// Has the user asked for the launch in flight to stop?
pub(crate) fn launch_cancelled() -> bool {
    LAUNCH_CANCELLED.load(Ordering::Acquire)
}

/// `Err(LAUNCH_CANCELLED_MESSAGE)` when cancelled, so call sites can bail with
/// a `?` instead of repeating the message.
pub(crate) fn ensure_not_cancelled() -> Result<(), String> {
    if launch_cancelled() {
        Err(LAUNCH_CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

/// Should a cancelled launch also shut Steam down?
///
/// Only when r2modmac started Steam for this launch. By the time the button
/// can be pressed again the request has already gone out — Steam was started
/// and `steam://run` was dispatched — so ending the wait alone leaves Steam
/// booting and the game coming up regardless, which is not what "stop" means
/// to anyone watching.
///
/// A Steam the user already had open is never touched: they were using it
/// before this launch and closing it would cost them whatever else it was
/// doing. That is also exactly the case issue #36 is about — the hang happens
/// when Steam is *not* running.
pub(crate) fn cancelled_launch_should_close_steam(steam_was_running: bool) -> bool {
    !steam_was_running
}

/// How long to keep watching for the Steam a cancelled launch started.
///
/// Long enough to cover a cold boot: the user can press stop a second after
/// pressing Play, when Steam does not exist yet and there is nothing to close.
/// Standing down at that moment is what let Steam boot on regardless and start
/// the game anyway — so the watch outlives the boot instead.
const STEAM_QUIT_TOTAL_MS: u64 = 60_000;

/// How long one quit request is given to take effect before the next.
const STEAM_QUIT_ROUND_MS: u64 = 3_000;

/// How often the watch looks at the world while it waits.
const STEAM_QUIT_POLL_MS: u64 = 500;

/// Keep asking the Steam a cancelled launch started to close, until it does.
///
/// Written as a watch rather than a request because of what the log showed: the
/// user cancels about a second after pressing Play, long before Steam is up,
/// so a single request lands on nothing and Steam boots on to start the game.
/// The watch instead waits for Steam to appear, asks it to close, and keeps
/// asking until its processes are gone.
///
/// A quit is only ever sent while Steam is actually visible. `steam://exit`
/// goes through the URL handler, which would *start* Steam if it were not
/// running — the exact opposite of the point.
///
/// Two routes are alternated, because a Steam that is still booting answers
/// neither reliably:
///
/// * `tell application id "com.valvesoftware.steam" to quit` — the AppleScript
///   quit, addressed by bundle id rather than by name so it also finds the copy
///   Steam's own updater keeps under
///   `~/Library/Application Support/Steam/Steam.AppBundle`, which is the one
///   that actually runs.
/// * `steam://exit` — Valve's shutdown URL. A booting client ignores it
///   outright (measured: still up twenty seconds later); a booted one answers
///   in a few seconds.
///
/// Whether Steam went away is decided by looking for its processes, never by an
/// exit code: the AppleScript quit reports error -128 ("cancelled by the user")
/// while quitting Steam perfectly well.
///
/// Nothing force-kills. Killing Steam mid-boot is what left it crashing on the
/// next start, so a client that keeps refusing is left running and the log says
/// so. The watch also stands down the moment another launch begins.
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
                // Not up yet. Steam takes its time; keep watching rather than
                // sending a request that would start it.
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

    // These share one process-wide flag, so they run as a single test rather
    // than racing each other in parallel.
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

        // Pressing Play again must not abort instantly.
        begin_launch();
        assert!(!launch_cancelled());
        assert!(ensure_not_cancelled().is_ok());
    }

    /// The real thing, against the real Steam on this machine.
    ///
    /// Ignored by default because it starts and closes Steam, which no ordinary
    /// test run should do. Run it deliberately when the shutdown routes need
    /// checking — they are macOS behaviour, not ours, and they have already
    /// changed once (a booting Steam ignores `steam://exit`):
    ///
    /// ```text
    /// cargo test --lib shuts_a_booting_steam_down_for_real -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "starts and closes the real Steam"]
    #[cfg(target_os = "macos")]
    fn shuts_a_booting_steam_down_for_real() {
        // Start Steam the way a launch does, then cancel while it is booting —
        // the exact situation issue #36 is about.
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

        // The watch runs on its own thread so the button comes back at once;
        // give it the whole window before believing it failed.
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

    /// The case the log caught: stop pressed one second after Play, before
    /// Steam exists. A single request would land on nothing and Steam would
    /// boot on to start the game — the watch has to outlive the boot.
    #[test]
    #[ignore = "starts and closes the real Steam"]
    #[cfg(target_os = "macos")]
    fn shuts_steam_down_even_when_cancelled_before_it_appears() {
        begin_launch();
        let _ = std::process::Command::new("/usr/bin/open")
            .args(["-b", "com.valvesoftware.steam"])
            .status();

        // One second in: this is what the user actually does.
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
        assert!(closed, "Steam was left running after a cancel it did not see");
    }

    #[test]
    fn a_new_launch_stands_the_leftover_shutdown_down() {
        // Cancel, then press Play again while Steam is still closing: the old
        // watch must not close the Steam the new launch is starting.
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
        // The hang this fixes happens with Steam down: r2modmac starts it, and
        // cancelling has to undo that, or Steam boots and launches the game
        // anyway — a "stop" that stops nothing.
        assert!(cancelled_launch_should_close_steam(false));

        // A Steam the user already had open stays open. It was not ours to
        // close, and they may be mid-download in it.
        assert!(!cancelled_launch_should_close_steam(true));
    }
}
