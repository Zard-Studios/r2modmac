use super::*;

pub(crate) fn is_macos_app_bundle_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".app"))
        .unwrap_or(false)
}

pub(crate) fn macos_app_bundle_score(app_bundle: &std::path::Path) -> i32 {
    let mut score = 0;
    let contents = app_bundle.join("Contents");
    if contents.join("MacOS").is_dir() {
        score += 100;
    }
    if contents.join("Resources").join("Data").is_dir() {
        score += 200;
    }
    if contents.join("Info.plist").is_file() || contents.join("Info").is_file() {
        score += 20;
    }
    score
}

pub(crate) fn find_macos_app_bundles_in_dir(
    root: &std::path::Path,
    max_depth: usize,
) -> Vec<(std::path::PathBuf, usize)> {
    let mut found = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0usize));

    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }

            if is_macos_app_bundle_path(&path) {
                found.push((path, depth + 1));
                continue;
            }

            if depth < max_depth {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if matches!(
                    name.as_str(),
                    "bepinex" | "doorstop_libs" | "plugins" | "__macosx"
                ) {
                    continue;
                }
                queue.push_back((path, depth + 1));
            }
        }
    }

    found
}

pub(crate) fn find_macos_app_bundle(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<(std::path::PathBuf, usize, i32)> = Vec::new();
    let mut push_candidate = |candidate: std::path::PathBuf, depth: usize| {
        if candidates
            .iter()
            .any(|(existing, _, _)| *existing == candidate)
        {
            return;
        }
        let score = macos_app_bundle_score(&candidate);
        candidates.push((candidate, depth, score));
    };

    if is_macos_app_bundle_path(game_path) {
        push_candidate(game_path.to_path_buf(), 0);
    }

    let is_contents_dir = game_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("Contents"))
        .unwrap_or(false);
    if is_contents_dir {
        if let Some(parent) = game_path.parent() {
            if is_macos_app_bundle_path(parent) {
                push_candidate(parent.to_path_buf(), 0);
            }
        }
    }

    if game_path.is_dir() {
        for (bundle, depth) in find_macos_app_bundles_in_dir(game_path, 4) {
            push_candidate(bundle, depth);
        }
    }

    candidates.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    candidates.first().map(|(bundle, _, _)| bundle.clone())
}

pub(crate) fn find_macos_launch_bundle(game_path: &std::path::Path) -> Option<std::path::PathBuf> {
    // If the stored game path already lives inside an app bundle (for example
    // `/Applications/Foo.app/Contents`), launch that enclosing bundle instead
    // of a nested child app. Some standalone macOS wrappers prepare runtime
    // state before handing off to the real game executable.
    find_enclosing_app_bundle(game_path).or_else(|| find_macos_app_bundle(game_path))
}

pub(crate) fn is_steam_bundle_path(path: &std::path::Path) -> bool {
    let lower = canonicalize_or_original(path)
        .to_string_lossy()
        .to_lowercase();
    lower.ends_with("/steam.app")
        || lower.contains("/steam.app/")
        || lower.ends_with("/steam.appbundle/steam")
        || lower.contains("/steam.appbundle/steam/")
}

pub(crate) fn find_macos_wrapper_launcher_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let launch_bundle = find_macos_launch_bundle(game_path)?;
    let macos_dir = launch_bundle.join("Contents").join("MacOS");
    let load_script = macos_dir.join("load");
    let steam_appid = macos_dir.join("steam_appid.txt");
    let ipcserver = macos_dir.join("ipcserver");

    if load_script.is_file() && steam_appid.is_file() && ipcserver.exists() {
        Some(load_script)
    } else {
        None
    }
}

fn read_cf_bundle_executable_name(app_bundle: &std::path::Path) -> Option<String> {
    fn extract_from_plist_text(plist_text: &str) -> Option<String> {
        let key = "<key>CFBundleExecutable</key>";
        let key_pos = plist_text.find(key)?;
        let after_key = &plist_text[key_pos + key.len()..];
        let string_start = after_key.find("<string>")?;
        let value_start = string_start + "<string>".len();
        let value_end = after_key[value_start..].find("</string>")? + value_start;
        let value = after_key[value_start..value_end].trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    let plist_candidates = [
        app_bundle.join("Contents").join("Info.plist"),
        app_bundle.join("Contents").join("Info"),
    ];

    for plist_path in &plist_candidates {
        if let Ok(plist_text) = fs::read_to_string(plist_path) {
            if let Some(value) = extract_from_plist_text(&plist_text) {
                return Some(value);
            }
        }
    }

    for plist_path in &plist_candidates {
        let Ok(output) = std::process::Command::new("/usr/bin/defaults")
            .args(["read", &plist_path.to_string_lossy(), "CFBundleExecutable"])
            .output()
        else {
            continue;
        };

        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }

    None
}

fn macos_file_has_shebang(path: &std::path::Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut prefix = [0u8; 2];
    std::io::Read::read_exact(&mut file, &mut prefix).is_ok() && prefix == [b'#', b'!']
}

fn macos_file_has_macho_magic(path: &std::path::Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    if std::io::Read::read_exact(&mut file, &mut magic).is_err() {
        return false;
    }

    matches!(
        magic,
        [0xFE, 0xED, 0xFA, 0xCE]
            | [0xCE, 0xFA, 0xED, 0xFE]
            | [0xFE, 0xED, 0xFA, 0xCF]
            | [0xCF, 0xFA, 0xED, 0xFE]
            | [0xCA, 0xFE, 0xBA, 0xBE]
            | [0xBE, 0xBA, 0xFE, 0xCA]
            | [0xCA, 0xFE, 0xBA, 0xBF]
            | [0xBF, 0xBA, 0xFE, 0xCA]
    )
}

fn macos_executable_candidate_score(
    candidate: &std::path::Path,
    expected_name: Option<&str>,
) -> i32 {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let mut score = 0;
    let file_name = candidate
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let lower_name = file_name.to_lowercase();

    if expected_name.is_some_and(|expected| expected.eq_ignore_ascii_case(file_name)) {
        score += 250;
    }

    if candidate
        .extension()
        .and_then(|value| value.to_str())
        .map(|ext| ext.is_empty())
        .unwrap_or(true)
    {
        score += 40;
    }

    #[cfg(unix)]
    if fs::metadata(candidate)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
    {
        score += 80;
    }

    if macos_file_has_macho_magic(candidate) {
        score += 500;
    }

    if macos_file_has_shebang(candidate) {
        score -= 250;
    }

    if matches!(
        candidate.extension().and_then(|value| value.to_str()),
        Some("sh" | "command" | "py" | "txt" | "plist")
    ) {
        score -= 300;
    }

    if matches!(lower_name.as_str(), "load" | "reset" | "ipcserver") {
        score -= 400;
    }

    if lower_name.contains("launcher")
        || lower_name.contains("bootstrap")
        || lower_name.contains("helper")
        || lower_name.contains("crash")
    {
        score -= 150;
    }

    score
}

fn find_best_macos_executable_in_dir(
    macos_dir: &std::path::Path,
    expected_name: Option<&str>,
) -> Option<std::path::PathBuf> {
    let mut candidates = fs::read_dir(macos_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|entry| {
            let path = entry.path();
            let score = macos_executable_candidate_score(&path, expected_name);
            (path, score)
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    candidates.into_iter().next().map(|(path, _)| path)
}

pub(crate) fn find_macos_executable_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let app_bundle = find_macos_app_bundle(game_path)?;
    let macos_dir = app_bundle.join("Contents").join("MacOS");
    if !macos_dir.is_dir() {
        return None;
    }

    let expected_name = read_cf_bundle_executable_name(&app_bundle);
    find_best_macos_executable_in_dir(&macos_dir, expected_name.as_deref())
}

pub(crate) fn resolve_macos_runtime_root(game_path: &std::path::Path) -> std::path::PathBuf {
    let Some(app_bundle) = find_macos_app_bundle(game_path) else {
        return game_path.to_path_buf();
    };
    let Some(parent_dir) = app_bundle.parent() else {
        return game_path.to_path_buf();
    };

    if parent_dir != game_path && parent_dir.starts_with(game_path) {
        parent_dir.to_path_buf()
    } else {
        game_path.to_path_buf()
    }
}

pub(crate) fn macos_executable_supports_x86_64(
    executable_path: &std::path::Path,
) -> Result<bool, String> {
    let output = std::process::Command::new("/usr/bin/lipo")
        .args(["-archs", &executable_path.to_string_lossy()])
        .output()
        .map_err(|e| format!("Failed to inspect macOS executable architectures: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to inspect macOS executable architectures: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let archs = String::from_utf8_lossy(&output.stdout).to_lowercase();
    Ok(archs.split_whitespace().any(|arch| arch == "x86_64"))
}

pub(crate) fn validate_macos_bepinex_support(game_path: &std::path::Path) -> Result<(), String> {
    let Some(app_bundle) = find_macos_app_bundle(game_path) else {
        if find_pe_game_executable_path(game_path).is_some() {
            return Err(
                "This looks like a non-native compatibility build. Use a compatibility-tool profile instead of native macOS BepInEx."
                    .to_string(),
            );
        }

        return Err(
            "macOS mod support currently requires a native macOS .app bundle inside the game directory."
                .to_string(),
        );
    };

    let data_dir = app_bundle.join("Contents").join("Resources").join("Data");
    if !data_dir.is_dir() {
        return Err(
            "Could not find a Unity Data folder inside the macOS app bundle. Native macOS BepInEx currently supports Unity .app builds only."
                .to_string(),
        );
    }

    let executable_path = find_macos_executable_path(game_path).ok_or_else(|| {
        "Could not find the macOS game executable inside the app bundle.".to_string()
    })?;

    if !macos_executable_supports_x86_64(&executable_path)? {
        return Err(
            "This macOS build is arm64-only. Current BepInEx macOS runtimes are x64-only, so this game cannot be launched modded natively on macOS right now. If a compatibility-tool build is moddable, use a compatibility-tool profile."
                .to_string(),
        );
    }

    Ok(())
}

pub(crate) fn resolve_macos_app_executable_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    find_macos_executable_path(game_path)
}

pub(crate) fn macho_file_supports_arm64(path: &std::path::Path) -> bool {
    let Ok(output) = std::process::Command::new("/usr/bin/file")
        .arg("-b")
        .arg(path)
        .output()
    else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    String::from_utf8_lossy(&output.stdout)
        .to_lowercase()
        .contains("arm64")
}

pub(crate) fn is_apple_silicon_host() -> bool {
    if std::env::consts::ARCH == "aarch64" {
        return true;
    }

    let Ok(output) = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-in", "sysctl.proc_translated"])
        .output()
    else {
        return false;
    };

    output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1"
}

pub(crate) fn is_macos_bundle_signature_valid(bundle_path: &std::path::Path) -> bool {
    std::process::Command::new("codesign")
        .args(["-v", "--strict", &bundle_path.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn ad_hoc_sign_macos_bundle(bundle_path: &std::path::Path) -> bool {
    std::process::Command::new("codesign")
        .args([
            "--force",
            "--deep",
            "-s",
            "-",
            &bundle_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn clear_macos_bundle_quarantine(bundle_path: &std::path::Path) {
    let recursive_ok = std::process::Command::new("xattr")
        .args([
            "-dr",
            "com.apple.quarantine",
            &bundle_path.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if !recursive_ok {
        let _ = std::process::Command::new("xattr")
            .args(["-c", &bundle_path.to_string_lossy()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub(crate) fn should_use_native_macos_bepinex_launcher(game_path: &std::path::Path) -> bool {
    if !is_apple_silicon_host() {
        return false;
    }

    let runtime_root = resolve_macos_runtime_root(game_path);
    let Some(executable_path) = resolve_macos_app_executable_path(game_path) else {
        return false;
    };

    let root_doorstop = runtime_root.join("libdoorstop.dylib");
    root_doorstop.is_file()
        && macho_file_supports_arm64(&executable_path)
        && macho_file_supports_arm64(&root_doorstop)
}

// ── macOS process pattern builders ───────────────────────────────────────────

#[cfg(target_os = "macos")]
pub(crate) fn push_macos_path_process_patterns(
    patterns: &mut Vec<String>,
    executable_path: &std::path::Path,
) {
    let mut push_path = |path_text: String| {
        push_unique_pattern(patterns, regex::escape(&path_text));
        for (from, to) in [
            ("/Contents/game/", "/Contents/Game/"),
            ("/Contents/Game/", "/Contents/game/"),
        ] {
            if path_text.contains(from) {
                push_unique_pattern(patterns, regex::escape(&path_text.replace(from, to)));
            }
        }
    };

    push_path(executable_path.to_string_lossy().to_string());

    let canonical = canonicalize_or_original(executable_path);
    if canonical != executable_path {
        push_path(canonical.to_string_lossy().to_string());
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn build_macos_process_match_patterns(executable_path: &std::path::Path) -> Vec<String> {
    let mut patterns = Vec::new();
    push_macos_path_process_patterns(&mut patterns, executable_path);

    // macOS app processes may expose only the CFBundleExecutable name in ps/pgrep.
    if let Some(file_name) = executable_path.file_name().and_then(|value| value.to_str()) {
        push_unique_pattern(&mut patterns, regex::escape(file_name));
    }

    patterns
}

#[cfg(target_os = "macos")]
pub(crate) fn build_macos_process_kill_patterns(executable_path: &std::path::Path) -> Vec<String> {
    let mut patterns = Vec::new();
    push_macos_path_process_patterns(&mut patterns, executable_path);
    patterns
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_temp_dir(label: &str) -> std::path::PathBuf {
        let unique = format!(
            "r2modmac-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(path: &std::path::Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).unwrap();
        }
    }

    #[test]
    fn prefers_real_binary_over_shell_wrapper_candidate() {
        let dir = create_temp_dir("macos-exec-wrapper");
        let macos_dir = dir.join("Contents").join("MacOS");
        fs::create_dir_all(&macos_dir).unwrap();

        let wrapper = macos_dir.join("load");
        let binary = macos_dir.join("Valheim");
        write_file(&wrapper, b"#!/bin/sh\nexit 0\n");
        write_file(&binary, &[0xCF, 0xFA, 0xED, 0xFE, 0, 0, 0, 0]);

        let selected = find_best_macos_executable_in_dir(&macos_dir, Some("load")).unwrap();
        assert_eq!(selected, binary);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prefers_expected_bundle_executable_when_it_is_binary() {
        let dir = create_temp_dir("macos-exec-expected");
        let macos_dir = dir.join("Contents").join("MacOS");
        fs::create_dir_all(&macos_dir).unwrap();

        let expected = macos_dir.join("Silksong");
        let helper = macos_dir.join("bootstrap_helper");
        write_file(&expected, &[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 0]);
        write_file(&helper, b"#!/bin/sh\nexit 0\n");

        let selected = find_best_macos_executable_in_dir(&macos_dir, Some("Silksong")).unwrap();
        assert_eq!(selected, expected);

        let _ = fs::remove_dir_all(dir);
    }
}
