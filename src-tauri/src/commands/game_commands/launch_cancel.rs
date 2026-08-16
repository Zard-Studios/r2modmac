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

use std::sync::atomic::{AtomicBool, Ordering};

/// Reported when a wait ended because the user asked for it.
///
/// The frontend matches on this to tell a cancellation apart from a failure:
/// one deserves an explanation on screen, the other is the user getting what
/// they asked for.
pub(crate) const LAUNCH_CANCELLED_MESSAGE: &str = "Launch cancelled.";

static LAUNCH_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Clear any cancellation left over from a previous attempt.
///
/// Called as a launch begins, so pressing Play after cancelling starts from a
/// clean slate rather than aborting instantly.
pub(crate) fn begin_launch() {
    LAUNCH_CANCELLED.store(false, Ordering::Release);
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

/// Ask the native macOS Steam to quit, the way its own menu does.
///
/// `steam://exit` is Valve's own shutdown route, so the client closes its
/// sessions and writes its files first. Nothing here force-kills: killing
/// Steam mid-boot is what left it crashing on the next start.
#[cfg(target_os = "macos")]
pub(crate) fn shut_down_steam_after_cancel() {
    log::info!("[cancel_game_launch] Asking Steam to quit; r2modmac started it for this launch.");

    if std::process::Command::new("/usr/bin/open")
        .arg("steam://exit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
    {
        return;
    }

    // Only if the URL handler is not registered — an install Steam has never
    // been run from, most likely.
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", "tell application \"Steam\" to quit"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
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
