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

// ── Process polling helpers ───────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
pub(crate) fn wait_for_process_start_pattern(pattern: &str, timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);
    for _ in 0..attempts {
        if is_process_running_for_pattern(pattern) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }
    is_process_running_for_pattern(pattern)
}

pub(crate) fn wait_for_process_start_patterns(patterns: &[String], timeout_ms: u64) -> bool {
    let poll_interval = 250u64;
    let attempts = std::cmp::max(1, timeout_ms / poll_interval);
    for _ in 0..attempts {
        if is_process_running_for_patterns(patterns) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_interval));
    }
    is_process_running_for_patterns(patterns)
}

pub(crate) fn wait_for_process_start(executable_path: &std::path::Path, timeout_ms: u64) -> bool {
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
