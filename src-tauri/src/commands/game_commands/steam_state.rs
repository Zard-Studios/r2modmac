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
    pub const UPDATE_REQUIRED: u64 = 2;
    pub const FILES_MISSING: u64 = 32;
    pub const FILES_CORRUPT: u64 = 128;
    pub const UPDATE_RUNNING: u64 = 256;
    pub const UPDATE_PAUSED: u64 = 512;
    pub const UPDATE_STARTED: u64 = 1024;
}

/// How many trailing bytes of `console_log.txt` to inspect. The file grows to
/// several MB over months; only the tail describes the launch we just asked for.
const CONSOLE_LOG_TAIL_BYTES: u64 = 256 * 1024;

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
            "This game has a paused Steam update. Resume it in Steam's Downloads page, then try again."
                .to_string(),
        );
    }
    if flags & (state_flags::UPDATE_RUNNING | state_flags::UPDATE_STARTED) != 0 {
        return Some(
            "Steam is currently updating this game. Wait for the download to finish, then try again."
                .to_string(),
        );
    }
    if flags & state_flags::UPDATE_REQUIRED != 0 {
        return Some(
            "This game has a pending Steam update. Steam will not start it until the update is installed — open Steam and let it download, then try again."
                .to_string(),
        );
    }
    None
}

/// Inspect a Steam console log tail for a launch that is parked on a prompt.
///
/// Steam writes one `GameAction [AppID N, ActionID M]` line per launch step. A
/// launch that is waiting on the user ends on `waiting for user response to X`
/// with no following `continues with user response` for the same step — that is
/// the invisible-dialog case.
pub(crate) fn pending_user_prompt_for_app(log_contents: &str, app_id: &str) -> Option<String> {
    let marker = format!("[AppID {},", app_id);
    let mut waiting_on: Option<String> = None;

    for line in log_contents.lines() {
        if !line.contains(&marker) {
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
        "SynchronizingCloud" => "Steam is waiting for an answer to a Steam Cloud conflict for this game. Open Steam, respond to the cloud sync prompt, then try again.".to_string(),
        "ShowInterstitials" => "Steam is waiting for an answer to a prompt shown before the game starts. Open Steam, respond to it, then try again.".to_string(),
        other => format!(
            "Steam is waiting for an answer to its '{}' prompt. Open Steam, respond to it, then try again.",
            other
        ),
    }
}

/// Read the tail of Steam's console log, if it exists.
pub(crate) fn read_console_log_tail(steam_root: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = steam_root.join("logs").join("console_log.txt");
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if length > CONSOLE_LOG_TAIL_BYTES {
        file.seek(SeekFrom::Start(length - CONSOLE_LOG_TAIL_BYTES))
            .ok()?;
    }
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
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
    let log = read_console_log_tail(steam_root)?;
    pending_user_prompt_for_app(&log, app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn update_required_alone_is_reported() {
        let blocker = describe_state_blocker(4 | 2).expect("update required must be reported");
        assert!(blocker.contains("pending Steam update"), "{blocker}");
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
    fn app_id_match_is_not_a_loose_substring() {
        // "352729" must not match "[AppID 3527290," — the trailing comma anchors it.
        let log = "[2026-08-06 20:07:37] GameAction [AppID 3527290, ActionID 1] : LaunchApp waiting for user response to SynchronizingCloud \"pendingcloudsessions\"\n";
        assert_eq!(pending_user_prompt_for_app(log, "352729"), None);
    }
}
