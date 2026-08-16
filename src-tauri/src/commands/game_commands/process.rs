use super::*;

// ── Shared build helpers ──────────────────────────────────────────────────────

pub(crate) fn push_unique_pattern(patterns: &mut Vec<String>, pattern: String) {
    if !pattern.is_empty() && !patterns.contains(&pattern) {
        patterns.push(pattern)
    }
}

pub(crate) fn build_process_match_pattern(path: &std::path::Path) -> String {
    regex::escape(&path.to_string_lossy())
}

pub(crate) fn process_text_candidates(process: &sysinfo::Process) -> Vec<String> {
    let mut candidates = Vec::new();

    let name = process.name().to_string_lossy();
    if !name.is_empty() {
        candidates.push(name.into_owned());
    }

    if let Some(exe_path) = process.exe() {
        let exe_text = exe_path.to_string_lossy();
        if !exe_text.is_empty() {
            candidates.push(exe_text.into_owned());
        }
    }

    let command_line = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    if !command_line.is_empty() {
        candidates.push(command_line);
    }

    candidates
}

/// The exact argument handed to `pgrep -f`: the pattern un-escaped, then made
/// self-immune. Built here alone, so tests exercise what the poll runs.
#[cfg(target_os = "macos")]
pub(crate) fn pgrep_pattern_for(pattern: &str) -> String {
    let plain = pattern.replace(r"\/", "/").replace(r"\.", ".");
    make_pgrep_pattern_self_immune(&plain)
}

/// Stops a `pgrep -f` pattern from matching the `pgrep` that runs it.
///
/// `-f` matches whole command lines, so two overlapping polls see each other
/// and report the game as running while Steam is still booting. Bracketing the
/// first character fixes it: `[P]EAK.exe` still matches `PEAK.exe`, but is no
/// longer matched by itself.
#[cfg(target_os = "macos")]
fn make_pgrep_pattern_self_immune(plain: &str) -> String {
    let Some(first) = plain.chars().next() else {
        return String::new();
    };

    // Only characters that mean themselves inside a bracket expression are
    // safe to wrap; anything else is left as written.
    if !first.is_ascii_alphanumeric() && first != '/' && first != '_' {
        return plain.to_string();
    }

    format!("[{}]{}", first, &plain[first.len_utf8()..])
}

// ── Cross-platform process detection ─────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
pub(crate) fn is_process_running_for_pattern(pattern: &str) -> bool {
    is_process_running_for_patterns(&[pattern.to_string()])
}

pub(crate) fn is_process_running_for_patterns(patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }

    let compiled_patterns: Vec<regex::Regex> = patterns
        .iter()
        .filter_map(|pattern| regex::Regex::new(pattern).ok())
        .collect();
    if compiled_patterns.is_empty() {
        return false;
    }

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let matched_by_sysinfo = system.processes().values().any(|process| {
        let candidates = process_text_candidates(process);
        candidates
            .iter()
            .any(|candidate| compiled_patterns.iter().any(|re| {
                if re.is_match(candidate) {
                    log::debug!(
                        "[is_process_running_for_patterns] sysinfo MATCHED. PID: {}, Name: {}, Pattern: {:?}, Candidate: {:?}",
                        process.pid(), process.name().to_string_lossy(), re.as_str(), candidate
                    );
                    true
                } else {
                    false
                }
            }))
    });

    if matched_by_sysinfo {
        return true;
    }

    // Windows: try tasklist as a fallback for processes not visible to sysinfo.
    #[cfg(windows)]
    {
        for image_name in patterns
            .iter()
            .filter_map(|pattern| windows_image_name_from_pattern(pattern))
        {
            if is_windows_process_running_tasklist(&image_name) {
                return true;
            }
        }
    }

    // macOS: sysinfo may miss sandboxed or Rosetta-translated processes.
    // Use pgrep -f with the plain (regex-unescaped) pattern as a last resort.
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        for pattern in patterns {
            let needle = pgrep_pattern_for(pattern);
            if needle.is_empty() {
                continue;
            }
            let status = Command::new("pgrep").arg("-f").arg(&needle).output();
            if let Ok(output) = status {
                if output.status.success() {
                    let matched_text = String::from_utf8_lossy(&output.stdout);
                    log::debug!(
                        "[is_process_running_for_patterns] Pattern {:?} MATCHED. pgrep output:\n{}",
                        pattern,
                        matched_text
                    );
                    return true;
                }
            }
        }
    }

    false
}

/// Does any candidate of one process match, without being an excluded one?
///
/// Pulled out of the poll so the exclusion rule can be tested directly.
#[cfg_attr(unix, allow(dead_code))]
fn process_matches_excluding(
    candidates: &[String],
    patterns: &[regex::Regex],
    exclude: &str,
) -> bool {
    if candidates
        .iter()
        .any(|candidate| candidate.contains(exclude))
    {
        return false;
    }
    candidates
        .iter()
        .any(|candidate| patterns.iter().any(|pattern| pattern.is_match(candidate)))
}

/// Decide, from the command lines `pgrep -fl` printed, whether anything is
/// still running that is not one of ours.
///
/// Separated from the call to `pgrep` so the rule can be tested against the
/// real lines seen on a machine, without needing those processes to exist.
fn any_line_is_a_live_match(lines: &str, exclude: &str) -> bool {
    lines
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        // `pgrep -f` matches whole command lines, including the command that
        // asked the question.
        .filter(|line| !line.contains("pgrep"))
        .any(|line| !line.contains(exclude))
}

/// Like [`is_process_running_for_patterns`], but blind to processes whose
/// command line contains `exclude`.
///
/// Needed to watch a Wine-hosted Steam close. `steam.exe -shutdown` is itself a
/// process carrying the path of `steam.exe`, and it does not exit once it has
/// delivered the request, so a watch that counts it never sees Steam go away —
/// measured at seventy seconds of chasing its own tail while Steam had in fact
/// closed within twenty.
///
/// Reads the command lines from `pgrep -fl` rather than from sysinfo, because
/// sysinfo does not expose the arguments of a Wine-hosted process: our
/// `steam.exe -shutdown` appears there as nothing more than a copy of
/// `steam.exe` in Wine's temp directory, which made an exclusion built on
/// sysinfo silently useless.
#[cfg(unix)]
pub(crate) fn is_process_running_for_patterns_excluding(
    patterns: &[String],
    exclude: &str,
) -> bool {
    patterns.iter().any(|pattern| {
        let Ok(output) = std::process::Command::new("/usr/bin/pgrep")
            .arg("-fl")
            .arg(pattern)
            .output()
        else {
            return false;
        };
        any_line_is_a_live_match(&String::from_utf8_lossy(&output.stdout), exclude)
    })
}

/// Windows has no `pgrep`, and no Wine in the way either: sysinfo reports the
/// arguments of a native process, so the same exclusion works from there.
#[cfg(not(unix))]
pub(crate) fn is_process_running_for_patterns_excluding(
    patterns: &[String],
    exclude: &str,
) -> bool {
    let compiled: Vec<regex::Regex> = patterns
        .iter()
        .filter_map(|pattern| regex::Regex::new(pattern).ok())
        .collect();
    if compiled.is_empty() {
        return false;
    }

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system.processes().values().any(|process| {
        process_matches_excluding(&process_text_candidates(process), &compiled, exclude)
    })
}

pub(crate) fn is_process_running_for_executable(executable_path: &std::path::Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        is_process_running_for_patterns(&build_macos_process_match_patterns(executable_path))
    }

    #[cfg(not(target_os = "macos"))]
    {
        is_process_running_for_pattern(&build_process_match_pattern(executable_path))
    }
}

// ── Confirming that a game really started ────────────────────────────────────

/// How long a process must be seen without interruption before it counts as
/// the game that was asked for.
///
/// A Steam that is still booting spawns short-lived helpers, and some of them
/// carry the game's name. Believing the first sighting is what reported a
/// launch as finished seconds after the request, while the game itself was
/// still half a minute away. A game that has really started stays up, so a
/// second of continuous presence separates the two and costs a launch nothing.
pub(crate) const START_CONFIRMATION_WINDOW_MS: u64 = 1_000;

/// Turns a stream of "is it running?" answers into "has it really started?".
///
/// Poll intervals differ between launch paths, so this measures elapsed time
/// rather than counting polls: the guarantee is the same wherever it is used.
#[derive(Default)]
pub(crate) struct StartConfirmation {
    first_seen: Option<std::time::Instant>,
}

impl StartConfirmation {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feeds one observation. Returns true once the process has been present
    /// for the whole confirmation window; a single absence resets the clock.
    pub(crate) fn observe(&mut self, running: bool) -> bool {
        if !running {
            self.first_seen = None;
            return false;
        }

        let since = *self.first_seen.get_or_insert_with(std::time::Instant::now);
        since.elapsed() >= std::time::Duration::from_millis(START_CONFIRMATION_WINDOW_MS)
    }

    /// True when something is currently matching but has not been there long
    /// enough to be believed — used only to explain the wait in the log.
    pub(crate) fn is_pending(&self) -> bool {
        self.first_seen.is_some()
    }
}

// ── Process polling helpers ───────────────────────────────────────────────────

/// How a wait for a game process ended.
///
/// `Cancelled` exists so a launch the user called off is not reported as a
/// game that failed to start: the deadline was never reached, the user simply
/// stopped waiting (issue #36).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StartWait {
    Started,
    TimedOut,
    Cancelled,
}

impl StartWait {
    pub(crate) fn started(&self) -> bool {
        matches!(self, StartWait::Started)
    }

    /// `Err(LAUNCH_CANCELLED_MESSAGE)` when the user called the launch off, so
    /// a launch path can hand the decision straight back to the caller with a
    /// `?` and leave its own "did not start" wording for a real timeout.
    pub(crate) fn ok_unless_cancelled(&self) -> Result<(), String> {
        if matches!(self, StartWait::Cancelled) {
            Err(super::launch_cancel::LAUNCH_CANCELLED_MESSAGE.to_string())
        } else {
            Ok(())
        }
    }
}

const START_POLL_INTERVAL_MS: u64 = 250;

/// The decision behind every start wait, with both the world and the user's
/// patience passed in as closures so it can be exercised without either.
fn wait_for_start_with(
    timeout_ms: u64,
    confirm_immediately: bool,
    is_running: impl Fn() -> bool,
    should_cancel: impl Fn() -> bool,
    sleep: impl Fn(std::time::Duration),
) -> StartWait {
    let attempts = std::cmp::max(1, timeout_ms / START_POLL_INTERVAL_MS);
    let mut confirmation = StartConfirmation::new();

    for _ in 0..attempts {
        // Checked before the first poll too: a cancellation that arrives while
        // the previous step was still running must not buy another full wait.
        if should_cancel() {
            return StartWait::Cancelled;
        }
        let running = is_running();
        if confirm_immediately && running {
            return StartWait::Started;
        }
        if !confirm_immediately && confirmation.observe(running) {
            return StartWait::Started;
        }
        sleep(std::time::Duration::from_millis(START_POLL_INTERVAL_MS));
    }

    if should_cancel() {
        return StartWait::Cancelled;
    }
    let running = is_running();
    let started = if confirm_immediately {
        running
    } else {
        confirmation.observe(running)
    };
    if started {
        StartWait::Started
    } else {
        StartWait::TimedOut
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn wait_for_process_start_pattern(pattern: &str, timeout_ms: u64) -> StartWait {
    wait_for_start_with(
        timeout_ms,
        true,
        || is_process_running_for_pattern(pattern),
        super::launch_cancel::launch_cancelled,
        std::thread::sleep,
    )
}

pub(crate) fn wait_for_process_start_patterns(patterns: &[String], timeout_ms: u64) -> StartWait {
    wait_for_start_with(
        timeout_ms,
        false,
        || is_process_running_for_patterns(patterns),
        super::launch_cancel::launch_cancelled,
        std::thread::sleep,
    )
}

pub(crate) fn wait_for_process_start(
    executable_path: &std::path::Path,
    timeout_ms: u64,
) -> StartWait {
    #[cfg(target_os = "macos")]
    {
        wait_for_process_start_patterns(
            &build_macos_process_match_patterns(executable_path),
            timeout_ms,
        )
    }

    #[cfg(not(target_os = "macos"))]
    {
        wait_for_process_start_pattern(&build_process_match_pattern(executable_path), timeout_ms)
    }
}

pub(crate) fn wait_for_process_exit_patterns(patterns: &[String], timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);
    for _ in 0..attempts {
        if !is_process_running_for_patterns(patterns) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }
    !is_process_running_for_patterns(patterns)
}

// ── Shared path utilities ─────────────────────────────────────────────────────

/// Walks up the path hierarchy to find the enclosing `.app` bundle, if any.
pub(crate) fn find_enclosing_app_bundle(path: &std::path::Path) -> Option<std::path::PathBuf> {
    path.ancestors().find_map(|ancestor| {
        let is_app_bundle = ancestor
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("app"))
            .unwrap_or(false);
        if is_app_bundle {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
}

#[cfg(all(test, target_os = "macos"))]
mod pgrep_self_match_tests {
    use super::*;

    /// A process whose command line contains `text`, alive until dropped.
    ///
    /// `sh -c '<script>' <arg0>` puts `arg0` on the command line without
    /// changing what runs, which is how we plant an exact string for `pgrep -f`
    /// to find — or, for the phantom case, to correctly ignore. The script has
    /// to be a loop rather than a single `sleep`: for a simple command the
    /// shell execs it directly and the planted `arg0` disappears with the shell.
    struct ProcessWithCommandLine(std::process::Child);

    impl ProcessWithCommandLine {
        fn spawn(text: &str) -> Self {
            let child = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg("while :; do sleep 1; done")
                .arg(text)
                .spawn()
                .unwrap();
            // Give the process table a moment to show it.
            std::thread::sleep(std::time::Duration::from_millis(400));
            ProcessWithCommandLine(child)
        }
    }

    impl Drop for ProcessWithCommandLine {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Each test needs an executable path no other test is holding a process
    /// open for: cargo runs them in parallel, and these assertions are about
    /// what is and is not present in the machine's process table.
    fn unique_executable_path(label: &str) -> String {
        format!(
            "/Steam/steamapps/common/{0}-{1}/{0}-{1}.exe",
            label,
            std::process::id()
        )
    }

    fn patterns_for(executable: &str) -> Vec<String> {
        vec![regex::escape(executable)]
    }

    /// The bug behind the PEAK log, stated as an invariant on the argument the
    /// poll actually runs: searching for the game must not advertise the game.
    ///
    /// While a launch waits for the game the UI polls for the same thing, so
    /// two `pgrep -f` calls overlap; when the pattern is on their own command
    /// lines they match each other. That is what reported PEAK as started four
    /// seconds in, while Steam was still booting, leaving the button to flip
    /// back to green when the phantom exited.
    #[test]
    fn the_argument_we_pass_to_pgrep_never_matches_itself() {
        for executable in [
            "/Steam/steamapps/common/PEAK/PEAK.exe",
            "PEAK.exe",
            "/Volumes/Feduzi/Giochi/Crossover/Bottles/Steam/drive_c/PEAK/PEAK.exe",
        ] {
            let pattern = regex::escape(executable);
            let needle = pgrep_pattern_for(&pattern);
            let compiled = regex::Regex::new(&needle).unwrap();

            assert!(
                compiled.is_match(&format!("wine {}", executable)),
                "{needle} must still find the game"
            );
            assert!(
                !compiled.is_match(&format!("pgrep -f {}", needle)),
                "{needle} must not match the command line of a poll using it"
            );
        }
    }

    /// A pattern that cannot be wrapped safely is left exactly as it was,
    /// rather than silently changing meaning.
    #[test]
    fn a_pattern_starting_with_an_unsafe_character_is_left_alone() {
        assert_eq!(pgrep_pattern_for("-flag"), "-flag");
        assert_eq!(pgrep_pattern_for(""), "");
    }

    /// The other half of the guarantee: a process that really is running the
    /// executable is still detected.
    #[test]
    fn the_real_game_process_is_still_detected() {
        let plain = unique_executable_path("RealGame");
        let _game = ProcessWithCommandLine::spawn(&plain);

        assert!(
            is_process_running_for_patterns(&patterns_for(&plain)),
            "a process actually running the executable must be detected"
        );
    }

    /// A phantom shaped exactly like one of our own concurrent polls must not
    /// be counted as the game.
    #[test]
    fn a_poll_shaped_process_is_not_mistaken_for_the_game() {
        let plain = unique_executable_path("PhantomPoll");
        let phantom = format!("pgrep -f {}", pgrep_pattern_for(&regex::escape(&plain)));
        let _concurrent_poll = ProcessWithCommandLine::spawn(&phantom);

        assert!(
            !is_process_running_for_patterns(&patterns_for(&plain)),
            "a poll of ours must not count as the game running"
        );
    }

    /// End to end, with the real code on both sides: nothing is running this
    /// executable, so any sighting is one poll seeing another.
    #[test]
    fn two_concurrent_polls_never_see_each_other() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let plain = unique_executable_path("ConcurrentPoll");
        let patterns = patterns_for(&plain);
        let stop = Arc::new(AtomicBool::new(false));

        let background = {
            let patterns = patterns.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = is_process_running_for_patterns(&patterns);
                }
            })
        };

        let sightings = (0..40)
            .filter(|_| is_process_running_for_patterns(&patterns))
            .count();

        stop.store(true, Ordering::Relaxed);
        background.join().unwrap();

        assert_eq!(sightings, 0, "a poll saw something, and nothing is running");
    }
}

#[cfg(test)]
mod start_confirmation_tests {
    use super::*;

    #[test]
    fn a_match_that_vanishes_never_confirms() {
        let mut confirmation = StartConfirmation::new();
        for _ in 0..20 {
            assert!(!confirmation.observe(true), "too soon to believe");
            assert!(!confirmation.observe(false), "gone again");
        }
    }

    #[test]
    fn a_match_that_stays_confirms_after_the_window() {
        let mut confirmation = StartConfirmation::new();
        assert!(
            !confirmation.observe(true),
            "the first sighting proves nothing"
        );
        assert!(confirmation.is_pending());

        std::thread::sleep(std::time::Duration::from_millis(
            START_CONFIRMATION_WINDOW_MS + 50,
        ));

        assert!(
            confirmation.observe(true),
            "a process that stayed is the game"
        );
    }

    #[test]
    fn an_absence_restarts_the_clock() {
        let mut confirmation = StartConfirmation::new();
        confirmation.observe(true);
        std::thread::sleep(std::time::Duration::from_millis(
            START_CONFIRMATION_WINDOW_MS + 50,
        ));
        confirmation.observe(false);

        assert!(!confirmation.is_pending());
        assert!(
            !confirmation.observe(true),
            "the window must be served again from scratch"
        );
    }
}

#[cfg(test)]
mod start_wait_cancellation_tests {
    use super::*;
    use std::cell::Cell;

    /// Nothing in these tests may actually sleep: a cancelled wait has to be
    /// quick, and a test that slept its way through a three-minute deadline
    /// would prove nothing about that.
    fn no_sleep(_: std::time::Duration) {}

    #[test]
    fn a_cancelled_wait_stops_at_once_instead_of_running_out_the_deadline() {
        let polls = Cell::new(0u32);

        let outcome = wait_for_start_with(
            180_000, // the cold-Steam deadline: three minutes of dead time
            false,
            || {
                polls.set(polls.get() + 1);
                false
            },
            || polls.get() >= 3, // the user presses the button again
            no_sleep,
        );

        assert_eq!(outcome, StartWait::Cancelled);
        assert!(
            polls.get() <= 4,
            "should stop within a poll of the cancellation, polled {} times",
            polls.get()
        );
    }

    #[test]
    fn a_cancellation_that_arrives_before_the_first_poll_is_honoured() {
        let outcome = wait_for_start_with(60_000, false, || false, || true, no_sleep);
        assert_eq!(outcome, StartWait::Cancelled);
    }

    #[test]
    fn a_game_that_starts_is_still_reported_as_started() {
        // Cancellation must not get in the way of the normal path.
        let outcome = wait_for_start_with(60_000, true, || true, || false, no_sleep);
        assert_eq!(outcome, StartWait::Started);
        assert!(outcome.started());
    }

    #[test]
    fn a_game_that_never_appears_still_times_out_rather_than_reporting_cancelled() {
        let outcome = wait_for_start_with(1_000, true, || false, || false, no_sleep);
        assert_eq!(outcome, StartWait::TimedOut);
        assert!(!outcome.started());
    }

    #[test]
    fn only_a_cancelled_wait_reports_the_message_the_frontend_watches_for() {
        // The frontend decides whether to show an error dialog by matching this
        // exact sentence (src/utils/launchIssue.ts), so the two sides only
        // agree by string — pin it on this side too.
        assert_eq!(
            StartWait::Cancelled.ok_unless_cancelled().unwrap_err(),
            "Launch cancelled."
        );
        assert!(StartWait::Started.ok_unless_cancelled().is_ok());
        // A timeout keeps its own wording: the caller explains what did not
        // start, which is a different thing to tell the user.
        assert!(StartWait::TimedOut.ok_unless_cancelled().is_ok());
    }

    #[test]
    fn a_game_that_starts_in_the_same_tick_as_the_cancellation_counts_as_cancelled() {
        // Deliberate: the user asked to stop waiting, and the running-state
        // poll picks the game up anyway if it really did come up. Reporting a
        // start here would leave the button lying about a launch the user
        // already called off.
        let outcome = wait_for_start_with(60_000, true, || true, || true, no_sleep);
        assert_eq!(outcome, StartWait::Cancelled);
    }
}

#[cfg(test)]
mod process_exclusion_tests {
    use super::{any_line_is_a_live_match, process_matches_excluding};

    /// Real `pgrep -fl` output from a cancelled CrossOver launch, once Steam
    /// had closed: all that is left are our own shutdown commands.
    const ONLY_OUR_SHUTDOWNS: &str = concat!(
        "13763 /Applications/CrossOver.app/Contents/SharedSupport/CrossOver/lib/wine/x86_64-windows/winewrapper.exe --run -- /Bottles/Steam/drive_c/Program Files (x86)/Steam/steam.exe -shutdown\n",
        "13766 C:\\Program Files (x86)\\Steam\\steam.exe -shutdown\n"
    );

    /// The same, with the client still up.
    const STEAM_STILL_UP: &str = concat!(
        "86276 C:\\Program Files (x86)\\Steam\\steam.exe -applaunch 1229490\n",
        "13766 C:\\Program Files (x86)\\Steam\\steam.exe -shutdown\n"
    );

    #[test]
    fn our_own_shutdown_commands_do_not_count_as_a_running_steam() {
        // They linger after Steam has closed and carry the same path, which is
        // what made the watch run its full window every time.
        assert!(!any_line_is_a_live_match(ONLY_OUR_SHUTDOWNS, "-shutdown"));
    }

    #[test]
    fn a_steam_that_is_still_up_is_still_seen() {
        assert!(any_line_is_a_live_match(STEAM_STILL_UP, "-shutdown"));
    }

    #[test]
    fn the_asking_pgrep_does_not_count_as_a_match() {
        let lines = "500 pgrep -fl steam\\.exe\n";
        assert!(!any_line_is_a_live_match(lines, "-shutdown"));
    }

    #[test]
    fn nothing_running_is_nothing_running() {
        assert!(!any_line_is_a_live_match("", "-shutdown"));
        assert!(!any_line_is_a_live_match("   \n", "-shutdown"));
    }

    #[test]
    fn the_candidate_rule_used_on_windows_excludes_the_same_thing() {
        let patterns = [regex::Regex::new("steam.exe").unwrap()];
        assert!(process_matches_excluding(
            &[r"C:\Program Files (x86)\Steam\steam.exe -applaunch 1229490".to_string()],
            &patterns,
            "-shutdown"
        ));
        assert!(!process_matches_excluding(
            &[r"C:\Program Files (x86)\Steam\steam.exe -shutdown".to_string()],
            &patterns,
            "-shutdown"
        ));
    }
}
