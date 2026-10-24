use super::super::owml_patcher;
use super::super::*;

/// How long a just-started Balatro is watched before it counts as up. With
/// mods, Lovely and Steamodded take a couple of seconds before the game's own
/// `love.load` asks Steam for a session.
const EARLY_EXIT_WINDOW_MS: u64 = 10_000;

/// How long to wait for a Steam that r2modmac just opened to sign in.
const STEAM_SIGN_IN_WAIT_MS: u64 = 90_000;

/// What a just-started game did in its first seconds.
#[derive(Debug, PartialEq, Eq)]
enum EarlyExit {
    StillRunning,
    /// Exited normally. Balatro does this when Steam is not ready: its
    /// `love.load` quits as soon as `luasteam:init()` fails.
    Quit,
    /// Died, e.g. dyld could not load liblovely.dylib.
    Failed(String),
}

fn watch_early_exit(child: &mut std::process::Child, window_ms: u64) -> EarlyExit {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(window_ms);
    while std::time::Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return EarlyExit::Quit,
            Ok(Some(status)) => return EarlyExit::Failed(status.to_string()),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => return EarlyExit::StillRunning,
        }
    }
    EarlyExit::StillRunning
}

/// How long to wait for BepInEx to announce itself after the game is up.
const BEPINEX_LOG_GRACE_MS: u64 = 20_000;

/// Did BepInEx actually take over this launch?
///
/// Doorstop is injected by the launch script and reports its own progress, but
/// nothing downstream checks whether it reached BepInEx: the game starts either
/// way, so a loader that silently failed to hook the Mono runtime looks exactly
/// like a successful modded launch.
///
/// Two signals, because neither alone is enough. `LogOutput.log` is missing on
/// games whose disk logging is off — Muck runs BepInEx and never writes one —
/// while `cache/` is written by the preloader as it patches assemblies. Either
/// one appearing after the launch means BepInEx got control.
fn path_or_descendant_updated(path: &std::path::Path, launched_at: std::time::SystemTime) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };

    if metadata
        .modified()
        .is_ok_and(|modified| modified >= launched_at)
    {
        return true;
    }

    if !metadata.file_type().is_dir() {
        return false;
    }

    std::fs::read_dir(path).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let Ok(file_type) = entry.file_type() else {
                return false;
            };
            !file_type.is_symlink() && path_or_descendant_updated(&entry.path(), launched_at)
        })
    })
}

fn macos_bepinex_signal_updated(
    tree_roots: &[&std::path::Path],
    launched_at: std::time::SystemTime,
) -> bool {
    tree_roots.iter().any(|tree_root| {
        let bepinex = tree_root.join("BepInEx");

        // Cache directory mtimes do not change when BepInEx reuses and updates
        // an existing cache file, so inspect its descendants as well.
        if path_or_descendant_updated(&bepinex.join("cache"), launched_at) {
            return true;
        }

        // BepInEx may rotate or disambiguate its disk log instead of writing the
        // exact LogOutput.log path. Treat every LogOutput.log* file as evidence.
        std::fs::read_dir(&bepinex).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("LogOutput.log"))
                    && path_or_descendant_updated(&entry.path(), launched_at)
            })
        })
    })
}

fn macos_bepinex_took_over(
    tree_roots: &[&std::path::Path],
    launched_at: std::time::SystemTime,
) -> bool {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(BEPINEX_LOG_GRACE_MS);

    while std::time::Instant::now() < deadline {
        // The user can stop waiting here too: this grace period runs after the
        // game is already up, so there is nothing to gain by making them sit
        // through it.
        if super::super::launch_cancel::launch_cancelled() {
            return false;
        }

        if macos_bepinex_signal_updated(tree_roots, launched_at) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    false
}

pub(crate) async fn launch_game_with_mods_for_macos(
    app: &AppHandle,
    game_identifier: &str,
    profile_id: &str,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let settings = load_settings_impl(app);
    let launch_mode = get_profile_launch_mode(app, profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    let runtime_game_path = if is_balatro_identifier(game_identifier)
        || is_balatro_game_path(game_path)
        || is_outerwilds_identifier(game_identifier)
        || is_outerwilds_game_path(game_path)
    {
        game_path.to_path_buf()
    } else {
        let resolved = resolve_macos_runtime_root(game_path);
        if resolved != game_path {
            log::info!(
                "[launch_game_with_mods] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    };
    let executable_path = find_macos_executable_path(&runtime_game_path);

    // Outer Wilds: run socket-free patcher then launch OuterWilds.exe directly via Wine/CrossOver
    if is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(game_path) {
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if owml_disabled.exists() && !owml_folder.exists() {
            let _ = fs::rename(&owml_disabled, &owml_folder);
            log::info!("[launch_game_with_mods] Restored OWML_DISABLED -> OWML");
        }

        if !owml_folder.exists() {
            return Err("OWML folder not found. Please install OWML first.".to_string());
        }

        // Restore modded DLLs (Assembly-CSharp + mscorlib)
        let _ = restore_outerwilds_modded(game_path);

        // Run socket-free patcher — no sockets, no crashing
        log::info!("[launch_game_with_mods] Running OWMLPatcher.exe via Wine");
        if let Err(e) = owml_patcher::run_owml_patcher(game_path) {
            log::warn!(
                "[launch_game_with_mods] OWMLPatcher failed (non-fatal, continuing): {}",
                e
            );
        }

        // Launch OuterWilds.exe directly — mods injected via patched Assembly-CSharp.dll
        log::info!("[launch_game_with_mods] Launching OuterWilds.exe directly");
        return launch_windows_game(app, game_path, None, false);
    }

    if is_balatro_identifier(game_identifier) || is_balatro_game_path(game_path) {
        let run_script = game_path.join(BALATRO_LOVELY_SCRIPT);
        if !run_script.exists() {
            return Err("run_lovely_macos.sh not found".to_string());
        }

        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }

        set_executable_if_present(&run_script)?;
        dequarantine_recursive(game_path);

        // Lovely's own script `cd`s into the default Steam library before it
        // starts the game. With Balatro in another library that runs whatever
        // copy sits at the default path, without Lovely beside it, and the
        // game dies the moment dyld cannot find liblovely.dylib. Starting the
        // game here, from this folder and with an absolute library path, runs
        // the copy r2modmac installed Lovely into.
        let lovely = game_path.join("liblovely.dylib");
        let direct = executable_path
            .as_ref()
            .filter(|executable| executable.is_file() && lovely.is_file())
            .cloned();
        let spawn_game = || -> Result<std::process::Child, String> {
            match direct.as_ref() {
                Some(executable) => {
                    log::info!(
                        "[launch_game_with_mods] Starting {} with Lovely from {}",
                        executable.display(),
                        lovely.display()
                    );
                    std::process::Command::new(executable)
                        .env("DYLD_INSERT_LIBRARIES", &lovely)
                        .current_dir(game_path)
                        .spawn()
                        .map_err(|e| format!("Failed to launch Balatro with Lovely: {}", e))
                }
                None => std::process::Command::new("/bin/sh")
                    .arg(&run_script)
                    .current_dir(game_path)
                    .spawn()
                    .map_err(|e| format!("Failed to launch run_lovely_macos.sh: {}", e)),
            }
        };

        // Balatro only runs inside a Steam session: its love.load quits the
        // moment luasteam cannot reach Steam. The other games get Steam up
        // through the steam://run path; this one is started directly, so it
        // opens Steam the same way first and waits for Steam to finish signing
        // in, then starts the game once.
        if get_profile_distribution(app, profile_id) != "manual" {
            let marks = steam_connection_log_marks(app);
            if !ensure_macos_steam_running_for_launch(app) {
                let waiting_since = std::time::Instant::now();
                while !steam_logged_on_since(&marks) {
                    super::super::launch_cancel::ensure_not_cancelled()?;
                    if waiting_since.elapsed().as_millis() >= u128::from(STEAM_SIGN_IN_WAIT_MS) {
                        log::warn!("[launch_game_with_mods] Steam did not report signing in; starting Balatro anyway");
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                log::info!(
                    "[launch_game_with_mods] Steam ready after {} ms",
                    waiting_since.elapsed().as_millis()
                );
            }
        }

        let mut child = spawn_game()?;
        match watch_early_exit(&mut child, EARLY_EXIT_WINDOW_MS) {
            EarlyExit::StillRunning => {}
            EarlyExit::Failed(status) => {
                log::warn!(
                    "[launch_game_with_mods] Balatro died right after starting: {}",
                    status
                );
                return Err(format!(
                    "The game closed as soon as it started ({status}). Apply the profile again so the mod loader is in place, then launch again."
                ));
            }
            EarlyExit::Quit => {
                return Err(
                    "Balatro closed right after starting. On macOS it only runs while Steam is open and signed in: open Steam, sign in, then launch again.".to_string(),
                );
            }
        }

        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, 60_000);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_with_mods] run_lovely_macos.sh launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }

        return Ok(());
    }

    if crate::models::loaders::uses_return_of_modding(game_identifier, &runtime_game_path) {
        return launch_windows_game(app, &runtime_game_path, None, true);
    }

    validate_macos_bepinex_support(&runtime_game_path)?;
    let tree_root = bepinex_install_root(app, profile_id, &runtime_game_path)?;
    sync_macos_runtime_disabled_state_rooted(&runtime_game_path, false, Some(&tree_root)).map_err(
        |error| {
            format!(
                "Failed to enable macOS runtime before modded launch ({}): {}",
                runtime_game_path.display(),
                error
            )
        },
    )?;

    prepare_native_apple_silicon_for_launch(app, &runtime_game_path, &tree_root, executable_path.as_ref());

    let dist = infer_distribution_from_game_path(app, game_path, false);
    if dist == "steam" && !use_direct_launch {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        let run_script = canonicalize_macos_bepinex_script(&runtime_game_path)?;
        let bepinex_root = bepinex_install_root(app, profile_id, &runtime_game_path)?;
        let profile_root =
            (bepinex_root != runtime_game_path).then(|| bepinex_root.join("BepInEx"));
        configure_macos_bepinex_script(
            &run_script,
            &runtime_game_path,
            settings.write_debug_logs_to_game,
            profile_root.as_deref(),
        )?;
        dequarantine_recursive(&runtime_game_path);

        if let Ok(false) = macos_steam_launch_option_matches_desired(app, game_path) {
            log::info!(
                "[launch_game_with_mods] Steam launch option differs (arch/script mismatch) — reconciling managed option before Steam launch."
            );
            // An immediate steam://run follows below. Keep Steam closed after
            // writing localconfig.vdf so that this launch is the process that
            // reloads the newly persisted option.
            ensure_macos_steam_launch_options(app, game_path, true, false)?;
        }
        let launched_at = std::time::SystemTime::now();
        launch_via_steam_for_game_path(app, game_path)?;
        if !macos_bepinex_took_over(&[&bepinex_root, &runtime_game_path], launched_at) {
            super::super::launch_cancel::ensure_not_cancelled()?;
            // Disk logging is optional in BepInEx and may be buffered. A warm
            // cache may not be rewritten either, so missing fresh filesystem
            // activity is not evidence that injection failed. All actionable
            // setup and launch failures have already been returned above.
            log::warn!(
                "[launch_game_with_mods] The game started, but no fresh BepInEx log/cache signal was observed. Continuing because disk logging may be disabled, buffered, or using an unchanged warm cache."
            );
        }
        return Ok(());
    }

    let profile_bepinex_root = (tree_root != runtime_game_path).then(|| tree_root.join("BepInEx"));
    if launch_macos_bepinex_wrapper(
        app,
        &runtime_game_path,
        executable_path.as_ref(),
        profile_bepinex_root.as_deref(),
        "launch_game_with_mods",
    )? {
        return Ok(());
    }

    let app_bundle = find_macos_launch_bundle(game_path);
    if let Some(bundle) = app_bundle {
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        let _ = open::that(&bundle);
        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_with_mods] App bundle launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    Err("run_bepinex.sh not found and no .app bundle found either".to_string())
}

/// Puts the native Apple Silicon files beside the game when it can use them (see
/// apple_silicon.rs). Best effort: a problem here never stops the launch, which
/// then runs under Rosetta as before.
fn prepare_native_apple_silicon_for_launch(
    app: &AppHandle,
    runtime_game_path: &std::path::Path,
    tree_root: &std::path::Path,
    executable_path: Option<&std::path::PathBuf>,
) {
    use tauri::Manager;
    let resources = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(APPLE_SILICON_RESOURCE_DIR));
    let core_dir = tree_root.join("BepInEx").join("core");
    let host_is_apple_silicon = is_apple_silicon_host();
    let game_has_arm64 = host_is_apple_silicon
        && executable_path.is_some_and(|path| macho_file_supports_arm64(path));
    match prepare_native_apple_silicon(&NativeAppleSiliconTarget {
        resources: resources.as_deref(),
        runtime_root: runtime_game_path,
        core_dir: &core_dir,
        host_is_apple_silicon,
        game_has_arm64,
    }) {
        Ok(NativeAppleSilicon::NotApplicable(reason)) => {
            log::info!("[launch_game_with_mods] Native Apple Silicon not used: {}", reason)
        }
        Ok(outcome) => log::info!("[launch_game_with_mods] Native Apple Silicon: {:?}", outcome),
        Err(error) => log::warn!(
            "[launch_game_with_mods] Couldn't set up native Apple Silicon, launching as before: {}",
            error
        ),
    }
}

pub(crate) async fn launch_game_vanilla_for_macos(
    app: &AppHandle,
    game_identifier: &str,
    profile_id: &str,
    game_path_str: &str,
) -> Result<(), String> {
    let game_path = std::path::PathBuf::from(game_path_str);
    let settings = load_settings_impl(app);
    let launch_mode = get_profile_launch_mode(app, profile_id);
    let use_direct_launch = profile_prefers_direct_launch(&launch_mode);

    let runtime_game_path = if is_balatro_identifier(game_identifier)
        || is_balatro_game_path(&game_path)
        || is_outerwilds_identifier(game_identifier)
        || is_outerwilds_game_path(&game_path)
    {
        game_path.clone()
    } else {
        let resolved = resolve_macos_runtime_root(&game_path);
        if resolved != game_path {
            log::info!(
                "[launch_game_vanilla] Resolved macOS runtime root {} -> {}",
                game_path.display(),
                resolved.display()
            );
        }
        resolved
    };

    let is_outerwilds =
        is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(&game_path);
    if is_outerwilds {
        // Disable OWML by renaming folder
        let owml_folder = game_path.join("OWML");
        let owml_disabled = game_path.join("OWML_DISABLED");
        if owml_folder.exists() {
            if owml_disabled.exists() {
                let _ = fs::remove_dir_all(&owml_disabled);
            }
            let _ = fs::rename(&owml_folder, &owml_disabled);
            log::info!("[launch_game_vanilla] Renamed OWML -> OWML_DISABLED");
        }

        // Restore vanilla DLLs (Assembly-CSharp + mscorlib)
        let _ = restore_outerwilds_vanilla(&game_path);
        let _ = restore_mscorlib_vanilla(&game_path, false);

        // Launch OuterWilds.exe directly — vanilla, no mods
        log::info!("[launch_game_vanilla] Launching OuterWilds.exe directly (vanilla)");
        return launch_windows_game(app, &game_path, None, false);
    }

    let executable_path = find_macos_executable_path(&runtime_game_path);

    #[cfg(target_os = "macos")]
    if let Some(app_bundle) = find_macos_launch_bundle(&game_path) {
        if is_steam_bundle_path(&app_bundle) {
            log::info!(
                "[launch_game_vanilla] Skipping signature/quarantine changes for Steam bundle: {}",
                app_bundle.display()
            );
        } else {
            let is_bundled_steam_emu = find_macos_wrapper_launcher_path(&game_path).is_some();
            let needs_resign = !is_bundled_steam_emu
                && std::process::Command::new("codesign")
                    .args(["-v", "--strict", &app_bundle.to_string_lossy()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| !s.success())
                    .unwrap_or(false);

            if needs_resign {
                log::warn!(
                    "[launch_game_vanilla] App bundle has invalid/no signature — re-signing with ad-hoc"
                );
                let _ = std::process::Command::new("codesign")
                    .args([
                        "--force",
                        "--deep",
                        "-s",
                        "-",
                        &app_bundle.to_string_lossy(),
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else if is_bundled_steam_emu {
                log::info!(
                    "[launch_game_vanilla] Skipping re-sign for bundled Steam-emu wrapper (GOG-style): {}",
                    app_bundle.display()
                );
            }

            let dequarantine_recursive = std::process::Command::new("xattr")
                .args(["-dr", "com.apple.quarantine", &app_bundle.to_string_lossy()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let recursive_ok = dequarantine_recursive
                .as_ref()
                .map(|s| s.success())
                .unwrap_or(false);
            if !recursive_ok {
                let _ = std::process::Command::new("xattr")
                    .args(["-c", &app_bundle.to_string_lossy()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }

    let dist = infer_distribution_from_game_path(app, &game_path, false);
    if dist == "steam" && !use_direct_launch {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        let tree_root = bepinex_install_root(app, profile_id, &runtime_game_path)?;
        if let Err(error) =
            sync_macos_runtime_disabled_state_rooted(&runtime_game_path, true, Some(&tree_root))
        {
            log::warn!(
                "[launch_game_vanilla] Failed to enforce runtime_disabled state before vanilla Steam launch: {}",
                error
            );
        }
        if let Ok(true) = macos_steam_launch_option_is_managed(app, &game_path) {
            log::info!(
                "[launch_game_vanilla] Managed BepInEx launch option detected — clearing it before vanilla Steam launch so native macOS games start outside the mod wrapper."
            );
            ensure_macos_steam_launch_options(app, &game_path, false, false)?;
        }

        return launch_via_steam_for_game_path(app, &game_path);
    }

    if find_macos_wrapper_launcher_path(&game_path).is_some() {
        if let Some(inner_executable_path) = executable_path.as_ref() {
            if let Some(inner_app_bundle) = find_enclosing_app_bundle(inner_executable_path) {
                if !is_macos_bundle_signature_valid(&inner_app_bundle) {
                    log::warn!(
                        "[launch_game_vanilla] Inner game app has invalid/no signature — re-signing with ad-hoc: {}",
                        inner_app_bundle.display()
                    );
                    if !ad_hoc_sign_macos_bundle(&inner_app_bundle) {
                        log::warn!(
                            "[launch_game_vanilla] Failed to ad-hoc sign inner game app bundle: {}",
                            inner_app_bundle.display()
                        );
                    }
                }

                clear_macos_bundle_quarantine(&inner_app_bundle);
            }
        }

        let tree_root = bepinex_install_root(app, profile_id, &runtime_game_path)?;
        sync_macos_runtime_disabled_state_rooted(&runtime_game_path, true, Some(&tree_root))?;
        if launch_macos_bepinex_wrapper(
            app,
            &runtime_game_path,
            executable_path.as_ref(),
            None,
            "launch_game_vanilla",
        )? {
            return Ok(());
        }
    }

    let app_bundle = find_macos_launch_bundle(&game_path);

    if let Some(bundle) = app_bundle {
        if !settings.write_debug_logs_to_game {
            remove_r2modmac_debug_logs(&runtime_game_path);
        }
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        open::that(&bundle).map_err(|e| format!("Failed to launch app bundle: {}", e))?;
        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_vanilla] App bundle launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    if let Ok(Some(executable)) = find_game_executable(game_path_str.to_string()).await {
        if let Some(executable_path) = executable_path.as_ref() {
            if is_process_running_for_executable(executable_path) {
                return Err("Game is already running.".to_string());
            }
        }
        open::that(executable).map_err(|e| format!("Failed to launch game executable: {}", e))?;
        if let Some(executable_path) = executable_path.as_ref() {
            let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
            observed.ok_unless_cancelled()?;
            if !observed.started() {
                log::warn!(
                    "[launch_game_vanilla] Executable launch request succeeded, but the game process was not observed in time. Continuing optimistically."
                );
            }
        }
        return Ok(());
    }

    if let Some(executable_path) = executable_path.as_ref() {
        if is_process_running_for_executable(executable_path) {
            return Err("Game is already running.".to_string());
        }
    }
    open::that(&game_path).map_err(|e| format!("Failed to launch game: {}", e))?;
    if let Some(executable_path) = executable_path.as_ref() {
        let observed = wait_for_process_start(executable_path, MACOS_LAUNCH_OBSERVE_TIMEOUT_MS);
        observed.ok_unless_cancelled()?;
        if !observed.started() {
            return Err("Game did not start in time.".to_string());
        }
    }
    Ok(())
}

pub(crate) fn is_game_running_for_macos(game_path: &std::path::Path) -> Result<bool, String> {
    let Some(executable_path) = find_macos_executable_path(game_path) else {
        return Ok(false);
    };

    Ok(is_process_running_for_executable(&executable_path))
}

#[cfg(target_os = "macos")]
pub(crate) fn stop_game_for_macos(game_path: &std::path::Path) -> Result<(), String> {
    let executable_path = find_macos_executable_path(game_path)
        .ok_or_else(|| "Could not determine the game executable.".to_string())?;
    let exec_patterns = build_macos_process_kill_patterns(&executable_path);

    if !is_process_running_for_executable(&executable_path) {
        return Ok(());
    }

    for pattern in &exec_patterns {
        let _ = std::process::Command::new("/usr/bin/pkill")
            .args(["-TERM", "-f", pattern])
            .status()
            .map_err(|e| format!("Failed to stop the game: {}", e))?;
    }

    if wait_for_process_exit_patterns(&exec_patterns, 5_000) {
        return Ok(());
    }

    for pattern in &exec_patterns {
        let _ = std::process::Command::new("/usr/bin/pkill")
            .args(["-KILL", "-f", pattern])
            .status()
            .map_err(|e| format!("Failed to force stop the game: {}", e))?;
    }

    if !wait_for_process_exit_patterns(&exec_patterns, 3_000) {
        return Err("Game did not stop in time.".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod bepinex_takeover_signal_tests {
    use super::macos_bepinex_signal_updated;

    fn world(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-bepinex-signal-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn isolated_profile_log_proves_bepinex_loaded() {
        let root = world("profile-log");
        let game = root.join("game");
        let profile = root.join("profiles/active");
        std::fs::create_dir_all(game.join("BepInEx")).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx")).unwrap();
        std::fs::write(
            profile.join("BepInEx/LogOutput.log"),
            b"Chainloader started",
        )
        .unwrap();

        let before_any_test_file = std::time::UNIX_EPOCH;
        assert!(macos_bepinex_signal_updated(
            &[profile.as_path()],
            before_any_test_file
        ));
        assert!(!macos_bepinex_signal_updated(
            &[game.as_path()],
            before_any_test_file
        ));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_profile_signal_does_not_confirm_the_new_launch() {
        let root = world("stale-profile-log");
        let profile = root.join("profiles/active");
        std::fs::create_dir_all(profile.join("BepInEx/cache")).unwrap();

        let after_test_files = std::time::SystemTime::now() + std::time::Duration::from_secs(1);
        assert!(!macos_bepinex_signal_updated(
            &[profile.as_path()],
            after_test_files
        ));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn game_root_log_can_confirm_an_isolated_profile_launch() {
        let root = world("game-log");
        let game = root.join("game");
        let profile = root.join("profiles/active");
        std::fs::create_dir_all(game.join("BepInEx")).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx")).unwrap();
        std::fs::write(game.join("BepInEx/LogOutput.log.1"), b"Chainloader started").unwrap();

        assert!(macos_bepinex_signal_updated(
            &[profile.as_path(), game.as_path()],
            std::time::UNIX_EPOCH
        ));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn updated_cache_child_confirms_launch_even_if_cache_directory_is_old() {
        let root = world("cache-child");
        let profile = root.join("profiles/active");
        let cache = profile.join("BepInEx/cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let launched_at = std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(cache.join("patched-assembly.dll"), b"patched").unwrap();

        assert!(macos_bepinex_signal_updated(
            &[profile.as_path()],
            launched_at
        ));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn early_exits_tell_a_steam_quit_from_a_crash() {
        use super::{watch_early_exit, EarlyExit};
        let mut crashed = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 134"])
            .spawn()
            .unwrap();
        assert!(matches!(
            watch_early_exit(&mut crashed, 3_000),
            EarlyExit::Failed(_)
        ));

        let mut quit = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        assert_eq!(watch_early_exit(&mut quit, 3_000), EarlyExit::Quit);

        let mut running = std::process::Command::new("/bin/sleep")
            .arg("2")
            .spawn()
            .unwrap();
        assert_eq!(watch_early_exit(&mut running, 300), EarlyExit::StillRunning);
        let _ = running.kill();
    }
}
