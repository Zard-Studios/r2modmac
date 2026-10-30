//! Which architecture a modded BepInEx game runs in on Apple Silicon, per profile.
//!
//! A profile's `architecture` is `auto` (the default), `native` (Apple Silicon)
//! or `rosetta` (Intel). Automatic is what r2modmac always did: native when the
//! game and its loader allow it, and Rosetta after a native crash. The crash
//! "pin" is a file per profile beside the game, so one profile's mods don't
//! move the others. `native` and `rosetta` are explicit choices: the launcher
//! neither retries nor pins.
//!
//! `run_bepinex.sh` is shared by every profile of a game, so Rust hands each
//! launch its answer in `.r2modmac_launch_arch` (see `write_launch_override`);
//! the launcher reads it before it picks the architecture.
//!
//! Without Rosetta (Apple is removing it) Automatic never picks or retries
//! Intel, and a game with no Apple Silicon build stops with a clear message
//! instead of crashing.

use super::*;
use serde::Serialize;
use std::io::Read;

pub(crate) const LAUNCH_OVERRIDE_FILE: &str = ".r2modmac_launch_arch";
/// The launcher's crash pin: `.r2modmac_force_x64_state` for the whole game
/// (what older launchers wrote), `.r2modmac_force_x64_state.<profile>` per profile.
pub(crate) const PIN_FILE_PREFIX: &str = ".r2modmac_force_x64_state";
const ROSETTA_RUNTIME: &str = "/Library/Apple/usr/libexec/oah/libRosettaRuntime";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ArchitectureMode {
    Auto,
    Native,
    Rosetta,
}

impl ArchitectureMode {
    /// Anything unknown or missing is Automatic, so an old or odd profiles.json keeps working.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value {
            Some("native") => Self::Native,
            Some("rosetta") => Self::Rosetta,
            _ => Self::Auto,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Native => "native",
            Self::Rosetta => "rosetta",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LaunchArch {
    Native,
    Rosetta,
    Unavailable,
}

/// What a game, its loader and this Mac allow. Gathered once, then every answer is pure.
#[derive(Debug, Clone, Default)]
pub(crate) struct ArchitectureFacts {
    pub host_apple_silicon: bool,
    pub bepinex_present: bool,
    pub rosetta_installed: bool,
    /// The game starts through a launcher that only runs as Intel.
    pub wrapper_entry: bool,
    pub game_has_arm64: bool,
    pub game_has_x64: bool,
    /// The game's loader has an arm64 slice, or the shim brings one before launch.
    pub native_loader_ok: bool,
    pub x64_loader_ok: bool,
    /// BepInEx is one the Apple Silicon shim can start (a hint, never a requirement).
    pub shim_usable: bool,
    /// Epoch seconds of the profile's crash pin, when one applies to this executable.
    pub pinned_since: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchitectureOption {
    pub available: bool,
    pub reason: Option<String>,
}

impl ArchitectureOption {
    fn yes() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    fn no(reason: &str) -> Self {
        Self {
            available: false,
            reason: Some(reason.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchitecturePin {
    pub since: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchitectureStatus {
    /// False hides the setting: not Apple Silicon, not BepInEx, or nothing to choose from.
    pub applicable: bool,
    pub host_apple_silicon: bool,
    pub rosetta_installed: bool,
    pub game_has_arm64: bool,
    pub game_has_x64: bool,
    pub native: ArchitectureOption,
    pub rosetta: ArchitectureOption,
    pub shim_hint: Option<String>,
    pub auto_would_use: LaunchArch,
    pub pinned: Option<ArchitecturePin>,
    pub mode: ArchitectureMode,
    pub effective: LaunchArch,
}

/// `<r2modmac.app>/Contents/Resources/apple-silicon`, if this build has one.
pub(crate) fn bundled_apple_silicon_resources(app: &AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(APPLE_SILICON_RESOURCE_DIR))
}

pub(crate) fn rosetta_installed() -> bool {
    // R2MODMAC_ASSUME_NO_ROSETTA=1 lets a Mac that has Rosetta behave like one that doesn't.
    if std::env::var_os("R2MODMAC_ASSUME_NO_ROSETTA").is_some_and(|value| value == "1") {
        return false;
    }
    std::path::Path::new(ROSETTA_RUNTIME).exists()
}

pub(crate) fn compute_status(
    mode: ArchitectureMode,
    facts: &ArchitectureFacts,
) -> ArchitectureStatus {
    let applicable = facts.host_apple_silicon
        && facts.bepinex_present
        && !facts.wrapper_entry
        && (facts.game_has_arm64 || facts.game_has_x64);

    let native = if !facts.host_apple_silicon {
        ArchitectureOption::no("This Mac isn't Apple Silicon.")
    } else if !facts.game_has_arm64 {
        ArchitectureOption::no("This game has no Apple Silicon version.")
    } else if facts.wrapper_entry {
        ArchitectureOption::no("This game starts through a launcher that only runs on Intel.")
    } else if !facts.native_loader_ok {
        ArchitectureOption::no("This game's mod loader has no Apple Silicon version.")
    } else {
        ArchitectureOption::yes()
    };

    let rosetta = if !facts.game_has_x64 {
        ArchitectureOption::no("This game has no Intel version.")
    } else if facts.host_apple_silicon && !facts.rosetta_installed {
        ArchitectureOption::no("Rosetta isn't installed on this Mac.")
    } else if !facts.x64_loader_ok {
        ArchitectureOption::no("This game's mod loader has no Intel version.")
    } else {
        ArchitectureOption::yes()
    };

    let pinned = facts.pinned_since.map(|since| ArchitecturePin { since });

    // What Automatic does: native, unless a crash pinned the profile to Rosetta
    // and Rosetta is still there to go back to.
    let auto_would_use = if native.available && pinned.is_none() {
        LaunchArch::Native
    } else if rosetta.available {
        LaunchArch::Rosetta
    } else if native.available {
        LaunchArch::Native
    } else {
        LaunchArch::Unavailable
    };

    let effective = match mode {
        ArchitectureMode::Auto => auto_would_use,
        ArchitectureMode::Native if native.available => LaunchArch::Native,
        ArchitectureMode::Rosetta if rosetta.available => LaunchArch::Rosetta,
        _ => LaunchArch::Unavailable,
    };

    let shim_hint = (native.available && !facts.shim_usable).then(|| {
        "These mods use an older BepInEx, so Apple Silicon may be less stable.".to_string()
    });

    ArchitectureStatus {
        applicable,
        host_apple_silicon: facts.host_apple_silicon,
        rosetta_installed: facts.rosetta_installed,
        game_has_arm64: facts.game_has_arm64,
        game_has_x64: facts.game_has_x64,
        native,
        rosetta,
        shim_hint,
        auto_would_use,
        pinned,
        mode,
        effective,
    }
}

fn rosetta_missing_error() -> String {
    "ARCH_ROSETTA_MISSING: This game needs Rosetta, which isn't installed on this Mac. \
     Install Rosetta from Apple, or use Apple Silicon if the game supports it."
        .to_string()
}

/// The guard before a modded launch. `Ok(None)` means this launch isn't ours to
/// steer (not Apple Silicon, not BepInEx, a launcher-wrapped game that can still
/// run as before). `Err` is a message for the player instead of a crash.
pub(crate) fn resolve_launch(
    mode: ArchitectureMode,
    facts: &ArchitectureFacts,
) -> Result<Option<LaunchArch>, String> {
    if !facts.host_apple_silicon || !facts.bepinex_present {
        return Ok(None);
    }
    if facts.wrapper_entry {
        // The launcher forces Intel; it only needs Rosetta to be there.
        return if facts.game_has_x64 && !facts.rosetta_installed {
            Err(rosetta_missing_error())
        } else {
            Ok(None)
        };
    }
    let status = compute_status(mode, facts);
    if !status.applicable {
        return Ok(None);
    }
    match status.effective {
        LaunchArch::Native => Ok(Some(LaunchArch::Native)),
        LaunchArch::Rosetta => Ok(Some(LaunchArch::Rosetta)),
        LaunchArch::Unavailable => {
            let intel_blocked_by_rosetta =
                facts.game_has_x64 && !facts.rosetta_installed && !status.native.available;
            Err(match mode {
                ArchitectureMode::Native => format!(
                    "ARCH_NATIVE_UNAVAILABLE: {} Switch Architecture to Automatic in this profile's Settings.",
                    status.native.reason.unwrap_or_default()
                ),
                ArchitectureMode::Rosetta if !facts.rosetta_installed && facts.game_has_x64 => {
                    rosetta_missing_error()
                }
                ArchitectureMode::Rosetta => format!(
                    "ARCH_ROSETTA_UNAVAILABLE: {} Switch Architecture to Automatic in this profile's Settings.",
                    status.rosetta.reason.unwrap_or_default()
                ),
                ArchitectureMode::Auto if intel_blocked_by_rosetta => rosetta_missing_error(),
                ArchitectureMode::Auto => format!(
                    "ARCH_NO_USABLE_ARCH: This game can't run with mods on this Mac. {} {}",
                    status.native.reason.unwrap_or_default(),
                    status.rosetta.reason.unwrap_or_default()
                ),
            })
        }
    }
}

// ── Crash pins ───────────────────────────────────────────────────────────────

/// Keeps a profile id safe to put in a file name: `[A-Za-z0-9_-]`, anything else becomes `_`.
pub(crate) fn sanitize_profile_key(profile_id: &str) -> String {
    profile_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn pin_file_name(profile_key: &str) -> String {
    if profile_key.is_empty() {
        PIN_FILE_PREFIX.to_string()
    } else {
        format!("{PIN_FILE_PREFIX}.{profile_key}")
    }
}

fn file_mtime_secs(path: &std::path::Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// The launcher's pin key is `"<app bundle>|<executable mtime>"`, with the
/// bundle's own mtime (or 0) when the executable can't be read.
fn pin_key_parts(executable: &std::path::Path) -> (String, u64) {
    let text = executable.to_string_lossy();
    let app_path = match text.rfind("/Contents/MacOS") {
        Some(at) => text[..at].to_string(),
        None => text.to_string(),
    };
    let mtime = file_mtime_secs(executable)
        .or_else(|| file_mtime_secs(std::path::Path::new(&app_path)))
        .unwrap_or(0);
    (app_path, mtime)
}

/// When this profile was pinned to Rosetta, if the pin still belongs to this
/// executable (a game update changes its mtime and makes the pin stale, which
/// the launcher then deletes). Reads only.
pub(crate) fn pin_since(
    runtime_root: &std::path::Path,
    profile_key: &str,
    executable: &std::path::Path,
) -> Option<u64> {
    let pin = runtime_root.join(pin_file_name(profile_key));
    let content = fs::read_to_string(&pin).ok()?;
    let (app_path, mtime) = pin_key_parts(executable);
    let same_bundle = |recorded: &str| {
        recorded == app_path
            || matches!(
                (fs::canonicalize(recorded), fs::canonicalize(&app_path)),
                (Ok(a), Ok(b)) if a == b
            )
    };
    let matches = content.lines().any(|line| {
        line.rsplit_once('|').is_some_and(|(recorded, time)| {
            time.trim() == mtime.to_string() && same_bundle(recorded)
        })
    });
    matches.then(|| file_mtime_secs(&pin).unwrap_or(0))
}

/// Removes every pin beside the game: the whole-game file and each profile's.
pub(crate) fn remove_all_architecture_pins(runtime_root: &std::path::Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(runtime_root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == PIN_FILE_PREFIX || name.starts_with(&format!("{PIN_FILE_PREFIX}.")) {
            fs::remove_file(entry.path())
                .map_err(|e| format!("Couldn't remove {}: {}", entry.path().display(), e))?;
        }
    }
    Ok(())
}

/// "Try Apple Silicon again": forgets this profile's pin, and the whole-game
/// one an older launcher wrote (a launch started from Steam alone still reads it).
pub(crate) fn clear_architecture_pin(
    runtime_root: &std::path::Path,
    profile_key: &str,
) -> Result<(), String> {
    for name in [pin_file_name(profile_key), PIN_FILE_PREFIX.to_string()] {
        let path = runtime_root.join(name);
        if path.is_file() {
            fs::remove_file(&path)
                .map_err(|e| format!("Couldn't remove {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

/// What this launch should use, for the launcher to read. Written atomically
/// so a launcher never sees half of it.
pub(crate) fn write_launch_override(
    runtime_root: &std::path::Path,
    mode: ArchitectureMode,
    profile_key: &str,
    rosetta_installed: bool,
) -> Result<(), String> {
    let body = format!(
        "v=1\nmode={}\nprofile={}\nrosetta={}\n",
        mode.as_str(),
        profile_key,
        if rosetta_installed { 1 } else { 0 }
    );
    let target = runtime_root.join(LAUNCH_OVERRIDE_FILE);
    let tmp = runtime_root.join(format!("{LAUNCH_OVERRIDE_FILE}.tmp"));
    fs::write(&tmp, body).map_err(|e| format!("Couldn't write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, &target).map_err(|e| format!("Couldn't write {}: {}", target.display(), e))
}

// ── Gathering the facts ──────────────────────────────────────────────────────

/// The first KiB of a file: enough for a Mach-O or universal header.
fn read_head(path: &std::path::Path) -> Vec<u8> {
    let mut buf = vec![0u8; 1024];
    match fs::File::open(path).and_then(|mut file| file.read(&mut buf)) {
        Ok(read) => {
            buf.truncate(read);
            buf
        }
        Err(_) => Vec::new(),
    }
}

fn slices(path: &std::path::Path) -> (bool, bool) {
    let head = read_head(path);
    (
        mach_o_bytes_have_arch(&head, CPU_TYPE_ARM64),
        mach_o_bytes_have_arch(&head, CPU_TYPE_X86_64),
    )
}

/// Reads the folder, the game and the Mac. `runtime_root` holds the app bundle,
/// the loader and the pins; `tree_root` holds the BepInEx the profile uses.
pub(crate) fn gather_facts(
    resources: Option<&std::path::Path>,
    runtime_root: &std::path::Path,
    tree_root: &std::path::Path,
    profile_key: &str,
) -> ArchitectureFacts {
    let executable = find_macos_executable_path(runtime_root);
    let (game_has_arm64, game_has_x64) =
        executable.as_deref().map(slices).unwrap_or((false, false));
    let core_dir = tree_root.join("BepInEx").join("core");
    let root_loader = runtime_root.join("libdoorstop.dylib");
    let (root_loader_arm64, root_loader_x64) = slices(&root_loader);
    let shim_usable = bepinex_can_use_shim(&core_dir).is_ok();
    let wrapper_entry = match (
        resolve_macos_launch_entry_path(runtime_root),
        executable.as_ref(),
    ) {
        (Ok(entry), Some(executable)) => &entry != executable,
        _ => false,
    };

    ArchitectureFacts {
        host_apple_silicon: is_apple_silicon_host(),
        bepinex_present: macos_bepinex_core_is_bootstrappable(&core_dir),
        rosetta_installed: rosetta_installed(),
        wrapper_entry,
        game_has_arm64,
        game_has_x64,
        native_loader_ok: root_loader_arm64 || (shim_usable && resources_carry_shim(resources)),
        x64_loader_ok: root_loader_x64
            || runtime_root
                .join("doorstop_libs")
                .join("libdoorstop_x64.dylib")
                .is_file(),
        shim_usable,
        pinned_since: executable
            .as_deref()
            .and_then(|executable| pin_since(runtime_root, profile_key, executable)),
    }
}

/// The profile's setting, read the way `get_profile_launch_mode` reads its own.
pub(crate) fn get_profile_architecture(app: &AppHandle, profile_id: &str) -> ArchitectureMode {
    let profiles_path = crate::utils::paths::app_data_dir(app)
        .unwrap_or_default()
        .join("profiles.json");
    let Ok(data) = fs::read_to_string(&profiles_path) else {
        return ArchitectureMode::Auto;
    };
    let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&data) else {
        return ArchitectureMode::Auto;
    };
    profiles
        .iter()
        .find(|profile| profile["id"].as_str() == Some(profile_id))
        .map(|profile| ArchitectureMode::parse(profile["architecture"].as_str()))
        .unwrap_or(ArchitectureMode::Auto)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Mac with Rosetta, a universal game, a BepInEx the shim can start, no pin.
    fn universal() -> ArchitectureFacts {
        ArchitectureFacts {
            host_apple_silicon: true,
            bepinex_present: true,
            rosetta_installed: true,
            wrapper_entry: false,
            game_has_arm64: true,
            game_has_x64: true,
            native_loader_ok: true,
            x64_loader_ok: true,
            shim_usable: true,
            pinned_since: None,
        }
    }

    fn status(mode: ArchitectureMode, facts: &ArchitectureFacts) -> ArchitectureStatus {
        compute_status(mode, facts)
    }

    #[test]
    fn the_mode_defaults_to_automatic() {
        assert_eq!(ArchitectureMode::parse(None), ArchitectureMode::Auto);
        assert_eq!(ArchitectureMode::parse(Some("")), ArchitectureMode::Auto);
        assert_eq!(
            ArchitectureMode::parse(Some("nonsense")),
            ArchitectureMode::Auto
        );
        assert_eq!(
            ArchitectureMode::parse(Some("native")),
            ArchitectureMode::Native
        );
        assert_eq!(
            ArchitectureMode::parse(Some("rosetta")),
            ArchitectureMode::Rosetta
        );
        assert_eq!(ArchitectureMode::Rosetta.as_str(), "rosetta");
    }

    #[test]
    fn a_universal_game_has_both_options_and_automatic_runs_it_natively() {
        let s = status(ArchitectureMode::Auto, &universal());
        assert!(s.applicable && s.native.available && s.rosetta.available);
        assert_eq!(s.auto_would_use, LaunchArch::Native);
        assert_eq!(s.effective, LaunchArch::Native);
        assert!(s.shim_hint.is_none() && s.pinned.is_none());
    }

    #[test]
    fn a_crash_pin_moves_automatic_to_rosetta_and_only_automatic() {
        let facts = ArchitectureFacts {
            pinned_since: Some(1_700_000_000),
            ..universal()
        };
        let auto = status(ArchitectureMode::Auto, &facts);
        assert_eq!(auto.effective, LaunchArch::Rosetta);
        assert_eq!(
            auto.pinned,
            Some(ArchitecturePin {
                since: 1_700_000_000
            })
        );
        assert_eq!(
            status(ArchitectureMode::Native, &facts).effective,
            LaunchArch::Native
        );
        assert_eq!(
            status(ArchitectureMode::Rosetta, &facts).effective,
            LaunchArch::Rosetta
        );
    }

    #[test]
    fn without_rosetta_a_pin_is_ignored_and_automatic_stays_native() {
        let facts = ArchitectureFacts {
            rosetta_installed: false,
            pinned_since: Some(1),
            ..universal()
        };
        let s = status(ArchitectureMode::Auto, &facts);
        assert_eq!(s.effective, LaunchArch::Native);
        assert_eq!(
            s.rosetta.reason.as_deref(),
            Some("Rosetta isn't installed on this Mac.")
        );
        assert_eq!(
            resolve_launch(ArchitectureMode::Auto, &facts),
            Ok(Some(LaunchArch::Native))
        );
    }

    #[test]
    fn an_intel_only_game_runs_under_rosetta_and_has_no_native_option() {
        let facts = ArchitectureFacts {
            game_has_arm64: false,
            ..universal()
        };
        let s = status(ArchitectureMode::Auto, &facts);
        assert_eq!(s.effective, LaunchArch::Rosetta);
        assert_eq!(
            s.native.reason.as_deref(),
            Some("This game has no Apple Silicon version.")
        );
        assert!(s.applicable);
    }

    #[test]
    fn an_intel_only_game_without_rosetta_stops_with_a_clear_message() {
        let facts = ArchitectureFacts {
            game_has_arm64: false,
            rosetta_installed: false,
            ..universal()
        };
        for mode in [ArchitectureMode::Auto, ArchitectureMode::Rosetta] {
            let error = resolve_launch(mode, &facts).unwrap_err();
            assert!(
                error.starts_with("ARCH_ROSETTA_MISSING"),
                "{mode:?}: {error}"
            );
        }
        let error = resolve_launch(ArchitectureMode::Native, &facts).unwrap_err();
        assert!(error.starts_with("ARCH_NATIVE_UNAVAILABLE"), "{error}");
    }

    #[test]
    fn an_apple_silicon_only_game_has_no_rosetta_option() {
        let facts = ArchitectureFacts {
            game_has_x64: false,
            ..universal()
        };
        let s = status(ArchitectureMode::Auto, &facts);
        assert_eq!(s.effective, LaunchArch::Native);
        assert_eq!(
            s.rosetta.reason.as_deref(),
            Some("This game has no Intel version.")
        );
        let error = resolve_launch(ArchitectureMode::Rosetta, &facts).unwrap_err();
        assert!(error.starts_with("ARCH_ROSETTA_UNAVAILABLE"), "{error}");
    }

    #[test]
    fn a_missing_loader_slice_closes_that_option() {
        let no_arm_loader = ArchitectureFacts {
            native_loader_ok: false,
            ..universal()
        };
        let s = status(ArchitectureMode::Auto, &no_arm_loader);
        assert!(!s.native.available);
        assert_eq!(s.effective, LaunchArch::Rosetta);

        let no_x64_loader = ArchitectureFacts {
            x64_loader_ok: false,
            ..universal()
        };
        let s = status(ArchitectureMode::Auto, &no_x64_loader);
        assert!(!s.rosetta.available);
        assert_eq!(s.effective, LaunchArch::Native);
    }

    #[test]
    fn an_older_bepinex_only_adds_a_hint() {
        let facts = ArchitectureFacts {
            shim_usable: false,
            ..universal()
        };
        let s = status(ArchitectureMode::Auto, &facts);
        assert!(s.native.available);
        assert!(s.shim_hint.is_some());
    }

    #[test]
    fn nothing_is_asked_where_the_choice_does_not_exist() {
        let intel_mac = ArchitectureFacts {
            host_apple_silicon: false,
            ..universal()
        };
        assert!(!status(ArchitectureMode::Auto, &intel_mac).applicable);
        assert_eq!(resolve_launch(ArchitectureMode::Auto, &intel_mac), Ok(None));

        let no_bepinex = ArchitectureFacts {
            bepinex_present: false,
            ..universal()
        };
        assert!(!status(ArchitectureMode::Auto, &no_bepinex).applicable);
        assert_eq!(
            resolve_launch(ArchitectureMode::Native, &no_bepinex),
            Ok(None)
        );

        let thirty_two_bit = ArchitectureFacts {
            game_has_arm64: false,
            game_has_x64: false,
            ..universal()
        };
        assert!(!status(ArchitectureMode::Auto, &thirty_two_bit).applicable);
        assert_eq!(
            resolve_launch(ArchitectureMode::Auto, &thirty_two_bit),
            Ok(None)
        );
    }

    #[test]
    fn a_launcher_wrapped_game_only_needs_rosetta_to_exist() {
        let wrapped = ArchitectureFacts {
            wrapper_entry: true,
            ..universal()
        };
        assert!(!status(ArchitectureMode::Auto, &wrapped).applicable);
        assert_eq!(resolve_launch(ArchitectureMode::Auto, &wrapped), Ok(None));
        let no_rosetta = ArchitectureFacts {
            rosetta_installed: false,
            ..wrapped
        };
        assert!(resolve_launch(ArchitectureMode::Auto, &no_rosetta)
            .unwrap_err()
            .starts_with("ARCH_ROSETTA_MISSING"));
    }

    #[test]
    fn a_game_that_runs_nowhere_says_so() {
        let facts = ArchitectureFacts {
            native_loader_ok: false,
            x64_loader_ok: false,
            ..universal()
        };
        let error = resolve_launch(ArchitectureMode::Auto, &facts).unwrap_err();
        assert!(error.starts_with("ARCH_NO_USABLE_ARCH"), "{error}");
    }

    #[test]
    fn the_status_serializes_with_the_names_the_interface_reads() {
        let json = serde_json::to_value(status(ArchitectureMode::Auto, &universal())).unwrap();
        assert_eq!(json["applicable"], true);
        assert_eq!(json["hostAppleSilicon"], true);
        assert_eq!(json["autoWouldUse"], "native");
        assert_eq!(json["effective"], "native");
        assert_eq!(json["mode"], "auto");
        assert_eq!(json["native"]["available"], true);
        assert!(json["pinned"].is_null());
        assert!(json["rosetta"]["reason"].is_null());
    }

    // ── pins and the launch override ─────────────────────────────────────────

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-architecture-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// `<root>/Game.app/Contents/MacOS/Game`
    fn game_bundle(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let macos = root.join("Game.app/Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        let executable = macos.join("Game");
        fs::write(&executable, b"exe").unwrap();
        (root.join("Game.app"), executable)
    }

    fn pin_line(app: &std::path::Path, executable: &std::path::Path) -> String {
        format!(
            "{}|{}\n",
            app.display(),
            file_mtime_secs(executable).unwrap()
        )
    }

    #[test]
    fn profile_keys_are_safe_file_names() {
        assert_eq!(
            sanitize_profile_key("7e774110-c505-42c9-b4d2-f6a26b200b87"),
            "7e774110-c505-42c9-b4d2-f6a26b200b87"
        );
        assert_eq!(sanitize_profile_key("../../etc/passwd"), "______etc_passwd");
        assert_eq!(sanitize_profile_key("a b/c"), "a_b_c");
        assert_eq!(sanitize_profile_key(""), "");
        assert_eq!(pin_file_name(""), ".r2modmac_force_x64_state");
        assert_eq!(pin_file_name("abc"), ".r2modmac_force_x64_state.abc");
    }

    #[test]
    fn a_pin_counts_only_for_its_own_profile_and_executable() {
        let root = temp_root("pin");
        let (app, executable) = game_bundle(&root);
        fs::write(root.join(pin_file_name("one")), pin_line(&app, &executable)).unwrap();

        assert!(pin_since(&root, "one", &executable).is_some());
        assert_eq!(
            pin_since(&root, "two", &executable),
            None,
            "another profile isn't pinned"
        );
        assert_eq!(
            pin_since(&root, "", &executable),
            None,
            "nor is the whole game"
        );

        fs::write(
            root.join(pin_file_name("one")),
            format!("{}|1\n", app.display()),
        )
        .unwrap();
        assert_eq!(
            pin_since(&root, "one", &executable),
            None,
            "a game update makes the pin stale"
        );

        fs::write(root.join(pin_file_name("one")), "nonsense").unwrap();
        assert_eq!(pin_since(&root, "one", &executable), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_whole_game_pin_still_answers_for_a_launch_without_a_profile() {
        let root = temp_root("legacy");
        let (app, executable) = game_bundle(&root);
        fs::write(root.join(pin_file_name("")), pin_line(&app, &executable)).unwrap();
        assert!(pin_since(&root, "", &executable).is_some());
        assert_eq!(pin_since(&root, "one", &executable), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trying_again_forgets_the_profile_pin_and_the_whole_game_one() {
        let root = temp_root("clear");
        let (app, executable) = game_bundle(&root);
        let line = pin_line(&app, &executable);
        for name in [
            pin_file_name(""),
            pin_file_name("one"),
            pin_file_name("two"),
        ] {
            fs::write(root.join(name), &line).unwrap();
        }
        clear_architecture_pin(&root, "one").unwrap();
        assert!(!root.join(pin_file_name("one")).exists());
        assert!(!root.join(pin_file_name("")).exists());
        assert!(
            root.join(pin_file_name("two")).exists(),
            "another profile's pin stays"
        );

        remove_all_architecture_pins(&root).unwrap();
        assert!(!root.join(pin_file_name("two")).exists());
        assert!(executable.exists(), "the game is untouched");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_launch_override_is_written_whole_and_replaced_in_place() {
        let root = temp_root("override");
        write_launch_override(&root, ArchitectureMode::Rosetta, "abc-1", true).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(LAUNCH_OVERRIDE_FILE)).unwrap(),
            "v=1\nmode=rosetta\nprofile=abc-1\nrosetta=1\n"
        );
        write_launch_override(&root, ArchitectureMode::Auto, "", false).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(LAUNCH_OVERRIDE_FILE)).unwrap(),
            "v=1\nmode=auto\nprofile=\nrosetta=0\n"
        );
        assert!(!root.join(format!("{LAUNCH_OVERRIDE_FILE}.tmp")).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_first_kib_of_a_file_tells_which_slices_it_has() {
        let root = temp_root("slices");
        // FAT_MAGIC and two 20-byte entries: x86_64 (0x01000007), then arm64 (0x0100000c).
        let mut universal = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2];
        for cpu_type in [[0x01, 0, 0, 0x07], [0x01, 0, 0, 0x0c]] {
            universal.extend(cpu_type);
            universal.extend([0u8; 16]);
        }
        let x86_only = [0xcf, 0xfa, 0xed, 0xfe, 0x07, 0x00, 0x00, 0x01];
        let arm_only = [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01];
        for (name, bytes, expected) in [
            ("universal", &universal[..], (true, true)),
            ("x86", &x86_only[..], (false, true)),
            ("arm", &arm_only[..], (true, false)),
            ("other", &b"MZ not a mach-o"[..], (false, false)),
        ] {
            let path = root.join(name);
            fs::write(&path, bytes).unwrap();
            assert_eq!(slices(&path), expected, "{name}");
        }
        assert_eq!(slices(&root.join("missing")), (false, false));
        fs::remove_dir_all(root).unwrap();
    }
}
