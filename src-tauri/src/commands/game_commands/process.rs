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
                    eprintln!(
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
            let plain = pattern.replace(r"\/", "/").replace(r"\.", ".");
            let status = Command::new("pgrep").arg("-f").arg(&plain).output();
            if let Ok(output) = status {
                if output.status.success() {
                    let matched_text = String::from_utf8_lossy(&output.stdout);
                    eprintln!(
                        "[is_process_running_for_patterns] Pattern {:?} MATCHED. pgrep output:\n{}",
                        pattern, matched_text
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
