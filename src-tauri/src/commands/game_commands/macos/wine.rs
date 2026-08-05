use super::*;

fn push_unique_existing_path(paths: &mut Vec<std::path::PathBuf>, candidate: std::path::PathBuf) {
    if candidate.exists() && candidate.is_file() && !paths.contains(&candidate) {
        paths.push(candidate);
    }
}

fn find_named_file_under(
    root: &std::path::Path,
    names: &[&str],
    max_depth: usize,
) -> Option<std::path::PathBuf> {
    if !root.exists() || !root.is_dir() {
        return None;
    }

    let wanted: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_lowercase();

            if wanted.iter().any(|name| name == &file_name) && path.is_file() {
                return Some(path);
            }

            if depth < max_depth && entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push((path, depth + 1));
            }
        }
    }

    None
}

fn find_executables_in_path(names: &[&str]) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Some(path_var) = std::env::var_os("PATH") else {
        return found;
    };

    for dir in std::env::split_paths(&path_var) {
        for name in names {
            push_unique_existing_path(&mut found, dir.join(name));
        }
    }

    found
}

fn is_crossover_bottle(prefix_root: &std::path::Path) -> bool {
    prefix_root.join("cxbottle.conf").exists()
        || prefix_root
            .to_string_lossy()
            .to_lowercase()
            .contains("/crossover/bottles/")
}

fn is_crossover_runner(runner_path: &std::path::Path) -> bool {
    runner_path
        .to_string_lossy()
        .to_lowercase()
        .contains("crossover")
}

/// Find the Sikarugir/Wineskin wrapper `.app` bundle that contains the given Wine prefix or
/// executable. Returns the bundle path (e.g. `~/Applications/Sikarugir/Steam.app`) when found.
///
/// Sikarugir's real launch mechanism requires modifying `Contents/Info.plist` inside the bundle
/// and then invoking `open -n <bundle.app>` — the CLI launcher binary cannot be called directly
/// because `NSBundle.main()` would not resolve to the wrapper, producing `WineAppInitializationError`.
pub(crate) fn find_macos_wineskin_launcher_binary(
    prefix_root: Option<&std::path::Path>,
    executable_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let bundle_path = find_enclosing_app_bundle(executable_path)
        .or_else(|| prefix_root.and_then(find_enclosing_app_bundle))?;

    // Confirm there is a Sikarugir/Wineskin launcher binary inside Contents/MacOS/.
    // The file may be named "Sikarugir", "launcher", or "wineskinlauncher" (all symlink to the same binary).
    let macos_dir = bundle_path.join("Contents").join("MacOS");
    let has_launcher = [
        "Sikarugir",
        "launcher",
        "wineskinlauncher",
        "WineskinLauncher",
    ]
    .iter()
    .any(|name| macos_dir.join(name).exists());
    if !has_launcher {
        return None;
    }

    // Confirm the prefix belongs to this bundle.
    if let Some(prefix_root) = prefix_root {
        // Sikarugir uses Contents/SharedSupport/prefix as the WINEPREFIX,
        // and Contents/drive_c is a symlink to Contents/SharedSupport/prefix/drive_c.
        let bundle_prefix = bundle_path
            .join("Contents")
            .join("SharedSupport")
            .join("prefix");
        let canonical_prefix = canonicalize_or_original(prefix_root);
        let canonical_bundle_prefix = canonicalize_or_original(&bundle_prefix);
        let prefix_matches_bundle = canonical_prefix == canonical_bundle_prefix
            || canonical_prefix.starts_with(&canonical_bundle_prefix)
            || canonical_bundle_prefix.starts_with(&canonical_prefix);
        if !prefix_matches_bundle {
            return None;
        }
    }

    Some(bundle_path)
}

/// Convert a macOS-native path that lives under `prefix_root/drive_c/` into the corresponding
/// Windows `C:\...` path (with backslash separators) as expected by Sikarugir's `Info.plist`.
/// Sikarugir stores the value *without* the `C:` prefix in `Program Name and Path`, using a
/// leading `/` (e.g. `/Program Files (x86)/Steam/steam.exe`), so we return that form.
fn windows_rel_path_from_drive_c(
    prefix_root: &std::path::Path,
    native_path: &std::path::Path,
) -> Option<String> {
    // Resolve symlinks so that `Contents/drive_c` -> `Contents/SharedSupport/prefix/drive_c`
    // comparisons work correctly.
    let drive_c_root = canonicalize_or_original(&prefix_root.join("drive_c"));
    let canonical_native = canonicalize_or_original(native_path);
    if !canonical_native.starts_with(&drive_c_root) {
        return None;
    }

    let relative = canonical_native.strip_prefix(&drive_c_root).ok()?;
    let components: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();

    // Sikarugir Info.plist format: "/Program Files (x86)/Steam/steam.exe"
    // (forward slashes, leading slash, no "C:")
    Some(format!("/{}", components.join("/")))
}

/// Launch a Windows executable inside a Sikarugir/Wineskin wrapper.
///
/// `bundle_path` is the `.app` wrapper returned by `find_macos_wineskin_launcher_binary`.
/// `prefix_root` is the Wine prefix directory (typically `Contents/SharedSupport/prefix`).
/// `executable_path` is the native macOS path to the `.exe` inside `drive_c`.
/// `args` are additional command-line arguments (e.g. `["-applaunch", "1966720"]`).
///
/// ## Mechanism
/// Sikarugir reads the program to run from `Contents/Info.plist` keys:
///   - `Program Name and Path`: Windows path relative to `C:\` with forward slashes and a leading `/`
///     (e.g. `/Program Files (x86)/Steam/steam.exe`)
///   - `Program Flags`: space-separated argument string (e.g. `-applaunch 1966720`)
/// We temporarily overwrite these keys using `/usr/libexec/PlistBuddy`, call `open -n <bundle.app>`,
/// and restore the original values after a short delay. This is the only reliable method — invoking
/// the binary directly from the CLI causes `WineAppInitializationError` because `NSBundle.main()`
/// does not resolve to the wrapper.
pub(crate) fn launch_macos_wineskin_program(
    bundle_path: &std::path::Path,
    prefix_root: &std::path::Path,
    executable_path: &std::path::Path,
    args: &[String],
    working_dir: Option<&std::path::Path>,
    context: &str,
) -> Result<(), String> {
    let info_plist = bundle_path.join("Contents").join("Info.plist");
    if !info_plist.is_file() {
        return Err(format!(
            "Sikarugir wrapper has no Info.plist at {:?}",
            info_plist
        ));
    }

    // Map the native executable path to the Windows C:\ relative form expected by Info.plist.
    let win_path =
        windows_rel_path_from_drive_c(prefix_root, executable_path).ok_or_else(|| {
            format!(
                "Could not map {:?} to a Windows path inside the Sikarugir prefix {:?}",
                executable_path, prefix_root
            )
        })?;
    let win_flags = args.join(" ");

    eprintln!(
        "[{}] Launching via Sikarugir bundle {:?}: program={:?} flags={:?}",
        context, bundle_path, win_path, win_flags
    );

    // Read original values so we can restore them after launch.
    let plistbuddy = std::path::Path::new("/usr/libexec/PlistBuddy");
    if !plistbuddy.exists() {
        return Err(
            "PlistBuddy not found at /usr/libexec/PlistBuddy (required for Sikarugir launch)"
                .to_string(),
        );
    }

    let read_key = |key: &str| -> String {
        std::process::Command::new(plistbuddy)
            .arg("-c")
            .arg(format!("Print '{}'", key))
            .arg(&info_plist)
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .unwrap_or_default()
            .trim()
            .to_string()
    };

    let write_key = |key: &str, value: &str| -> Result<(), String> {
        let status = std::process::Command::new(plistbuddy)
            .arg("-c")
            .arg(format!("Set '{}' '{}'", key, value.replace('\'', "\\'")))
            .arg(&info_plist)
            .status()
            .map_err(|e| format!("PlistBuddy failed: {}", e))?;
        if !status.success() {
            return Err(format!("PlistBuddy could not set key '{}'", key));
        }
        Ok(())
    };

    let orig_program = read_key("Program Name and Path");
    let orig_flags = read_key("Program Flags");

    // Write launch configuration.
    write_key("Program Name and Path", &win_path)?;
    write_key("Program Flags", &win_flags)?;

    // Open the bundle as a new macOS application instance.
    let mut open_command = std::process::Command::new("open");
    open_command.arg("-n");
    if working_dir.is_some_and(|path| path.join("version.dll").is_file()) {
        open_command
            .arg("--env")
            .arg("WINEDLLOVERRIDES=version=n,b");
    }
    let open_status = open_command
        .arg(bundle_path)
        .status()
        .map_err(|e| format!("Failed to open Sikarugir bundle: {}", e))?;

    if !open_status.success() {
        // Restore before returning error.
        let _ = write_key("Program Name and Path", &orig_program);
        let _ = write_key("Program Flags", &orig_flags);
        return Err(format!(
            "'open -n {:?}' failed with status {}",
            bundle_path, open_status
        ));
    }

    // Restore the original Info.plist values in a background thread.
    // We wait a short time to ensure the launcher has already read the plist before we restore.
    let info_plist_clone = info_plist.clone();
    let orig_program_clone = orig_program.clone();
    let orig_flags_clone = orig_flags.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let _ = std::process::Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg(format!(
                "Set 'Program Name and Path' '{}'",
                orig_program_clone.replace('\'', "\\'")
            ))
            .arg(&info_plist_clone)
            .status();
        let _ = std::process::Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg(format!(
                "Set 'Program Flags' '{}'",
                orig_flags_clone.replace('\'', "\\'")
            ))
            .arg(&info_plist_clone)
            .status();
    });

    Ok(())
}

pub(crate) fn find_macos_compat_runner_binary(
    prefix_root: Option<&std::path::Path>,
    executable_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();

    if let Some(prefix_root) = prefix_root {
        for relative in ["bin/wine", "wine/bin/wine", "bin/wine64", "wine/bin/wine64"] {
            push_unique_existing_path(&mut candidates, prefix_root.join(relative));
        }
    }

    if let Some(bundle_path) = find_enclosing_app_bundle(executable_path)
        .or_else(|| prefix_root.and_then(find_enclosing_app_bundle))
    {
        for relative in [
            "Contents/Frameworks/wswine.bundle/bin/wine",
            "Contents/Resources/wine/bin/wine",
            "Contents/SharedSupport/wine/bin/wine",
            "Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine",
            "Contents/Frameworks/wswine.bundle/bin/wine64",
            "Contents/Resources/wine/bin/wine64",
            "Contents/SharedSupport/wine/bin/wine64",
            "Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine64",
        ] {
            push_unique_existing_path(&mut candidates, bundle_path.join(relative));
        }

        if let Some(found) =
            find_named_file_under(&bundle_path.join("Contents"), &["wine", "wine64"], 6)
        {
            push_unique_existing_path(&mut candidates, found);
        }
    }

    if let Some(home) = dirs::home_dir() {
        for app_path in [
            std::path::PathBuf::from("/Applications/CrossOver.app"),
            home.join("Applications").join("CrossOver.app"),
        ] {
            push_unique_existing_path(
                &mut candidates,
                app_path.join("Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine"),
            );
            push_unique_existing_path(
                &mut candidates,
                app_path
                    .join("Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine64"),
            );
        }

        for root in [
            home.join("Library/Application Support/heroic/tools"),
            home.join("Library/Application Support/Whisky"),
        ] {
            if let Some(found) = find_named_file_under(&root, &["wine", "wine64"], 6) {
                push_unique_existing_path(&mut candidates, found);
            }
        }

        for app_root in [
            std::path::PathBuf::from("/Applications"),
            home.join("Applications"),
        ] {
            let entries = match fs::read_dir(&app_root) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.filter_map(|entry| entry.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if !path.is_dir() || !name.ends_with(".app") {
                    continue;
                }
                if ![
                    "crossover",
                    "wine",
                    "wineskin",
                    "sikarugir",
                    "kegworks",
                    "whisky",
                    "heroic",
                ]
                .iter()
                .any(|keyword| name.contains(keyword))
                {
                    continue;
                }
                if let Some(found) =
                    find_named_file_under(&path.join("Contents"), &["wine", "wine64"], 6)
                {
                    push_unique_existing_path(&mut candidates, found);
                }
            }
        }
    }

    for path in find_executables_in_path(&["wine", "wine64"]) {
        push_unique_existing_path(&mut candidates, path);
    }

    for candidate in [
        "/opt/homebrew/bin/wine",
        "/usr/local/bin/wine",
        "/opt/homebrew/bin/wine64",
        "/usr/local/bin/wine64",
    ] {
        push_unique_existing_path(&mut candidates, std::path::PathBuf::from(candidate));
    }

    candidates.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "r2modmac_{}_{}_{}",
            name,
            std::process::id(),
            nanos
        ))
    }

    fn touch(path: &std::path::Path) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, b"").expect("write file");
    }

    #[test]
    fn detects_wineskin_launcher_from_sikarugir_prefix() {
        let root = unique_temp_dir("sikarugir_launcher");
        let wrapper = root.join("SteamWin.app");
        // Sikarugir uses "Sikarugir" or "launcher" (not "WineskinLauncher" capitalised) —
        // but any of the known names should be accepted.
        let launcher_bin = wrapper.join("Contents/MacOS/Sikarugir");
        let prefix = wrapper.join("Contents/SharedSupport/prefix");
        let steam_exe = prefix.join("drive_c/Program Files (x86)/Steam/steam.exe");
        touch(&launcher_bin);
        touch(&steam_exe);

        // The function now returns the *bundle* path, not the binary path.
        let detected = find_macos_wineskin_launcher_binary(Some(&prefix), &steam_exe);
        assert_eq!(detected.as_deref(), Some(wrapper.as_path()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn maps_native_path_to_sikarugir_info_plist_format() {
        let root = unique_temp_dir("sikarugir_path");
        let prefix = root.join("SteamWin.app/Contents/SharedSupport/prefix");
        let steam_exe = prefix.join("drive_c/Program Files (x86)/Steam/steam.exe");
        touch(&steam_exe);

        let win_path = windows_rel_path_from_drive_c(&prefix, &steam_exe);
        // Sikarugir Info.plist format: "/Program Files (x86)/Steam/steam.exe" (forward slashes,
        // leading slash, no "C:" prefix)
        assert_eq!(
            win_path.as_deref(),
            Some("/Program Files (x86)/Steam/steam.exe")
        );

        let _ = fs::remove_dir_all(root);
    }
}

pub(crate) fn configure_macos_compat_runner_command(
    command: &mut std::process::Command,
    runner_path: &std::path::Path,
    prefix_root: Option<&std::path::Path>,
) -> Result<(), String> {
    let use_crossover_bottle_mode = prefix_root
        .map(|prefix_root| is_crossover_bottle(prefix_root) && is_crossover_runner(runner_path))
        .unwrap_or(false);

    if use_crossover_bottle_mode {
        let bottle_path = prefix_root
            .ok_or_else(|| "CrossOver bottle path could not be determined.".to_string())?;
        command.arg("--bottle").arg(bottle_path);
        eprintln!(
            "[compat_runner] Using CrossOver bottle {:?} with runner {:?}",
            bottle_path, runner_path
        );
    } else if let Some(prefix_root) = prefix_root {
        command.env("WINEPREFIX", prefix_root);
        eprintln!(
            "[compat_runner] Using Wine prefix {:?} with runner {:?}",
            prefix_root, runner_path
        );
    } else {
        eprintln!(
            "[compat_runner] Using runner {:?} without explicit prefix",
            runner_path
        );
    }

    Ok(())
}
