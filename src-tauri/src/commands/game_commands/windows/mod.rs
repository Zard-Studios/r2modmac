mod launch;
mod process;

pub(crate) use self::launch::*;
pub(crate) use self::process::*;

use super::*;

/// Tell Wine to load the loader's proxy DLL instead of its own builtin.
///
/// Which proxy that is depends on the pack the game uses, so the value is
/// derived from the files present rather than assuming `version.dll`: Hades II
/// is hooked through Hell2Modding's `d3d12.dll` and would otherwise launch
/// unmodded under Wine.
fn configure_native_loader_dll_override(
    command: &mut std::process::Command,
    game_path: &std::path::Path,
) {
    if let Some(value) = crate::models::loaders::wine_dll_override_value(game_path) {
        command.env("WINEDLLOVERRIDES", value);
    }
}

pub(crate) fn launch_windows_direct_game(game_path: &std::path::Path) -> Result<(), String> {
    launch_windows_direct_game_with_working_dir(game_path, None)
}

pub(crate) fn launch_windows_direct_game_with_working_dir(
    game_path: &std::path::Path,
    working_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    let executable_path = find_pe_game_executable_path(game_path).ok_or_else(|| {
        "Could not find a Windows game executable in the selected folder.".to_string()
    })?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if is_process_running_for_patterns(&process_patterns) {
        log::warn!(
            "[launch_windows_direct_game] Blocked: 'already running'. Patterns: {:?}",
            process_patterns
        );
        return Err("Game is already running.".to_string());
    }

    let executable_dir =
        working_dir.unwrap_or_else(|| executable_path.parent().unwrap_or(game_path));

    #[cfg(unix)]
    {
        let prefix_root =
            find_wine_prefix_root(&executable_path).or_else(|| find_wine_prefix_root(game_path));

        #[cfg(target_os = "macos")]
        if let Some(prefix_root_path) = prefix_root.as_deref() {
            if let Some(bundle_path) =
                find_macos_wineskin_launcher_binary(Some(prefix_root_path), &executable_path)
            {
                // For Wineskin/PortingKit bundles, Wine renames all processes to `wine64`.
                // The wineserver of the specific bundle is the reliable sentinel that the
                // bundle (and therefore the game) has started. We use it for the post-
                // launch wait, but NOT for the pre-launch check (to avoid false positives
                // when Steam is already running inside the same bundle).
                let wait_patterns = build_windows_wineskin_bundle_patterns(&executable_path)
                    .unwrap_or_else(|| process_patterns.clone());

                match launch_macos_wineskin_program(
                    &bundle_path,
                    prefix_root_path,
                    &executable_path,
                    &[],
                    Some(executable_dir),
                    "launch_windows_direct_game",
                ) {
                    Ok(()) => {
                        let observed = wait_for_process_start_patterns(&wait_patterns, 30_000);
                        observed.ok_unless_cancelled()?;
                        if !observed.started() {
                            log::warn!(
                                "[launch_windows_direct_game] Wineskin bundle started but game process not observed in time. Continuing optimistically."
                            );
                        }
                        return Ok(());
                    }
                    Err(error) => {
                        log::warn!(
                            "[launch_windows_direct_game] Sikarugir/Wineskin launch failed ({}); falling back to direct Wine.",
                            error
                        );
                    }
                }
            }
        }

        if let Some(runner_path) =
            find_host_compat_runner_binary(prefix_root.as_deref(), &executable_path)
        {
            let mut command = std::process::Command::new(&runner_path);
            configure_host_compat_runner_command(
                &mut command,
                &runner_path,
                prefix_root.as_deref(),
            )?;
            configure_native_loader_dll_override(&mut command, game_path);
            log::info!(
                "[launch_windows_direct_game] Launching Windows executable directly: {:?}",
                executable_path
            );
            command
                .arg(&executable_path)
                .current_dir(executable_dir)
                .spawn()
                .map_err(|e| {
                    format!(
                        "Failed to launch the Windows game via {}: {}",
                        runner_path.display(),
                        e
                    )
                })?;
        } else if let Some(app_bundle) = find_enclosing_app_bundle(game_path) {
            open::that(&app_bundle)
                .map_err(|e| format!("Failed to launch the Windows wrapper app: {}", e))?;
        } else {
            return Err(
	            "No compatible runner was found for this platform. Install a supported compatibility tool or point the game path inside its prefix."
	                .to_string(),
	        );
        }
    }

    #[cfg(windows)]
    {
        log::info!(
            "[launch_windows_direct_game] Launching Windows executable directly: {:?}",
            executable_path
        );
        std::process::Command::new(&executable_path)
            //.arg("-applaunch")
            .arg(&executable_path)
            .current_dir(executable_dir)
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to launch the Windows game via {}: {}",
                    executable_path.display(),
                    e
                )
            })?;
    }

    let observed = wait_for_process_start_patterns(&process_patterns, 20_000);
    observed.ok_unless_cancelled()?;
    if !observed.started() {
        return Err("Game did not start in time.".to_string());
    }

    Ok(())
}

/// How long to keep watching for the Steam a cancelled launch started, and how
/// long each request is given to take effect. Matched to the macOS watch: a
/// cold Steam under Wine takes just as long to come up.
const STEAM_QUIT_TOTAL_MS: u64 = 60_000;

/// Steam's own shutdown switch — and the marker that tells our own shutdown
/// commands apart from the client, since they carry the same executable path
/// and linger after delivering the request.
const SHUTDOWN_ARGUMENT: &str = "-shutdown";
const STEAM_QUIT_ROUND_MS: u64 = 3_000;
const STEAM_QUIT_POLL_MS: u64 = 500;

/// How to ask the Windows Steam that r2modmac just started to close again.
///
/// `steam.exe -shutdown` is Steam's own documented shutdown switch, so the
/// client exits the way it does from its own menu — nothing is killed. Which
/// route carries it depends on how the client was started in the first place:
/// through a Sikarugir/Wineskin wrapper, through a Wine or CrossOver runner, or
/// natively on Windows.
#[allow(dead_code)]
enum WindowsSteamShutdown {
    #[cfg(all(unix, target_os = "macos"))]
    Wineskin {
        bundle: std::path::PathBuf,
        prefix: std::path::PathBuf,
        steam_executable: std::path::PathBuf,
    },
    #[cfg(unix)]
    Runner {
        runner: std::path::PathBuf,
        prefix: Option<std::path::PathBuf>,
        steam_executable: std::path::PathBuf,
    },
    #[cfg(windows)]
    Native {
        steam_executable: std::path::PathBuf,
    },
}

impl WindowsSteamShutdown {
    /// Keep asking the Steam this launch started to close, until it does.
    ///
    /// Same shape as the macOS watch, and for the same reason the log showed:
    /// the user cancels about a second after pressing Play, long before Steam
    /// exists, so a single request lands on nothing and Steam boots on to start
    /// the game. This waits for Steam to appear, asks, and keeps asking until
    /// its processes are gone.
    ///
    /// Nothing here is specific to one Wine launcher. The route is whichever
    /// one started this Steam — a Sikarugir/Wineskin wrapper, a CrossOver or
    /// Wine runner, or Windows itself — so a launcher that can start a game can
    /// stop it, without this code knowing which launcher it is. The command is
    /// Steam's own `-shutdown` in every case, and "did it work?" is answered by
    /// looking for Steam's processes on the host, which is equally
    /// launcher-agnostic.
    ///
    /// Runs on its own thread so the button comes back at once, and stands down
    /// the moment another launch begins.
    fn watch_until_closed(self, steam_patterns: Vec<String>) {
        let generation = crate::commands::game_commands::launch_cancel::launch_generation();

        std::thread::spawn(move || {
            log::info!(
                "[launch_windows_steam_game] Watching for the Steam this launch started, to close it again."
            );

            let started = std::time::Instant::now();
            let mut seen_running = false;
            let mut rounds = 0u32;

            while started.elapsed() < std::time::Duration::from_millis(STEAM_QUIT_TOTAL_MS) {
                if crate::commands::game_commands::launch_cancel::launch_superseded(generation) {
                    log::info!(
                        "[launch_windows_steam_game] A new launch started; leaving Steam alone after {} round(s).",
                        rounds
                    );
                    return;
                }

                if !is_process_running_for_patterns_excluding(&steam_patterns, SHUTDOWN_ARGUMENT) {
                    if seen_running {
                        log::info!(
                            "[launch_windows_steam_game] Steam closed after {} round(s), {}ms.",
                            rounds,
                            started.elapsed().as_millis()
                        );
                        return;
                    }
                    // Steam is not up yet. `-shutdown` on an absent client only
                    // starts one, so wait for it rather than send anything.
                    std::thread::sleep(std::time::Duration::from_millis(STEAM_QUIT_POLL_MS));
                    continue;
                }

                seen_running = true;
                rounds += 1;
                self.dispatch();
                std::thread::sleep(std::time::Duration::from_millis(STEAM_QUIT_ROUND_MS));
            }

            if seen_running
                && !is_process_running_for_patterns_excluding(&steam_patterns, SHUTDOWN_ARGUMENT)
            {
                log::info!(
                    "[launch_windows_steam_game] Steam closed after {} round(s).",
                    rounds
                );
            } else if seen_running {
                // Almost always a client sitting on the login window: it
                // ignores -shutdown, and it cannot start a game either, so
                // there is nothing left to undo by force.
                log::warn!(
                    "[launch_windows_steam_game] Steam did not close after {} round(s). Leaving it running rather than killing it; it is most likely waiting to be signed in.",
                    rounds
                );
            } else {
                log::info!("[launch_windows_steam_game] Steam never came up; nothing to close.");
            }
        });
    }

    /// One `-shutdown`, by the route that started this Steam.
    fn dispatch(&self) {
        match self {
            #[cfg(all(unix, target_os = "macos"))]
            WindowsSteamShutdown::Wineskin {
                bundle,
                prefix,
                steam_executable,
            } => {
                log::info!(
                    "[launch_windows_steam_game] Asking Steam to shut down through {:?}; r2modmac started it for this launch.",
                    bundle
                );
                if let Err(error) = launch_macos_wineskin_program(
                    bundle,
                    prefix,
                    steam_executable,
                    &[SHUTDOWN_ARGUMENT.to_string()],
                    None,
                    "cancel_windows_steam_launch",
                ) {
                    log::warn!(
                        "[launch_windows_steam_game] Could not shut Steam down after the cancellation: {}",
                        error
                    );
                }
            }
            #[cfg(unix)]
            WindowsSteamShutdown::Runner {
                runner,
                prefix,
                steam_executable,
            } => {
                log::info!(
                    "[launch_windows_steam_game] Asking Steam to shut down through {:?}; r2modmac started it for this launch.",
                    runner
                );
                let mut command = std::process::Command::new(runner);
                if let Err(error) =
                    configure_host_compat_runner_command(&mut command, runner, prefix.as_deref())
                {
                    log::warn!(
                        "[launch_windows_steam_game] Could not prepare the Steam shutdown command: {}",
                        error
                    );
                    return;
                }
                let _ = command
                    .arg(steam_executable)
                    .arg(SHUTDOWN_ARGUMENT)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            #[cfg(windows)]
            WindowsSteamShutdown::Native { steam_executable } => {
                log::info!(
                    "[launch_windows_steam_game] Asking Steam to shut down; r2modmac started it for this launch."
                );
                let _ = std::process::Command::new(steam_executable)
                    .arg(SHUTDOWN_ARGUMENT)
                    .spawn();
            }
        }
    }
}

pub(super) fn launch_windows_steam_game(
    game_path: &std::path::Path,
    target: &SteamLaunchTarget,
) -> Result<(), String> {
    let executable_path = find_pe_game_executable_path(game_path).ok_or_else(|| {
        "Could not find a Windows game executable in the selected folder.".to_string()
    })?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);

    if is_process_running_for_patterns(&process_patterns) {
        log::warn!(
            "[launch_windows_steam_game] Blocked: 'already running'. Patterns: {:?}",
            process_patterns
        );
        return Err("Game is already running.".to_string());
    }

    let steam_root = target.client_root.clone();
    let library_root = target.library_root.clone();
    let app_id = target.app_id.clone();
    let steam_executable = steam_root.join("steam.exe");
    if !steam_executable.exists() {
        return Err(format!(
            "Steam executable not found at {}. Check the Windows Steam directory in Settings.",
            steam_executable.display()
        ));
    }

    // Steam accepts a launch request even when it has no intention of starting
    // the game — a pending update or a paused download parks it indefinitely.
    // Report that up front rather than letting the caller wait for a timeout.
    if let Some(flags) =
        crate::commands::game_commands::steam_state::read_state_flags(&library_root, &app_id)
    {
        if let Some(blocker) =
            crate::commands::game_commands::steam_state::describe_state_blocker(flags)
        {
            log::warn!(
                "[launch_windows_steam_game] Refusing to launch app {}: StateFlags={} ({})",
                app_id,
                flags,
                blocker
            );
            return Err(blocker);
        }
    }

    // A cold Steam has to boot and sign in before it acts on the request, which
    // is much slower than a warm one and needs a different deadline (and a
    // different explanation if it runs out).
    let steam_was_running =
        is_process_running_for_patterns(&build_windows_process_match_patterns(&steam_executable));
    log::info!(
        "[launch_windows_steam_game] Steam already running: {}",
        steam_was_running
    );

    // Recorded as the client is started, and used only if the user cancels: by
    // then the request has already gone out, so ending the wait alone would
    // leave Steam booting and the game starting anyway.
    #[allow(unused_mut, unused_assignments)]
    let mut steam_shutdown: Option<WindowsSteamShutdown> = None;

    #[cfg(unix)]
    {
        let prefix_root = find_wine_prefix_root(&steam_executable)
            .or_else(|| find_wine_prefix_root(&executable_path))
            .or_else(|| find_wine_prefix_root(game_path));

        #[cfg(target_os = "macos")]
        if let Some(prefix_root_path) = prefix_root.as_deref() {
            if let Some(bundle_path) =
                find_macos_wineskin_launcher_binary(Some(prefix_root_path), &steam_executable)
            {
                let args = vec!["-applaunch".to_string(), app_id.clone()];
                match launch_macos_wineskin_program(
                    &bundle_path,
                    prefix_root_path,
                    &steam_executable,
                    &args,
                    Some(game_path),
                    "launch_windows_steam_game",
                ) {
                    Ok(()) => {
                        let timeout_ms = if steam_was_running { 60_000 } else { 180_000 };
                        match crate::commands::game_commands::steam_state::wait_for_launch_or_blocker(
                            &steam_root,
                            &library_root,
                            &app_id,
                            timeout_ms,
                            || is_process_running_for_patterns(&process_patterns),
                            crate::commands::game_commands::launch_cancel::launch_cancelled,
                        ) {
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::Started => return Ok(()),
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::Blocked(reason) => {
                                log::warn!(
                                    "[launch_windows_steam_game] Steam stalled the launch of app {}: {}",
                                    app_id,
                                    reason
                                );
                                return Err(reason);
                            }
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::Cancelled => {
                                log::info!(
                                    "[launch_windows_steam_game] Stopped waiting for app {} because the user cancelled the launch.",
                                    app_id
                                );
                                if crate::commands::game_commands::launch_cancel::cancelled_launch_should_close_steam(steam_was_running) {
                                    WindowsSteamShutdown::Wineskin {
                                        bundle: bundle_path.clone(),
                                        prefix: prefix_root_path.to_path_buf(),
                                        steam_executable: steam_executable.clone(),
                                    }
                                    .watch_until_closed(build_windows_process_match_patterns(
                                        &steam_executable,
                                    ));
                                }
                                return Err(crate::commands::game_commands::launch_cancel::LAUNCH_CANCELLED_MESSAGE.to_string());
                            }
                            crate::commands::game_commands::steam_state::LaunchWaitOutcome::TimedOut => {
                                log::warn!(
                                    "[launch_windows_steam_game] Wineskin accepted the launch request for app {}, but the game process was not observed in time.",
                                    app_id
                                );
                                return Err(if steam_was_running {
                                    "Steam accepted the launch but the game did not start. Open Steam to check for a prompt or an error waiting for you there.".to_string()
                                } else {
                                    "Steam was not running, so r2modmac started it first — but the game did not start in time. Check the Steam window and try again.".to_string()
                                });
                            }
                        }
                    }
                    Err(error) => {
                        log::warn!(
                            "[launch_windows_steam_game] Wineskin launch failed ({}); falling back to direct Wine.",
                            error
                        );
                    }
                }
            }
        }

        let runner_path = find_host_compat_runner_binary(prefix_root.as_deref(), &steam_executable)
			.ok_or_else(|| {
				"No compatible runner was found for this Steam installation. Set the game path inside a supported compatibility-tool prefix and try again."
					.to_string()
			})?;

        let mut command = std::process::Command::new(&runner_path);
        configure_host_compat_runner_command(&mut command, &runner_path, prefix_root.as_deref())?;
        configure_native_loader_dll_override(&mut command, game_path);
        log::info!(
			"[launch_windows_steam_game] Launching Steam app {} via {:?} using steam executable {:?}",
			app_id, runner_path, steam_executable
		);
        command
            .arg(&steam_executable)
            .arg("-applaunch")
            .arg(&app_id)
            .current_dir(&steam_root)
            .spawn()
            .map_err(|e| format!("Failed to launch Steam app {}: {}", app_id, e))?;

        // Same runner, same prefix: whatever started this Steam can stop it.
        steam_shutdown = Some(WindowsSteamShutdown::Runner {
            runner: runner_path.clone(),
            prefix: prefix_root.clone(),
            steam_executable: steam_executable.clone(),
        });
    }

    #[cfg(windows)]
    {
        log::info!(
            "[launch_windows_steam_game] Launching Steam app {} via {:?}",
            app_id,
            steam_executable
        );
        std::process::Command::new(&steam_executable)
            .arg("-applaunch")
            .arg(&app_id)
            .current_dir(&steam_root)
            .spawn()
            .map_err(|e| format!("Failed to launch Steam app {}: {}", app_id, e))?;

        steam_shutdown = Some(WindowsSteamShutdown::Native {
            steam_executable: steam_executable.clone(),
        });
    }

    // Steam can take the request and never create the process — parked on a
    // prompt the user cannot see while looking at r2modmac (a Steam Cloud
    // conflict, most often), or on an update it decided to fetch first. Watch
    // Steam's own state while waiting so the reason is reported as soon as it
    // is recorded, rather than after the full timeout.
    //
    // A cold Steam has to boot before it can even consider the request, which
    // takes far longer than a warm one, so the deadline accounts for that.
    let timeout_ms = if steam_was_running { 60_000 } else { 180_000 };
    match crate::commands::game_commands::steam_state::wait_for_launch_or_blocker(
        &steam_root,
        &library_root,
        &app_id,
        timeout_ms,
        || is_process_running_for_patterns(&process_patterns),
        crate::commands::game_commands::launch_cancel::launch_cancelled,
    ) {
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::Started => Ok(()),
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::Blocked(reason) => {
            log::warn!(
                "[launch_windows_steam_game] Steam stalled the launch of app {}: {}",
                app_id,
                reason
            );
            Err(reason)
        }
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::Cancelled => {
            log::info!(
                "[launch_windows_steam_game] Stopped waiting for app {} because the user cancelled the launch.",
                app_id
            );
            if crate::commands::game_commands::launch_cancel::cancelled_launch_should_close_steam(
                steam_was_running,
            ) {
                if let Some(shutdown) = steam_shutdown {
                    shutdown.watch_until_closed(build_windows_process_match_patterns(
                        &steam_executable,
                    ));
                }
            }
            Err(crate::commands::game_commands::launch_cancel::LAUNCH_CANCELLED_MESSAGE.to_string())
        }
        crate::commands::game_commands::steam_state::LaunchWaitOutcome::TimedOut => {
            log::warn!(
                "[launch_windows_steam_game] Steam accepted the launch request for app {} but the game did not start within {}ms.",
                app_id,
                timeout_ms
            );
            Err(if steam_was_running {
                "Steam accepted the launch but the game did not start. Open Steam to check for a prompt or an error waiting for you there.".to_string()
            } else {
                "Steam was not running, so r2modmac started it first — but the game did not start in time. Steam may still be signing in; check the Steam window, then press Play again.".to_string()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::configure_native_loader_dll_override;
    use std::ffi::OsStr;

    #[test]
    fn return_of_modding_loader_enables_its_own_proxy_dll_under_wine() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-rom-wine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("version.dll"), b"loader").unwrap();
        let mut command = std::process::Command::new("wine");
        configure_native_loader_dll_override(&mut command, &root);
        assert!(command.get_envs().any(|(key, value)| {
            key == OsStr::new("WINEDLLOVERRIDES") && value == Some(OsStr::new("version=n,b"))
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hell2modding_proxy_is_overridden_under_wine() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-h2m-wine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("d3d12.dll"), b"loader").unwrap();
        let mut command = std::process::Command::new("wine");
        configure_native_loader_dll_override(&mut command, &root);
        assert!(command.get_envs().any(|(key, value)| {
            key == OsStr::new("WINEDLLOVERRIDES") && value == Some(OsStr::new("d3d12=n,b"))
        }));
        std::fs::remove_dir_all(root).unwrap();
    }
}

pub(crate) fn launch_windows_game(
    app: &AppHandle,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let steam_roots = get_steam_roots_for_platform(app, true);
    log::info!(
        "[launch_windows_game] Planning launch of {:?}. Known Windows Steam roots: {:?}",
        game_path,
        steam_roots
    );

    match plan_windows_launch(&steam_roots, game_path) {
        WindowsLaunchPlan::ViaSteam(target) => launch_windows_steam_game(game_path, &target),
        WindowsLaunchPlan::Direct => launch_windows_direct_game(game_path),
        WindowsLaunchPlan::SteamClientMissing => Err(
            "This game is installed through Steam, but r2modmac could not find the Windows Steam client that owns it. Set the Windows Steam directory (the folder that contains steam.exe, inside your CrossOver/Wine bottle) in Settings and try again."
                .to_string(),
        ),
    }
}

#[cfg(all(test, unix))]
mod wine_steam_shutdown_tests {
    use super::*;

    /// The real thing, against a real Wine-hosted Steam.
    ///
    /// Ignored by default, and skipped unless pointed at a launcher, because
    /// there is no single Wine setup to test against: CrossOver, Sikarugir,
    /// Whisky, plain Wine and Windows all differ, and no one machine has them
    /// all. Anyone with one can check it on theirs by pointing these at it:
    ///
    /// ```text
    /// R2MODMAC_TEST_WINE_RUNNER="/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine" \
    /// R2MODMAC_TEST_WINE_PREFIX="/path/to/bottle-or-prefix" \
    /// R2MODMAC_TEST_STEAM_EXE="/path/to/drive_c/Program Files (x86)/Steam/steam.exe" \
    /// cargo test --lib shuts_a_wine_steam_down_for_real -- --ignored --nocapture
    /// ```
    ///
    /// It reproduces the case the log caught: Steam started, cancelled one
    /// second later — before Steam exists — and expected to be closed anyway.
    ///
    /// **Steam must be signed in for this to pass.** A client parked on the
    /// login window ignores `-shutdown` entirely (measured: still up ninety
    /// seconds later), and nothing short of killing it changes that — see
    /// `docs/stopping-a-launch.md` for why that line is not crossed. It does
    /// not matter in practice: a signed-out Steam cannot start a game either,
    /// so there is no launch left to undo.
    #[test]
    #[ignore = "starts and closes a real Wine-hosted Steam"]
    fn shuts_a_wine_steam_down_for_real() {
        let (Ok(runner), Ok(prefix), Ok(steam_executable)) = (
            std::env::var("R2MODMAC_TEST_WINE_RUNNER"),
            std::env::var("R2MODMAC_TEST_WINE_PREFIX"),
            std::env::var("R2MODMAC_TEST_STEAM_EXE"),
        ) else {
            println!(
                "skipped: set R2MODMAC_TEST_WINE_RUNNER, R2MODMAC_TEST_WINE_PREFIX and R2MODMAC_TEST_STEAM_EXE to run this"
            );
            return;
        };

        let runner = std::path::PathBuf::from(runner);
        let prefix = std::path::PathBuf::from(prefix);
        let steam_executable = std::path::PathBuf::from(steam_executable);
        let patterns = build_windows_process_match_patterns(&steam_executable);

        crate::commands::game_commands::launch_cancel::begin_launch();

        // Start Steam the way a launch does, through the same runner plumbing.
        let mut command = std::process::Command::new(&runner);
        configure_host_compat_runner_command(&mut command, &runner, Some(&prefix)).unwrap();
        // Left running deliberately: this is Steam starting up, and the point
        // of the test is what closes it.
        let mut steam = command
            .arg(&steam_executable)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("could not start Steam through the runner");

        // One second in: what the user actually does.
        std::thread::sleep(std::time::Duration::from_secs(1));
        let started = std::time::Instant::now();

        WindowsSteamShutdown::Runner {
            runner: runner.clone(),
            prefix: Some(prefix),
            steam_executable,
        }
        .watch_until_closed(patterns.clone());

        let deadline = started + std::time::Duration::from_millis(STEAM_QUIT_TOTAL_MS + 10_000);
        let mut ever_seen = false;
        let mut closed = false;
        while std::time::Instant::now() < deadline {
            if is_process_running_for_patterns_excluding(&patterns, SHUTDOWN_ARGUMENT) {
                ever_seen = true;
            } else if ever_seen {
                closed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        println!(
            "Steam came up: {}, then closed: {} after {:?}",
            ever_seen,
            closed,
            started.elapsed()
        );
        let _ = steam.try_wait();
        assert!(ever_seen, "Steam never started, so nothing was proven");
        assert!(
            closed,
            "Steam was left running after a cancel it did not see"
        );
    }
}
