//! Windows games run by the macOS Steam client through NotProton.
//!
//! NotProton (https://github.com/NotProtonNot/NotProton) enables Steam Play in
//! the native macOS client and registers CrossOver as a compatibility tool, the
//! way Proton works on Linux. The game is the Windows build, installed in an
//! ordinary macOS Steam library, and it runs in a Wine prefix Steam keeps at
//! `steamapps/compatdata/<appid>/pfx` of that library. There is no bottle and no
//! Windows Steam: launching is the macOS client's job.
//!
//! So a Windows profile whose game sits in a macOS library is installed the way
//! every Windows profile is (the Windows BepInEx pack and `doorstop_config.ini`
//! beside the game, profile paths spelled through `Z:`, which a Proton-style
//! prefix maps to `/`). What differs is the launch: the loader's proxy DLL is
//! made native in the prefix's own registry, then the macOS client is asked to
//! run the game.

use super::*;

/// The folder name every NotProton build registers under
/// `compatibilitytools.d`, followed by the CrossOver version (`notproton-26.3`).
const NOTPROTON_TOOL_PREFIX: &str = "notproton";

/// Where a launch that needs the prefix stops when it is not there yet.
pub(crate) const NOTPROTON_PREFIX_MISSING_ERROR: &str = "NotProton has not created this game's Wine prefix yet. Start the game once from Steam (with NotProton selected under Properties → Compatibility), quit it, then press Play in r2modmac again.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotProtonGame {
    pub app_id: String,
    /// The macOS Steam library that holds the game, and its `compatdata`.
    pub library_root: std::path::PathBuf,
}

impl NotProtonGame {
    pub(crate) fn compat_data(&self) -> std::path::PathBuf {
        self.library_root
            .join("steamapps")
            .join("compatdata")
            .join(&self.app_id)
    }

    pub(crate) fn prefix(&self) -> std::path::PathBuf {
        self.compat_data().join("pfx")
    }
}

/// The macOS Steam client's own folder, where NotProton installs its tools.
pub(crate) fn native_macos_steam_root() -> Option<std::path::PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let root = dirs::home_dir()?.join("Library/Application Support/Steam");
    root.is_dir().then_some(root)
}

/// The NotProton tools registered with this Steam client, by folder name.
pub(crate) fn installed_notproton_tools(steam_root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(steam_root.join("compatibilitytools.d")) else {
        return Vec::new();
    };
    let mut tools: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join("compatibilitytool.vdf").is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.to_ascii_lowercase().starts_with(NOTPROTON_TOOL_PREFIX))
        .collect();
    tools.sort();
    tools
}

/// A Windows build: a game executable and no macOS app bundle beside it. Steam
/// downloads the macOS build of a game that has one unless a compatibility tool
/// is forced, so a folder holding both is the macOS build.
pub(crate) fn is_windows_build_dir(game_path: &std::path::Path) -> bool {
    if !game_path.is_dir() || find_pe_game_executable_path(game_path).is_none() {
        return false;
    }
    let has_app_bundle = fs::read_dir(game_path).is_ok_and(|entries| {
        entries.filter_map(|entry| entry.ok()).any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.to_ascii_lowercase().ends_with(".app"))
        })
    });
    !has_app_bundle
}

/// The NotProton game at `game_path`, when `steam_root` is a macOS client with
/// NotProton installed and the folder is the Windows build of one of its games.
pub(crate) fn find_notproton_game_in(
    steam_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Option<NotProtonGame> {
    // A game inside a bottle belongs to that bottle's Windows Steam.
    if find_wine_prefix_root(game_path).is_some()
        || find_wine_prefix_root(&canonicalize_or_original(game_path)).is_some()
        || installed_notproton_tools(steam_root).is_empty()
        || !is_windows_build_dir(game_path)
    {
        return None;
    }

    let (app_id, library_root) = find_steam_app_in_listed_libraries(steam_root, game_path)
        .or_else(|| {
            // A library added after Steam last wrote `libraryfolders.vdf`.
            let library_root = find_embedded_steam_library_root_for_game_path(game_path)?;
            let app_id = find_steam_app_id_for_library_root(&library_root, game_path)?;
            Some((app_id, library_root))
        })?;
    Some(NotProtonGame {
        app_id,
        library_root,
    })
}

/// [`find_notproton_game_in`] for the macOS Steam client on this Mac.
pub(crate) fn notproton_game_for_path(game_path: &std::path::Path) -> Option<NotProtonGame> {
    find_notproton_game_in(&native_macos_steam_root()?, game_path)
}

/// The macOS Steam libraries a Windows profile may find its game in: only when
/// NotProton is installed, since otherwise no Windows build there can run.
pub(crate) fn notproton_steam_roots() -> Vec<std::path::PathBuf> {
    native_macos_steam_root()
        .filter(|root| !installed_notproton_tools(root).is_empty())
        .into_iter()
        .collect()
}

/// The `WINEDLLOVERRIDES`-style list of proxies the prefix has to load from the
/// game folder, if any.
pub(crate) fn notproton_dll_overrides(
    game_path: &std::path::Path,
    shimloader: Option<&ShimloaderLaunch>,
) -> Option<String> {
    let mut overrides: Vec<String> = crate::models::loaders::wine_dll_override_value(game_path)
        .map(|value| value.split(';').map(str::to_string).collect())
        .unwrap_or_default();
    if shimloader.is_some() {
        let shim = crate::models::loaders::SHIMLOADER_DLL_OVERRIDE.to_string();
        if !overrides.contains(&shim) {
            overrides.push(shim);
        }
    }
    (!overrides.is_empty()).then(|| overrides.join(";"))
}

/// Gets the prefix ready for a launch: the proxies made native in its registry.
/// The prefix only exists after the game's first run under NotProton.
pub(crate) fn prepare_notproton_prefix(
    game: &NotProtonGame,
    overrides: Option<&str>,
) -> Result<(), String> {
    let Some(overrides) = overrides else {
        return Ok(());
    };
    let prefix = game.prefix();
    if !prefix.join("user.reg").is_file() {
        return Err(NOTPROTON_PREFIX_MISSING_ERROR.to_string());
    }
    ensure_native_dll_overrides(&prefix, overrides)?;
    Ok(())
}

/// A host path as the game reads it. The prefix's own drives are used when it
/// has them; a Proton-style prefix always maps `Z:` to `/`.
fn notproton_wine_path(prefix: &std::path::Path, path: &std::path::Path) -> String {
    map_native_path_to_wine_path(prefix, path)
        .unwrap_or_else(|| crate::commands::legacy_mod_commands::host_path_for_doorstop(path))
}

/// The arguments the game is handed after `%command%`.
pub(crate) fn notproton_game_arguments(
    game: &NotProtonGame,
    game_path: &std::path::Path,
    shimloader: Option<&ShimloaderLaunch>,
    enable_return_of_modding: bool,
) -> Vec<String> {
    let prefix = game.prefix();
    let mut arguments = Vec::new();
    if let Some(launch) = shimloader {
        for (flag, route) in [
            ("--mod-dir", "mod"),
            ("--pak-dir", "pak"),
            ("--cfg-dir", "cfg"),
            ("--overlay-dir", "overlay"),
        ] {
            let directory = launch.profile_dir.join("shimloader").join(route);
            arguments.push(flag.to_string());
            arguments.push(notproton_wine_path(&prefix, &directory));
        }
    }
    if enable_return_of_modding {
        arguments.push("--rom_modding_root_folder".to_string());
        arguments.push(notproton_wine_path(&prefix, game_path));
    }
    arguments
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_notproton_game(
    app: &AppHandle,
    game_path: &std::path::Path,
    game: &NotProtonGame,
    shimloader: Option<&ShimloaderLaunch>,
    enable_return_of_modding: bool,
) -> Result<(), String> {
    let executable_path = find_pe_game_executable_path(game_path).ok_or_else(|| {
        "Could not find a Windows game executable in the selected folder.".to_string()
    })?;
    let process_patterns = build_windows_process_match_patterns(&executable_path);
    if is_process_running_for_patterns(&process_patterns) {
        return Err("Game is already running.".to_string());
    }

    if let Some(blocker) = crate::commands::game_commands::steam_state::read_state_flags(
        &game.library_root,
        &game.app_id,
    )
    .and_then(crate::commands::game_commands::steam_state::describe_state_blocker)
    {
        log::warn!(
            "[notproton] Refusing to launch app {}: {}",
            game.app_id,
            blocker
        );
        return Err(blocker);
    }

    let overrides = notproton_dll_overrides(game_path, shimloader);
    prepare_notproton_prefix(game, overrides.as_deref())?;
    let arguments = notproton_game_arguments(game, game_path, shimloader, enable_return_of_modding);
    log::info!(
        "[notproton] Launching app {} from {} overrides={:?} arguments={:?}",
        game.app_id,
        game_path.display(),
        overrides,
        arguments
    );

    launch_via_macos_steam_and_observe(app, &game.app_id, &arguments, || {
        is_process_running_for_patterns(&process_patterns)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-notproton-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn touch(path: &std::path::Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    /// A macOS Steam client with NotProton installed and one library at its root.
    fn steam(root: &std::path::Path) -> std::path::PathBuf {
        let steam = root.join("Steam");
        touch(&steam.join("compatibilitytools.d/notproton-26.3/compatibilitytool.vdf"));
        let library = steam.to_string_lossy().replace('\\', "\\\\");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{library}\"\n\t}}\n}}\n"
            ),
        )
        .unwrap();
        steam
    }

    fn install(library: &std::path::Path, app_id: &str, dir: &str) -> std::path::PathBuf {
        fs::write(
            library.join(format!("steamapps/appmanifest_{app_id}.acf")),
            format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"),
        )
        .unwrap();
        let game = library.join("steamapps/common").join(dir);
        fs::create_dir_all(&game).unwrap();
        game
    }

    #[test]
    fn a_windows_build_in_a_macos_library_is_a_notproton_game() {
        let root = world("found");
        let steam = steam(&root);
        let game = install(&steam, "1966720", "Lethal Company");
        touch(&game.join("Lethal Company.exe"));
        touch(&game.join("UnityCrashHandler64.exe"));

        let found = find_notproton_game_in(&steam, &game).expect("a NotProton game");
        assert_eq!(found.app_id, "1966720");
        assert_eq!(
            canonicalize_or_original(&found.prefix()),
            canonicalize_or_original(&steam.join("steamapps/compatdata/1966720/pfx"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_macos_build_of_a_game_is_left_to_the_native_path() {
        let root = world("macbuild");
        let steam = steam(&root);
        let game = install(&steam, "632360", "Risk of Rain 2");
        fs::create_dir_all(game.join("Risk of Rain 2.app")).unwrap();
        touch(&game.join("Risk of Rain 2.exe"));

        assert_eq!(find_notproton_game_in(&steam, &game), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn without_notproton_installed_nothing_changes() {
        let root = world("notool");
        let steam = steam(&root);
        fs::remove_dir_all(steam.join("compatibilitytools.d")).unwrap();
        let game = install(&steam, "1966720", "Lethal Company");
        touch(&game.join("Lethal Company.exe"));

        assert_eq!(find_notproton_game_in(&steam, &game), None);
        assert!(installed_notproton_tools(&steam).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn other_compatibility_tools_are_not_notproton() {
        let root = world("othertool");
        let steam = steam(&root);
        fs::remove_dir_all(steam.join("compatibilitytools.d")).unwrap();
        touch(&steam.join("compatibilitytools.d/SomethingElse/compatibilitytool.vdf"));
        // A folder without the registration file is not a tool Steam knows.
        fs::create_dir_all(steam.join("compatibilitytools.d/notproton-leftover")).unwrap();

        assert!(installed_notproton_tools(&steam).is_empty());
        touch(&steam.join("compatibilitytools.d/NotProton-26.4/compatibilitytool.vdf"));
        assert_eq!(installed_notproton_tools(&steam), vec!["NotProton-26.4"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_folder_steam_did_not_install_is_not_a_notproton_game() {
        let root = world("manual");
        let steam = steam(&root);
        let game = root.join("Games/Lethal Company");
        touch(&game.join("Lethal Company.exe"));

        assert_eq!(find_notproton_game_in(&steam, &game), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_folder_with_only_crash_handlers_is_not_a_windows_build() {
        let root = world("nohandler");
        let steam = steam(&root);
        let game = install(&steam, "1966720", "Lethal Company");
        touch(&game.join("UnityCrashHandler64.exe"));

        assert_eq!(find_notproton_game_in(&steam, &game), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_game_inside_a_bottle_stays_with_the_bottle() {
        let root = world("bottle");
        let steam = steam(&root);
        let bottle_library = root.join("Bottles/Steam/drive_c/Program Files (x86)/Steam");
        fs::create_dir_all(bottle_library.join("steamapps")).unwrap();
        let game = install(&bottle_library, "1966720", "Lethal Company");
        touch(&game.join("Lethal Company.exe"));

        assert_eq!(find_notproton_game_in(&steam, &game), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_secondary_library_missing_from_the_list_is_still_found_by_its_manifest() {
        let root = world("secondary");
        let steam = steam(&root);
        let library = root.join("Volumes/Games/SteamLibrary");
        fs::create_dir_all(library.join("steamapps")).unwrap();
        let game = install(&library, "1966720", "Lethal Company");
        touch(&game.join("Lethal Company.exe"));

        let found = find_notproton_game_in(&steam, &game).expect("a NotProton game");
        assert_eq!(
            canonicalize_or_original(&found.library_root),
            canonicalize_or_original(&library)
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// A library on an external SSD, listed in `libraryfolders.vdf` like
    /// Steam does once it is added. Its games keep their prefix on that disk.
    #[test]
    fn a_game_on_an_external_ssd_uses_the_prefix_on_that_disk() {
        let root = world("externalssd");
        let steam = steam(&root);
        let ssd = root.join("Volumes/Extreme SSD/SteamLibrary");
        fs::create_dir_all(ssd.join("steamapps")).unwrap();
        let listed = |path: &std::path::Path| path.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                listed(&steam),
                listed(&ssd)
            ),
        )
        .unwrap();
        let game = install(&ssd, "1966720", "Lethal Company");
        touch(&game.join("Lethal Company.exe"));

        let found = find_notproton_game_in(&steam, &game).expect("a NotProton game");
        assert_eq!(
            canonicalize_or_original(&found.library_root),
            canonicalize_or_original(&ssd)
        );
        assert!(found
            .prefix()
            .starts_with(ssd.join("steamapps/compatdata/1966720")));

        // The disk unplugged: the folder is gone and nothing is found, without
        // falling back to some other copy.
        fs::remove_dir_all(root.join("Volumes")).unwrap();
        assert_eq!(find_notproton_game_in(&steam, &game), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_doorstop_proxy_is_the_override_a_bepinex_game_needs() {
        let root = world("overrides");
        let game = root.join("game");
        touch(&game.join("winhttp.dll"));
        assert_eq!(
            notproton_dll_overrides(&game, None).as_deref(),
            Some("winhttp=n,b")
        );

        let vanilla = root.join("vanilla");
        touch(&vanilla.join("winhttp.dll_DISABLED"));
        assert_eq!(notproton_dll_overrides(&vanilla, None), None);

        let shim = ShimloaderLaunch {
            profile_dir: root.join("profile"),
            binaries_dir: None,
        };
        assert_eq!(
            notproton_dll_overrides(&vanilla, Some(&shim)).as_deref(),
            Some("dwmapi=n,b")
        );
        touch(&vanilla.join("dwmapi.dll"));
        assert_eq!(
            notproton_dll_overrides(&vanilla, Some(&shim)).as_deref(),
            Some("dwmapi=n,b"),
            "the shim is listed once"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_modded_launch_before_the_first_run_explains_what_to_do() {
        let root = world("noprefix");
        let game = NotProtonGame {
            app_id: "1966720".to_string(),
            library_root: root.clone(),
        };
        assert_eq!(
            prepare_notproton_prefix(&game, Some("winhttp=n,b")),
            Err(NOTPROTON_PREFIX_MISSING_ERROR.to_string())
        );
        // Nothing to load means nothing to wait for: vanilla starts right away.
        assert_eq!(prepare_notproton_prefix(&game, None), Ok(()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_prefix_registry_makes_the_doorstop_native_once() {
        let root = world("registry");
        let game = NotProtonGame {
            app_id: "1966720".to_string(),
            library_root: root.clone(),
        };
        let user_reg = game.prefix().join("user.reg");
        fs::create_dir_all(game.prefix()).unwrap();
        fs::write(
            &user_reg,
            "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\DllOverrides] 1\n\"d3d11\"=\"native\"\n",
        )
        .unwrap();

        prepare_notproton_prefix(&game, Some("winhttp=n,b")).unwrap();
        let written = fs::read_to_string(&user_reg).unwrap();
        assert!(written.contains("\"winhttp\"=\"native,builtin\""));
        assert!(
            written.contains("\"d3d11\"=\"native\""),
            "other overrides are kept"
        );

        prepare_notproton_prefix(&game, Some("winhttp=n,b")).unwrap();
        assert_eq!(fs::read_to_string(&user_reg).unwrap(), written);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn loader_arguments_reach_the_profile_through_the_prefix_drives() {
        use std::os::unix::fs::symlink;

        let root = world("arguments");
        let game = NotProtonGame {
            app_id: "1145350".to_string(),
            library_root: root.join("Steam"),
        };
        let game_path = root.join("Steam/steamapps/common/Hades II/Ship");
        let profile = root.join("profiles/default");
        fs::create_dir_all(&game_path).unwrap();
        for route in ["mod", "pak", "cfg", "overlay"] {
            fs::create_dir_all(profile.join("shimloader").join(route)).unwrap();
        }
        fs::create_dir_all(game.prefix().join("dosdevices")).unwrap();
        symlink(&root, game.prefix().join("dosdevices/z:")).unwrap();

        let shim = ShimloaderLaunch {
            profile_dir: profile.clone(),
            binaries_dir: None,
        };
        let arguments = notproton_game_arguments(&game, &game_path, Some(&shim), true);
        assert_eq!(arguments[0], "--mod-dir");
        assert_eq!(arguments[1], "Z:\\profiles\\default\\shimloader\\mod");
        assert_eq!(arguments[8], "--rom_modding_root_folder");
        assert_eq!(arguments[9], "Z:\\Steam\\steamapps\\common\\Hades II\\Ship");
        assert_eq!(arguments.len(), 10);

        // Before the first run there are no drives yet; `Z:` is still `/`.
        fs::remove_dir_all(game.prefix()).unwrap();
        let arguments = notproton_game_arguments(&game, &game_path, None, true);
        let expected = crate::commands::legacy_mod_commands::host_path_for_doorstop(&game_path);
        assert_eq!(
            arguments,
            vec!["--rom_modding_root_folder".to_string(), expected]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_vanilla_launch_hands_the_game_no_arguments() {
        let root = world("vanillaargs");
        let game = NotProtonGame {
            app_id: "1966720".to_string(),
            library_root: root.clone(),
        };
        assert!(notproton_game_arguments(&game, &root, None, false).is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod doorstop_tests {
    use super::*;

    /// An isolated profile is handed to the game through `Z:`, which a
    /// NotProton prefix maps to `/`, and the health check follows it back.
    #[test]
    fn an_isolated_profile_is_reached_through_the_host_root() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-notproton-doorstop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let game = root.join("Steam/steamapps/common/Lethal Company");
        let profile = root.join("profiles/second");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(profile.join("BepInEx/core")).unwrap();
        fs::write(profile.join("BepInEx/core/BepInEx.Preloader.dll"), b"").unwrap();
        fs::write(
            game.join("doorstop_config.ini"),
            "[UnityDoorstop]\nenabled=true\ntargetAssembly=BepInEx\\core\\BepInEx.Preloader.dll\n",
        )
        .unwrap();

        crate::commands::mod_commands::point_game_doorstop_ini_at_tree(&game, &profile).unwrap();
        let written = fs::read_to_string(game.join("doorstop_config.ini")).unwrap();
        let expected =
            crate::commands::legacy_mod_commands::host_path_for_doorstop(&profile.join("BepInEx"));
        assert!(expected.starts_with("Z:\\"), "{expected}");
        assert!(written.contains(&format!(
            "targetAssembly={expected}\\core\\BepInEx.Preloader.dll"
        )));
        assert!(
            crate::commands::legacy_mod_commands::doorstop_config_reaches_tree(
                &game.join("doorstop_config.ini"),
                &game,
                &profile.join("BepInEx"),
            )
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod real_install_tests {
    use super::*;

    /// Skipped unless pointed at a game installed through NotProton:
    ///
    /// ```text
    /// R2MODMAC_TEST_NOTPROTON_GAME="/Volumes/SSD/SteamLibrary/steamapps/common/REPO" \
    /// cargo test --lib detects_a_real_notproton_install -- --ignored --nocapture
    /// ```
    ///
    /// Only reads: the prefix is not changed.
    #[test]
    #[ignore = "reads a real NotProton install"]
    fn detects_a_real_notproton_install() {
        let Ok(game_path) = std::env::var("R2MODMAC_TEST_NOTPROTON_GAME") else {
            println!("skipped: set R2MODMAC_TEST_NOTPROTON_GAME to run this");
            return;
        };
        let game_path = std::path::PathBuf::from(game_path);
        let steam = native_macos_steam_root().expect("the macOS Steam client");
        println!("tools: {:?}", installed_notproton_tools(&steam));
        println!("windows build: {}", is_windows_build_dir(&game_path));
        let game = notproton_game_for_path(&game_path).expect("a NotProton game");
        println!("game: {game:?}");
        println!("prefix: {}", game.prefix().display());
        println!(
            "prefix registry present: {}",
            game.prefix().join("user.reg").is_file()
        );
        println!("executable: {:?}", find_pe_game_executable_path(&game_path));
        println!(
            "doorstop path for the game: {}",
            notproton_wine_path(&game.prefix(), &game_path)
        );
        assert!(game.prefix().join("user.reg").is_file());
    }
}
