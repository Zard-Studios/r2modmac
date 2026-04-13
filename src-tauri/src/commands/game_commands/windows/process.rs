use super::*;

// ── Windows process pattern builders ─────────────────────────────────────────

pub(crate) fn map_native_path_to_wine_path(
    prefix_root: &std::path::Path,
    native_path: &std::path::Path,
) -> Option<String> {
    let canonical_native = canonicalize_or_original(native_path);
    let dosdevices_dir = prefix_root.join("dosdevices");
    if dosdevices_dir.is_dir() {
        let entries = fs::read_dir(&dosdevices_dir).ok()?;
        for entry in entries.filter_map(|entry| entry.ok()) {
            let device_name = entry.file_name().to_string_lossy().to_string();
            if device_name.len() != 2 || !device_name.ends_with(':') {
                continue;
            }

            let mapped_target = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            if !canonical_native.starts_with(&mapped_target) {
                continue;
            }

            let relative = canonical_native.strip_prefix(&mapped_target).ok()?;
            let mut wine_path = format!("{}\\", device_name.to_uppercase());
            let relative_path = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("\\");
            wine_path.push_str(&relative_path);
            return Some(wine_path.trim_end_matches('\\').to_string());
        }
    }

    let drive_c_root = prefix_root.join("drive_c");
    let canonical_drive_c = canonicalize_or_original(&drive_c_root);
    if canonical_native.starts_with(&canonical_drive_c) {
        let relative = canonical_native.strip_prefix(&canonical_drive_c).ok()?;
        let relative_path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\\");
        return Some(format!("C:\\{}", relative_path));
    }

    None
}

pub(crate) fn build_windows_process_match_patterns(
    executable_path: &std::path::Path,
) -> Vec<String> {
    let mut patterns = Vec::new();
    push_unique_pattern(&mut patterns, build_process_match_pattern(executable_path));

    if let Some(prefix_root) = find_wine_prefix_root(executable_path) {
        if let Some(windows_path) = map_native_path_to_wine_path(&prefix_root, executable_path) {
            push_unique_pattern(&mut patterns, regex::escape(&windows_path));
        }
    }

    if let Some(file_name) = executable_path.file_name().and_then(|value| value.to_str()) {
        push_unique_pattern(&mut patterns, regex::escape(file_name));
    }

    patterns
}

#[cfg(windows)]
pub(crate) fn unescape_regex_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut escaped = false;

    for ch in text.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }

    out
}

#[cfg(windows)]
pub(crate) fn windows_image_name_from_pattern(pattern: &str) -> Option<String> {
    let literal = unescape_regex_literal(pattern);
    let image_name = literal
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(literal.as_str())
        .trim_matches('"')
        .trim();

    if image_name.is_empty() {
        None
    } else {
        Some(image_name.to_string())
    }
}

#[cfg(windows)]
pub(crate) fn is_windows_process_running_tasklist(image_name: &str) -> bool {
    use std::os::windows::process::CommandExt;

    std::process::Command::new("tasklist")
        .creation_flags(0x08000000)
        .args([
            "/FI",
            &format!("IMAGENAME eq {}", image_name),
            "/NH",
            "/FO",
            "CSV",
        ])
        .output()
        .map(|out| {
            if !out.status.success() {
                return false;
            }
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            text.contains(&image_name.to_lowercase())
        })
        .unwrap_or(false)
}
