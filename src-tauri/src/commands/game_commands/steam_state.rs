//! Detection of Steam-side conditions that silently block a game launch.
//!
//! When r2modmac asks Steam to start a game, Steam can accept the request and
//! then stop before creating the process — waiting on a pending update, or on a
//! modal prompt the user never sees because they are looking at r2modmac rather
//! than at Steam. From the app's point of view the launch simply "does nothing",
//! which is what issue #25 reports.
//!
//! Both cases are observable from files Steam already maintains, so the launch
//! path can explain what happened instead of timing out silently.

use std::path::Path;

/// Bits of `StateFlags` in `steamapps/appmanifest_<appid>.acf`.
mod state_flags {
    pub const UNINSTALLED: u64 = 1;
    #[allow(dead_code)]
    pub const UPDATE_REQUIRED: u64 = 2;
    pub const FILES_MISSING: u64 = 32;
    pub const FILES_CORRUPT: u64 = 128;
    pub const UPDATE_RUNNING: u64 = 256;
    pub const UPDATE_PAUSED: u64 = 512;
    pub const UPDATE_STARTED: u64 = 1024;
}

/// How many trailing bytes of `console_log.txt` to inspect. The file grows to
/// several MB over months; only the tail describes the launch we just asked for.
const CONSOLE_LOG_TAIL_BYTES: u64 = 1024 * 1024;

pub(crate) fn appmanifest_path(steam_root: &Path, app_id: &str) -> std::path::PathBuf {
    steam_root
        .join("steamapps")
        .join(format!("appmanifest_{}.acf", app_id))
}

/// Read `StateFlags` out of an appmanifest.
///
/// The manifest is Valve's KeyValues text format; the flags live on a single
/// `"StateFlags"  "N"` line, so a full VDF parser would be overkill here.
pub(crate) fn read_state_flags(steam_root: &Path, app_id: &str) -> Option<u64> {
    let contents = std::fs::read_to_string(appmanifest_path(steam_root, app_id)).ok()?;
    parse_state_flags(&contents)
}

fn parse_state_flags(manifest: &str) -> Option<u64> {
    for line in manifest.lines() {
        let Some(rest) = line.trim().strip_prefix("\"StateFlags\"") else {
            continue;
        };
        // Remaining text is whitespace then the quoted value.
        return rest.trim().trim_matches('"').parse::<u64>().ok();
    }
    None
}

/// Describe why Steam will refuse to launch this app, if it will.
///
/// Returns `None` when nothing in the manifest blocks a launch.
pub(crate) fn describe_state_blocker(flags: u64) -> Option<String> {
    if flags & state_flags::UNINSTALLED != 0 {
        return Some("Steam reports this game as not installed.".to_string());
    }
    if flags & state_flags::FILES_CORRUPT != 0 {
        return Some(
            "Steam reports corrupted game files. Verify the game's files in Steam, then try again."
                .to_string(),
        );
    }
    if flags & state_flags::FILES_MISSING != 0 {
        return Some(
            "Steam reports missing game files. Verify the game's files in Steam, then try again."
                .to_string(),
        );
    }
    if flags & state_flags::UPDATE_PAUSED != 0 {
        return Some(
            "This game has a paused update in Steam. Resume the download in Steam before launching."
                .to_string(),
        );
    }
    if flags & (state_flags::UPDATE_RUNNING | state_flags::UPDATE_STARTED) != 0 {
        return Some(
            "Steam is currently updating this game. Wait for the update to finish before launching."
                .to_string(),
        );
    }
    None
}

/// Does this log line refer to `app_id`?
///
/// Steam writes the id both as `[AppID 123, ActionID 1]` and as
/// `AppID 123 "path"`, so the trailing character varies. It must still be a
/// boundary, or app 123 would match every line belonging to app 1234.
fn line_mentions_app(line: &str, app_id: &str) -> bool {
    let needle = format!("AppID {}", app_id);
    let mut search_from = 0;

    while let Some(offset) = line[search_from..].find(&needle) {
        let end = search_from + offset + needle.len();
        match line[end..].chars().next() {
            None => return true,
            Some(next) if !next.is_ascii_digit() => return true,
            _ => search_from = end,
        }
    }
    false
}

/// Inspect a Steam console log tail for a launch that is parked on a prompt.
///
/// Steam writes one `GameAction [AppID N, ActionID M]` line per launch step. A
/// launch that is waiting on the user ends on `waiting for user response to X`
/// with no following `continues with user response` for the same step — that is
/// the invisible-dialog case.
pub(crate) fn pending_user_prompt_for_app(log_contents: &str, app_id: &str) -> Option<String> {
    let mut waiting_on: Option<String> = None;

    for line in log_contents.lines() {
        if !line_mentions_app(line, app_id) {
            continue;
        }
        if let Some(index) = line.find("waiting for user response to ") {
            let task = line[index + "waiting for user response to ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .to_string();
            if !task.is_empty() {
                waiting_on = Some(task);
            }
        } else if line.contains("continues with user response") || line.contains("changed task to")
        {
            // The launch moved on; whatever it was waiting for was answered.
            waiting_on = None;
        }
    }

    waiting_on.map(|task| describe_pending_prompt(&task))
}

fn describe_pending_prompt(task: &str) -> String {
    match task {
        "SynchronizingCloud" | "CloudSync" => {
            "This game has a Steam Cloud conflict. Resolve the conflict in Steam before launching."
                .to_string()
        }
        "ShowInterstitials" => {
            "Steam is waiting for a response to a prompt before launch. Open Steam to respond."
                .to_string()
        }
        other => format!(
            "Steam is waiting for a response to '{}'. Resolve it in Steam before launching.",
            other
        ),
    }
}

/// Read the tail of Steam's console log, if it exists.
pub(crate) fn read_console_log_tail(steam_root: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = steam_root.join("logs").join("console_log.txt");
    if let Ok(mut file) = std::fs::File::open(&path) {
        let length = file.metadata().ok()?.len();
        if length > CONSOLE_LOG_TAIL_BYTES {
            let _ = file.seek(SeekFrom::Start(length - CONSOLE_LOG_TAIL_BYTES));
        }
        let mut buffer = Vec::new();
        if file.read_to_end(&mut buffer).is_ok() {
            return Some(String::from_utf8_lossy(&buffer).into_owned());
        }
    }
    None
}

/// Explain a launch that Steam accepted but never completed.
///
/// Called after the game process fails to appear, so the user gets the actual
/// reason instead of a bare timeout.
pub(crate) fn explain_stalled_launch(steam_root: &Path, app_id: &str) -> Option<String> {
    if let Some(flags) = read_state_flags(steam_root, app_id) {
        if let Some(blocker) = describe_state_blocker(flags) {
            return Some(blocker);
        }
    }
    if let Some(log) = read_console_log_tail(steam_root) {
        if let Some(prompt) = pending_user_prompt_for_app(&log, app_id) {
            return Some(prompt);
        }
    }

    // Fallback: check native macOS Steam log if different from steam_root
    if let Some(home) = dirs::home_dir() {
        let mac_steam_root = home.join("Library/Application Support/Steam");
        if mac_steam_root != steam_root && mac_steam_root.exists() {
            if let Some(log) = read_console_log_tail(&mac_steam_root) {
                if let Some(prompt) = pending_user_prompt_for_app(&log, app_id) {
                    return Some(prompt);
                }
            }
        }
    }

    None
}

/// How a launch request ended.
pub(crate) enum LaunchWaitOutcome {
    /// The game process appeared.
    Started,
    /// Steam is not going to start the game until something is resolved.
    Blocked(String),
    /// Nothing observable happened before the deadline.
    TimedOut,
}

/// Wait for the game to appear, watching Steam for a reason it will not start.
///
/// Polling Steam's own state while waiting means a blocked launch is reported
/// as soon as Steam records it, rather than after the caller's full timeout —
/// a Steam Cloud conflict shows up within a couple of seconds, so there is no
/// reason to make the user stare at a spinner for a minute first.
pub(crate) fn wait_for_launch_or_blocker(
    steam_root: &Path,
    app_id: &str,
    timeout_ms: u64,
    is_started: impl Fn() -> bool,
) -> LaunchWaitOutcome {
    const POLL_INTERVAL_MS: u64 = 250;
    // Reading a 256 KB log tail every tick would be wasteful; Steam takes a
    // moment to write the prompt line anyway.
    const STEAM_CHECK_EVERY: u64 = 8;

    let attempts = std::cmp::max(1, timeout_ms / POLL_INTERVAL_MS);
    for attempt in 0..attempts {
        if is_started() {
            return LaunchWaitOutcome::Started;
        }
        if attempt > 0 && attempt % STEAM_CHECK_EVERY == 0 {
            if let Some(reason) = explain_stalled_launch(steam_root, app_id) {
                // Re-check the process first: the game may have started in the
                // same tick, which beats a stale log line.
                if is_started() {
                    return LaunchWaitOutcome::Started;
                }
                return LaunchWaitOutcome::Blocked(reason);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    if is_started() {
        return LaunchWaitOutcome::Started;
    }
    match explain_stalled_launch(steam_root, app_id) {
        Some(reason) => LaunchWaitOutcome::Blocked(reason),
        None => LaunchWaitOutcome::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a throwaway Steam root with the given appmanifest / console log.
    fn fake_steam_root(app_id: &str, state_flags: u64, console_log: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-steamstate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::write(
            root.join("steamapps")
                .join(format!("appmanifest_{}.acf", app_id)),
            format!(
                "\"AppState\"\n{{\n\t\"StateFlags\"\t\t\"{}\"\n}}",
                state_flags
            ),
        )
        .unwrap();
        std::fs::write(root.join("logs").join("console_log.txt"), console_log).unwrap();
        root
    }

    #[test]
    fn wait_reports_started_without_consulting_steam() {
        let root = fake_steam_root("1229490", 4, "");
        let outcome = wait_for_launch_or_blocker(&root, "1229490", 5_000, || true);
        assert!(matches!(outcome, LaunchWaitOutcome::Started));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wait_reports_a_cloud_block_well_before_the_deadline() {
        let log = "[20:07:37] GameAction [AppID 3527290, ActionID 1] : LaunchApp waiting for user response to SynchronizingCloud \"pendingcloudsessions\"\n";
        let root = fake_steam_root("3527290", 4, log);
        let started = std::time::Instant::now();
        // A generous deadline: the point is that it returns as soon as Steam's
        // state is readable, not that it waits it out.
        let outcome = wait_for_launch_or_blocker(&root, "3527290", 60_000, || false);
        let elapsed = started.elapsed();
        match outcome {
            LaunchWaitOutcome::Blocked(reason) => {
                assert!(reason.contains("Steam Cloud"), "{reason}")
            }
            _ => panic!("expected Blocked"),
        }
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "should report early, took {elapsed:?}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wait_reports_a_pending_update_as_blocked() {
        // StateFlags 1030 — PEAK's observed state with a pending download.
        let root = fake_steam_root("3527290", 1030, "");
        match wait_for_launch_or_blocker(&root, "3527290", 30_000, || false) {
            LaunchWaitOutcome::Blocked(reason) => assert!(reason.contains("updating"), "{reason}"),
            _ => panic!("expected Blocked"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wait_times_out_when_steam_reports_nothing() {
        let root = fake_steam_root("1229490", 4, "");
        let outcome = wait_for_launch_or_blocker(&root, "1229490", 1_000, || false);
        assert!(matches!(outcome, LaunchWaitOutcome::TimedOut));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_game_that_starts_during_the_wait_beats_a_stale_log_line() {
        // The log records an old prompt, but the process is up: starting wins,
        // otherwise a leftover line would fail a launch that actually worked.
        let log = "[20:07:37] GameAction [AppID 3527290, ActionID 1] : LaunchApp waiting for user response to SynchronizingCloud \"pendingcloudsessions\"\n";
        let root = fake_steam_root("3527290", 4, log);
        let outcome = wait_for_launch_or_blocker(&root, "3527290", 10_000, || true);
        assert!(matches!(outcome, LaunchWaitOutcome::Started));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_state_flags_from_appmanifest() {
        let manifest =
            "\"AppState\"\n{\n\t\"appid\"\t\t\"3527290\"\n\t\"StateFlags\"\t\t\"1030\"\n}";
        assert_eq!(parse_state_flags(manifest), Some(1030));
    }

    #[test]
    fn fully_installed_state_has_no_blocker() {
        // StateFlags 4 = Fully Installed. This is ULTRAKILL's observed state,
        // which launches normally.
        assert_eq!(describe_state_blocker(4), None);
    }

    #[test]
    fn pending_update_state_is_reported() {
        // StateFlags 1030 = Fully Installed + Update Required + Update Started.
        // This is PEAK's observed state while it had a 1.3 GB pending update,
        // during which Steam parked every launch in DownloadingDepots.
        let blocker = describe_state_blocker(1030).expect("1030 must be reported as a blocker");
        assert!(blocker.contains("updating"), "{blocker}");
    }

    #[test]
    fn update_required_alone_is_not_a_blocker() {
        assert_eq!(describe_state_blocker(4 | 2), None);
    }

    #[test]
    fn corrupt_and_missing_files_are_reported() {
        assert!(describe_state_blocker(4 | 128)
            .expect("corrupt")
            .contains("corrupted"));
        assert!(describe_state_blocker(4 | 32)
            .expect("missing")
            .contains("missing"));
    }

    #[test]
    fn detects_launch_parked_on_a_cloud_conflict() {
        // Verbatim shape of the lines observed while PEAK would not launch.
        let log = concat!(
            "[2026-08-06 20:07:36] GameAction [AppID 3527290, ActionID 1] : LaunchApp changed task to SynchronizingCloud with \"\"\n",
            "[2026-08-06 20:07:37] GameAction [AppID 3527290, ActionID 1] : LaunchApp waiting for user response to SynchronizingCloud \"pendingcloudsessions\"\n",
        );
        let prompt = pending_user_prompt_for_app(log, "3527290").expect("should detect the prompt");
        assert!(prompt.contains("Steam Cloud"), "{prompt}");
    }

    #[test]
    fn answered_prompt_is_not_reported() {
        // ULTRAKILL's observed sequence: it waits on ShowInterstitials but then
        // continues, so nothing should be reported.
        let log = concat!(
            "[2026-08-06 20:09:56] GameAction [AppID 1229490, ActionID 2] : LaunchApp waiting for user response to ShowInterstitials \"\"\n",
            "[2026-08-06 20:09:56] GameAction [AppID 1229490, ActionID 2] : LaunchApp continues with user response \"ShowInterstitials\"\n",
            "[2026-08-06 20:09:57] GameAction [AppID 1229490, ActionID 2] : LaunchApp changed task to Completed with \"\"\n",
        );
        assert_eq!(pending_user_prompt_for_app(log, "1229490"), None);
    }

    #[test]
    fn ignores_prompts_belonging_to_a_different_app() {
        let log = "[2026-08-06 20:07:37] GameAction [AppID 3527290, ActionID 1] : LaunchApp waiting for user response to SynchronizingCloud \"pendingcloudsessions\"\n";
        assert_eq!(pending_user_prompt_for_app(log, "1229490"), None);
    }

    #[test]
    fn matches_both_the_comma_and_space_forms_steam_writes() {
        // "[AppID N, ActionID M]" for GameAction lines, "AppID N \"path\"" for
        // process lines — both must count as referring to the app.
        assert!(line_mentions_app(
            "GameAction [AppID 3527290, ActionID 1] : LaunchApp",
            "3527290"
        ));
        assert!(line_mentions_app(
            "Game process added : AppID 3527290 \"\"C:\\PEAK.exe\"\"",
            "3527290"
        ));
    }

    #[test]
    fn a_shorter_app_id_does_not_match_a_longer_one() {
        // 352729 must not match app 3527290, in either form.
        assert!(!line_mentions_app(
            "GameAction [AppID 3527290, ActionID 1] : LaunchApp",
            "352729"
        ));
        assert!(!line_mentions_app(
            "Game process added : AppID 3527290 \"x\"",
            "352729"
        ));
    }

    #[test]
    fn app_id_match_is_not_a_loose_substring() {
        // "352729" must not match "[AppID 3527290," — the trailing comma anchors it.
        let log = "[2026-08-06 20:07:37] GameAction [AppID 3527290, ActionID 1] : LaunchApp waiting for user response to SynchronizingCloud \"pendingcloudsessions\"\n";
        assert_eq!(pending_user_prompt_for_app(log, "352729"), None);
    }
}
