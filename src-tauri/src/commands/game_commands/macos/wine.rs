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
/// We temporarily overwrite these keys using `/usr/bin/plutil`, call `open -n <bundle.app>`,
/// and restore the original values after a short delay. This is the only reliable method — invoking
/// the binary directly from the CLI causes `WineAppInitializationError` because `NSBundle.main()`
/// does not resolve to the wrapper.
fn read_plist_string(info_plist: &std::path::Path, key: &str) -> String {
    std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw"])
        .arg(info_plist)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

fn write_plist_string(info_plist: &std::path::Path, key: &str, value: &str) -> Result<(), String> {
    // Do not use PlistBuddy's `-c "Set ..."` form here. It parses the value as
    // part of its own command language and strips the quotes and backslashes
    // from Windows arguments such as `"C:\\Program Files\\..."`.
    let status = std::process::Command::new("/usr/bin/plutil")
        .args(["-replace", key, "-string", value])
        .arg(info_plist)
        .status()
        .map_err(|error| format!("plutil failed: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("plutil could not set key '{key}'"))
    }
}

/// The `user.reg` text with these DLLs set to load the game's own copy first,
/// or `None` when it already does.
///
/// A BepInEx doorstop (`winhttp.dll`) and the other loader proxies only run
/// when Wine prefers the native file over its builtin. Handing that over as
/// `WINEDLLOVERRIDES` to a wrapper launch reaches Steam at best, not the game
/// Steam later starts; the bottle's own registry is what every process in it
/// reads, which is also where `winecfg` keeps it.
fn with_native_dll_overrides(content: &str, dlls: &[String], now: u64) -> Option<String> {
    const HEADER: &str = "[software\\\\wine\\\\dlloverrides]";
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let header = lines
        .iter()
        .position(|line| line.trim_start().to_ascii_lowercase().starts_with(HEADER));
    let mut changed = false;

    let header = match header {
        Some(index) => index,
        None => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[Software\\\\Wine\\\\DllOverrides] {now}"));
            changed = true;
            lines.len() - 1
        }
    };
    let section_end = lines[header + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map(|offset| header + 1 + offset)
        .unwrap_or(lines.len());

    let mut insert_at = header + 1;
    while insert_at < section_end && lines[insert_at].trim_start().starts_with('#') {
        insert_at += 1;
    }
    let mut section_end = section_end;
    for dll in dlls {
        let key = format!("\"{}\"=", dll.to_ascii_lowercase());
        let wanted = format!("\"{}\"=\"native,builtin\"", dll);
        match (header + 1..section_end).find(|&index| {
            lines[index]
                .trim_start()
                .to_ascii_lowercase()
                .starts_with(&key)
        }) {
            Some(index) => {
                let value = lines[index]
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or("");
                if !value.trim().trim_matches('"').starts_with("native") {
                    lines[index] = wanted;
                    changed = true;
                }
            }
            None => {
                lines.insert(insert_at, wanted);
                section_end += 1;
                changed = true;
            }
        }
    }

    changed.then(|| {
        let mut text = lines.join(newline);
        text.push_str(newline);
        text
    })
}

/// Make the bottle load these proxy DLLs from the game folder, through its
/// registry. Returns whether `user.reg` was changed. The first change keeps a
/// copy of the original beside it.
pub(crate) fn ensure_native_dll_overrides(
    prefix_root: &std::path::Path,
    overrides: &str,
) -> Result<bool, String> {
    let dlls = overrides
        .split(';')
        .filter_map(|entry| {
            entry
                .split_once('=')
                .map(|(name, _)| name.trim().to_string())
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    let user_reg = prefix_root.join("user.reg");
    if dlls.is_empty() || !user_reg.is_file() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&user_reg).map_err(|error| error.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let Some(updated) = with_native_dll_overrides(&content, &dlls, now) else {
        return Ok(false);
    };
    let backup = prefix_root.join("user.reg.r2modmac-backup");
    if !backup.exists() {
        std::fs::copy(&user_reg, &backup).map_err(|error| error.to_string())?;
    }
    let staged = prefix_root.join("user.reg.r2modmac-staging");
    std::fs::write(&staged, updated).map_err(|error| error.to_string())?;
    std::fs::rename(&staged, &user_reg).map_err(|error| error.to_string())?;
    log::info!(
        "[wine] Set native DLL overrides {:?} in {}",
        dlls,
        user_reg.display()
    );
    Ok(true)
}

/// The wrapper's own launch settings, kept on disk while a launch has them
/// replaced. Reading them back from the plist instead is what broke wrappers:
/// a second launch inside the restore window read the first launch's values as
/// the originals and restored those, so the wrapper kept opening the game with
/// r2modmac's flags, or with no program at all when the read failed.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct WrapperLaunchSettings {
    program: String,
    flags: String,
}

fn wrapper_settings_sidecar(info_plist: &std::path::Path) -> std::path::PathBuf {
    info_plist.with_file_name("Info.plist.r2modmac-original.json")
}

fn original_wrapper_settings(
    info_plist: &std::path::Path,
) -> Result<WrapperLaunchSettings, String> {
    let sidecar = wrapper_settings_sidecar(info_plist);
    if let Some(saved) = std::fs::read(&sidecar)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<WrapperLaunchSettings>(&bytes).ok())
        .filter(|saved| !saved.program.trim().is_empty())
    {
        return Ok(saved);
    }
    let current = WrapperLaunchSettings {
        program: read_plist_string(info_plist, "Program Name and Path"),
        flags: read_plist_string(info_plist, "Program Flags"),
    };
    if current.program.trim().is_empty() {
        return Err(format!(
            "Could not read the program this wrapper normally opens from {:?}; not launching, so its settings are not lost",
            info_plist
        ));
    }
    let bytes = serde_json::to_vec_pretty(&current).map_err(|error| error.to_string())?;
    std::fs::write(&sidecar, bytes).map_err(|error| error.to_string())?;
    Ok(current)
}

fn restore_wrapper_settings(info_plist: &std::path::Path, original: &WrapperLaunchSettings) {
    let restored = write_plist_string(info_plist, "Program Name and Path", &original.program)
        .and_then(|()| write_plist_string(info_plist, "Program Flags", &original.flags));
    match restored {
        Ok(()) => {
            let _ = std::fs::remove_file(wrapper_settings_sidecar(info_plist));
        }
        Err(error) => log::warn!(
            "[wine] Could not restore the wrapper settings in {:?}: {}. They stay saved beside it and are restored on the next launch.",
            info_plist,
            error
        ),
    }
}

pub(crate) fn launch_macos_wineskin_program(
    bundle_path: &std::path::Path,
    prefix_root: &std::path::Path,
    executable_path: &std::path::Path,
    args: &[String],
    working_dir: Option<&std::path::Path>,
    proxy_dir: Option<&std::path::Path>,
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
    let win_flags = serialize_windows_arguments(args);

    log::info!(
        "[{}] Launching via Sikarugir bundle {:?}: program={:?} flags={:?}",
        context,
        bundle_path,
        win_path,
        win_flags
    );

    // Read original values so we can restore them after launch.
    let plutil = std::path::Path::new("/usr/bin/plutil");
    if !plutil.exists() {
        return Err(
            "plutil not found at /usr/bin/plutil (required for Sikarugir launch)".to_string(),
        );
    }

    let original = original_wrapper_settings(&info_plist)?;

    // Write launch configuration.
    write_plist_string(&info_plist, "Program Name and Path", &win_path)?;
    write_plist_string(&info_plist, "Program Flags", &win_flags)?;

    // Open the bundle as a new macOS application instance.
    let mut open_command = std::process::Command::new("open");
    open_command.arg("-n");
    // The loader's proxy DLL has to win over Wine's builtin, and its name comes
    // from the pack the game uses (version.dll, d3d12.dll for Hades II, ...).
    // Shimloader's proxy sits with the game executable rather than in the
    // working directory the launch uses, so it is named separately.
    if let Some(overrides) = proxy_dir
        .or(working_dir)
        .and_then(crate::models::loaders::wine_dll_override_value)
    {
        if let Err(error) = ensure_native_dll_overrides(prefix_root, &overrides) {
            log::warn!(
                "[{}] Could not record the DLL overrides in the bottle registry: {}",
                context,
                error
            );
        }
        open_command
            .arg("--env")
            .arg(format!("WINEDLLOVERRIDES={overrides}"));
    }
    let open_status = open_command
        .arg(bundle_path)
        .status()
        .map_err(|e| format!("Failed to open Sikarugir bundle: {}", e))?;

    if !open_status.success() {
        // Restore before returning error.
        restore_wrapper_settings(&info_plist, &original);
        return Err(format!(
            "'open -n {:?}' failed with status {}",
            bundle_path, open_status
        ));
    }

    // Restore the original Info.plist values in a background thread.
    // We wait a short time to ensure the launcher has already read the plist before we restore.
    let info_plist_clone = info_plist.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        restore_wrapper_settings(&info_plist_clone, &original);
    });

    Ok(())
}

/// Activate the already-running wrapper after its Windows game process is
/// observed. The initial `open -n` foregrounds the wrapper launcher, but Steam
/// may create the actual game window much later and Wine can leave that child
/// behind r2modmac. A second ordinary `open` activates the existing app; it
/// deliberately omits `-n`, so it cannot create another Steam/game instance.
pub(crate) fn activate_macos_wineskin_bundle(bundle_path: &std::path::Path) -> Result<(), String> {
    let status = macos_wineskin_activation_command(bundle_path)
        .status()
        .map_err(|error| format!("Failed to activate Sikarugir bundle: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not bring Sikarugir bundle {:?} to the foreground (status {})",
            bundle_path, status
        ))
    }
}

fn macos_wineskin_activation_command(bundle_path: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new("open");
    command.arg(bundle_path);
    command
}

/// Serialize argv for Sikarugir's single `Program Flags` string using the
/// quoting rules consumed by CommandLineToArgvW. Paths inside Steam prefixes
/// commonly contain spaces, parentheses, and trailing backslashes.
fn serialize_windows_arguments(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty()
        && !argument
            .chars()
            .any(|character| character.is_whitespace() || character == '"')
    {
        return argument.to_string();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
            quoted.push('"');
        } else {
            quoted.push_str(&"\\".repeat(backslashes));
            quoted.push(character);
        }
        backslashes = 0;
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
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

    #[test]
    fn quotes_sikarugir_program_flags_without_splitting_hades_paths() {
        let arguments = vec![
            "-applaunch".to_string(),
            "1145350".to_string(),
            "--rom_modding_root_folder".to_string(),
            "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Hades II\\Ship".to_string(),
        ];
        assert_eq!(
            serialize_windows_arguments(&arguments),
            "-applaunch 1145350 --rom_modding_root_folder \"C:\\Program Files (x86)\\Steam\\steamapps\\common\\Hades II\\Ship\""
        );
    }

    #[test]
    fn quotes_empty_embedded_quote_and_trailing_backslash_arguments() {
        assert_eq!(quote_windows_argument(""), "\"\"");
        assert_eq!(quote_windows_argument("plain"), "plain");
        assert_eq!(quote_windows_argument("a\\\"b"), "\"a\\\\\\\"b\"");
        assert_eq!(
            quote_windows_argument("C:\\space here\\"),
            "\"C:\\space here\\\\\""
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_round_trip_preserves_windows_quotes_and_backslashes() {
        let root = unique_temp_dir("sikarugir_plist_flags");
        let info_plist = root.join("Info.plist");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            &info_plist,
            br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Program Flags</key><string></string></dict></plist>"#,
        )
        .unwrap();

        let flags = "-applaunch 1145350 --rom_modding_root_folder \"C:\\Program Files (x86)\\Steam\\steamapps\\common\\Hades II\\Ship\"";
        write_plist_string(&info_plist, "Program Flags", flags).unwrap();

        assert_eq!(read_plist_string(&info_plist, "Program Flags"), flags);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activation_reuses_the_wrapper_instead_of_launching_another_instance() {
        let command = macos_wineskin_activation_command(std::path::Path::new(
            "/Users/Player/Applications/Sikarugir/Steam.app",
        ));
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new(
                "/Users/Player/Applications/Sikarugir/Steam.app"
            )]
        );
        assert!(command
            .get_args()
            .all(|argument| argument != std::ffi::OsStr::new("-n")));
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
        log::info!(
            "[compat_runner] Using CrossOver bottle {:?} with runner {:?}",
            bottle_path,
            runner_path
        );
    } else if let Some(prefix_root) = prefix_root {
        command.env("WINEPREFIX", prefix_root);
        log::info!(
            "[compat_runner] Using Wine prefix {:?} with runner {:?}",
            prefix_root,
            runner_path
        );
    } else {
        log::info!(
            "[compat_runner] Using runner {:?} without explicit prefix",
            runner_path
        );
    }

    Ok(())
}

#[cfg(test)]
mod wine_registry_and_wrapper_tests {
    use super::*;

    const REG: &str = "WINE REGISTRY Version 2\n;; All keys relative to \\\\User\\\\S-1-5-21-0-0-0-1000\n\n#arch=win64\n\n[Software\\\\Wine\\\\Fonts] 1700000000\n#time=1da\n\"LogPixels\"=dword:00000060\n";

    fn dlls(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_bottle_without_overrides_gets_the_section_and_keeps_the_rest() {
        let updated = with_native_dll_overrides(REG, &dlls(&["winhttp"]), 42).unwrap();
        assert!(updated.starts_with(REG.trim_end()));
        assert!(updated
            .contains("[Software\\\\Wine\\\\DllOverrides] 42\n\"winhttp\"=\"native,builtin\"\n"));
        assert!(updated.contains("\"LogPixels\"=dword:00000060"));
        // Applying it again changes nothing.
        assert_eq!(
            with_native_dll_overrides(&updated, &dlls(&["winhttp"]), 43),
            None
        );
    }

    #[test]
    fn an_existing_builtin_override_is_turned_native_and_others_are_kept() {
        let reg = "[Software\\\\Wine\\\\DllOverrides] 1\n#time=1\n\"d3d11\"=\"native\"\n\"WinHttp\"=\"builtin\"\n\n[Software\\\\Other] 2\n\"x\"=\"y\"\n";
        let updated = with_native_dll_overrides(reg, &dlls(&["winhttp", "version"]), 9).unwrap();
        assert!(updated.contains("\"winhttp\"=\"native,builtin\""));
        assert!(!updated.contains("\"WinHttp\"=\"builtin\""));
        assert!(updated.contains("\"d3d11\"=\"native\""));
        // New entries go into the overrides section, not the one after it.
        let section = updated.split("[Software\\\\Other]").next().unwrap();
        assert!(section.contains("\"version\"=\"native,builtin\""));
        assert!(updated.contains("[Software\\\\Other] 2\n\"x\"=\"y\""));
        // A native-first value the user chose is left alone.
        let native = "[Software\\\\Wine\\\\DllOverrides] 1\n\"winhttp\"=\"native\"\n";
        assert_eq!(
            with_native_dll_overrides(native, &dlls(&["winhttp"]), 9),
            None
        );
    }

    #[test]
    fn windows_line_endings_are_kept() {
        let reg = "WINE REGISTRY Version 2\r\n\r\n[Software\\\\Wine\\\\DllOverrides] 1\r\n";
        let updated = with_native_dll_overrides(reg, &dlls(&["winhttp"]), 9).unwrap();
        assert!(updated.contains(
            "[Software\\\\Wine\\\\DllOverrides] 1\r\n\"winhttp\"=\"native,builtin\"\r\n"
        ));
        assert!(!updated.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn the_registry_file_is_changed_once_with_a_backup() {
        let root = std::env::temp_dir().join(format!("r2modmac-userreg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(
            !ensure_native_dll_overrides(&root, "winhttp=n,b").unwrap(),
            "no user.reg, nothing to do"
        );
        std::fs::write(root.join("user.reg"), REG).unwrap();
        assert!(ensure_native_dll_overrides(&root, "winhttp=n,b").unwrap());
        assert!(!ensure_native_dll_overrides(&root, "winhttp=n,b").unwrap());
        assert_eq!(
            std::fs::read_to_string(root.join("user.reg.r2modmac-backup")).unwrap(),
            REG
        );
        assert!(std::fs::read_to_string(root.join("user.reg"))
            .unwrap()
            .contains("\"winhttp\"=\"native,builtin\""));
        assert!(!root.join("user.reg.r2modmac-staging").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_second_launch_inside_the_restore_window_keeps_the_real_originals() {
        let root = std::env::temp_dir().join(format!("r2modmac-wrapper-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let plist = root.join("Info.plist");
        assert!(std::process::Command::new("/usr/bin/plutil")
            .args(["-create", "xml1"])
            .arg(&plist)
            .status()
            .unwrap()
            .success());
        write_plist_string(
            &plist,
            "Program Name and Path",
            "/Program Files (x86)/Steam/steam.exe",
        )
        .unwrap();
        write_plist_string(&plist, "Program Flags", "").unwrap();

        // First launch saves the originals and writes its own values.
        let first = original_wrapper_settings(&plist).unwrap();
        write_plist_string(&plist, "Program Flags", "-applaunch 632360").unwrap();
        // A second launch before the restore must not take those for originals.
        let second = original_wrapper_settings(&plist).unwrap();
        assert_eq!(first, second);
        assert_eq!(second.flags, "");

        restore_wrapper_settings(&plist, &second);
        assert_eq!(read_plist_string(&plist, "Program Flags"), "");
        assert!(!wrapper_settings_sidecar(&plist).exists());

        // A wrapper whose program cannot be read is not touched at all.
        let empty = root.join("Empty.plist");
        assert!(std::process::Command::new("/usr/bin/plutil")
            .args(["-create", "xml1"])
            .arg(&empty)
            .status()
            .unwrap()
            .success());
        assert!(original_wrapper_settings(&empty).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
