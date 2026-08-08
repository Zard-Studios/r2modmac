use super::*;

pub(super) fn find_steam_app_id_for_library_root(
    library_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Option<String> {
    let steamapps_dir = library_root.join("steamapps");
    if !steamapps_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(&steamapps_dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let app_id = match parse_manifest_value(&content, "appid") {
            Some(value) => value,
            None => continue,
        };
        let install_dir = match parse_manifest_value(&content, "installdir") {
            Some(value) => value,
            None => continue,
        };

        let manifest_game_path = library_root
            .join("steamapps")
            .join("common")
            .join(install_dir);
        if game_path_matches_install_root(game_path, &manifest_game_path) {
            return Some(app_id);
        }
    }

    None
}

pub(super) fn find_embedded_steam_library_root_for_game_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let canonical_game = canonicalize_or_original(game_path);

    for ancestor in canonical_game.ancestors() {
        let is_common = ancestor
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("common"))
            .unwrap_or(false);
        if !is_common {
            continue;
        }

        let Some(steamapps_dir) = ancestor.parent() else {
            continue;
        };
        let is_steamapps = steamapps_dir
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("steamapps"))
            .unwrap_or(false);
        if !is_steamapps {
            continue;
        }

        let Some(library_root) = steamapps_dir.parent() else {
            continue;
        };
        if find_steam_app_id_for_library_root(library_root, &canonical_game).is_some() {
            return Some(library_root.to_path_buf());
        }
    }

    None
}

/// Everything the Steam launch path needs, once the game has been matched to a
/// Steam install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SteamLaunchTarget {
    /// The Steam install that owns `steam.exe` and the console log.
    pub(super) client_root: std::path::PathBuf,
    /// The library folder holding the game and its `appmanifest_<id>.acf`.
    pub(super) library_root: std::path::PathBuf,
    pub(super) app_id: String,
}

/// How a Windows game should be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WindowsLaunchPlan {
    /// Ask Steam to launch it — the only thing that works for a Steam install.
    ViaSteam(SteamLaunchTarget),
    /// Not a Steam install (or one we can still reasonably run ourselves):
    /// hand the executable to the compatibility runner.
    Direct,
    /// A Steam install with no Steam client we can reach, and no prefix to fall
    /// back into. Launching the exe here cannot work, so say so instead.
    SteamClientMissing,
}

/// Decides how to launch a Windows game, given the Steam installs we know about.
///
/// Kept free of `AppHandle` so the whole decision — including the layout from
/// issue #25, where Steam sits in a bottle and the game sits in a library
/// outside it — can be exercised against real directories in tests.
pub(super) fn plan_windows_launch(
    steam_roots: &[std::path::PathBuf],
    game_path: &std::path::Path,
) -> WindowsLaunchPlan {
    let canonical_game = canonicalize_or_original(game_path);
    let has_client = |root: &std::path::Path| root.join("steam.exe").is_file();

    // The happy path: one of the clients we know about lists a library that
    // holds this game. A configured path that turns out to have no `steam.exe`
    // is kept only as a last resort — it must not shadow a real client, or the
    // launch fails on a setting the user got slightly wrong.
    let mut clientless_match: Option<SteamLaunchTarget> = None;
    for client_root in steam_roots {
        let Some((app_id, library_root)) =
            find_steam_app_in_listed_libraries(client_root, &canonical_game)
        else {
            continue;
        };

        let target = SteamLaunchTarget {
            client_root: client_root.clone(),
            library_root,
            app_id,
        };

        if has_client(client_root) {
            log::info!(
                "[plan_windows_launch] Steam launch: app {} in listed library {:?} via client {:?}",
                target.app_id,
                target.library_root,
                target.client_root
            );
            return WindowsLaunchPlan::ViaSteam(target);
        }

        log::warn!(
            "[plan_windows_launch] {:?} lists the game's library but has no steam.exe; keeping it only as a last resort.",
            client_root
        );
        clientless_match.get_or_insert(target);
    }

    // No client listed it, but the manifest beside the game says Steam
    // installed it. That happens when the library was added to Steam after the
    // last time it wrote `libraryfolders.vdf`, or when the library lives on a
    // path the client only knows by drive letter.
    let embedded_library = find_embedded_steam_library_root_for_game_path(&canonical_game);

    if let Some(library_root) = embedded_library.as_ref() {
        if let Some(app_id) = find_steam_app_id_for_library_root(library_root, &canonical_game) {
            let client_root = [library_root.clone()]
                .into_iter()
                .chain(library_root.parent().map(|parent| parent.to_path_buf()))
                .chain(steam_roots.iter().cloned())
                .find(|root| has_client(root));

            if let Some(client_root) = client_root {
                log::info!(
                    "[plan_windows_launch] Steam launch: app {} in unlisted library {:?} via client {:?}",
                    app_id,
                    library_root,
                    client_root
                );
                return WindowsLaunchPlan::ViaSteam(SteamLaunchTarget {
                    client_root,
                    library_root: library_root.clone(),
                    app_id,
                });
            }
        }
    }

    // Nothing has a `steam.exe`. Going through the one root that at least knows
    // the game still produces a precise "Steam executable not found at X"
    // rather than a launch that quietly does nothing.
    if let Some(target) = clientless_match {
        log::warn!(
            "[plan_windows_launch] No Steam client has steam.exe; falling back to {:?}",
            target.client_root
        );
        return WindowsLaunchPlan::ViaSteam(target);
    }

    let Some(library_root) = embedded_library else {
        log::info!(
            "[plan_windows_launch] Direct launch: {:?} is not a Steam install",
            canonical_game
        );
        return WindowsLaunchPlan::Direct;
    };

    // A Steam game with no client. Inside a prefix the direct launch at least
    // has a runtime around it and works for DRM-free titles; outside one, Wine
    // would run it against the default prefix and it would exit on the spot —
    // which is exactly the "Play button does nothing" of issue #25.
    if find_wine_prefix_root(&canonical_game).is_none() {
        log::warn!(
            "[plan_windows_launch] {:?} is a Steam install in library {:?}, but no Steam client was found and it lives outside any Wine prefix. Known roots: {:?}",
            canonical_game,
            library_root,
            steam_roots
        );
        return WindowsLaunchPlan::SteamClientMissing;
    }

    log::warn!(
        "[plan_windows_launch] Direct launch: {:?} is a Steam install in library {:?} but no Steam client was found. Known roots: {:?}",
        canonical_game,
        library_root,
        steam_roots
    );
    WindowsLaunchPlan::Direct
}

/// Resolves the app id **and** the library folder that actually holds the game,
/// looking only at the libraries this client lists.
///
/// The two are not interchangeable: a game installed in a secondary library
/// keeps its `appmanifest_<id>.acf` next to itself, not under the Steam client's
/// own `steamapps`, so anything reading the manifest (update state, download
/// state) has to look in the library, while the launch itself goes through the
/// client's root.
pub(super) fn find_steam_app_in_listed_libraries(
    steam_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Option<(String, std::path::PathBuf)> {
    let canonical_game = canonicalize_or_original(game_path);

    for library_root in get_steam_library_folders(steam_root) {
        if let Some(app_id) = find_steam_app_id_for_library_root(&library_root, &canonical_game) {
            return Some((app_id, library_root));
        }
    }

    None
}

pub(super) fn infer_distribution_from_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> String {
    let game_path = canonicalize_or_original(game_path);

    for steam_root in get_steam_roots_for_platform(app, is_windows_profile) {
        for library_root in get_steam_library_folders(&steam_root) {
            if find_steam_app_id_for_library_root(&library_root, &game_path).is_some() {
                return "steam".to_string();
            }
        }
    }

    if find_embedded_steam_library_root_for_game_path(&game_path).is_some() {
        return "steam".to_string();
    }

    "manual".to_string()
}

pub(super) fn get_steam_roots_for_platform(
    app: &AppHandle,
    is_windows_profile: bool,
) -> Vec<std::path::PathBuf> {
    let settings = load_settings_impl(app);
    let mut steam_paths_to_check = Vec::new();

    let expand_user_path = |raw: &str| -> std::path::PathBuf {
        if raw == "~" {
            return dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(raw));
        }

        if let Some(stripped) = raw.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }

        std::path::PathBuf::from(raw)
    };

    if !is_windows_profile {
        if let Some(home) = dirs::home_dir() {
            let mac_steam = home.join("Library/Application Support/Steam");
            if mac_steam.exists() {
                steam_paths_to_check.push(mac_steam);
            }
        }
    }

    let legacy_mac_steam_path = settings.steam_path.as_ref().filter(|path| {
        let lower = path.to_lowercase();
        !lower.contains("drive_c") && !lower.contains("crossover") && !lower.contains("wine")
    });

    let configured_steam_path = if is_windows_profile {
        settings
            .windows_steam_path
            .as_ref()
            .or(settings.steam_path.as_ref())
    } else {
        settings.mac_steam_path.as_ref().or(legacy_mac_steam_path)
    };

    if let Some(steam_path_str) = configured_steam_path {
        let configured_steam = expand_user_path(steam_path_str);
        if configured_steam.exists() && !steam_paths_to_check.contains(&configured_steam) {
            steam_paths_to_check.push(configured_steam);
        }
    }

    if is_windows_profile {
        for discovered in discover_windows_steam_roots() {
            if !steam_paths_to_check.contains(&discovered) {
                steam_paths_to_check.push(discovered);
            }
        }
    }

    steam_paths_to_check
}

/// Windows Steam installs found inside the compatibility prefixes on this Mac.
///
/// Users are not required to point r2modmac at their bottle's Steam, and most
/// never do — the setting exists for unusual layouts. Finding the client
/// ourselves is what lets a Steam game launch through Steam rather than being
/// dropped onto a bare `wine game.exe`, which for any Steam-DRM title just
/// exits on the spot.
pub(super) fn discover_windows_steam_roots() -> Vec<std::path::PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    discover_windows_steam_roots_in_home(&home)
}

fn discover_windows_steam_roots_in_home(home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();

    let prefix_containers = [
        home.join("Library/Application Support/CrossOver/Bottles"),
        home.join(
            "Library/Containers/com.isaacmarovitz.Whisky/Data/Library/Application Support/Bottles",
        ),
        home.join("Library/Application Support/com.isaacmarovitz.Whisky/Bottles"),
        home.join("Library/Application Support/Whisky/Bottles"),
    ];

    let mut prefix_roots: Vec<std::path::PathBuf> = Vec::new();
    for container in prefix_containers {
        let Ok(entries) = fs::read_dir(&container) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let prefix_root = entry.path();
            if prefix_root.join("drive_c").is_dir() {
                prefix_roots.push(prefix_root);
            }
        }
    }

    let default_prefix = home.join(".wine");
    if default_prefix.join("drive_c").is_dir() {
        prefix_roots.push(default_prefix);
    }

    for prefix_root in prefix_roots {
        for relative in [
            "drive_c/Program Files (x86)/Steam",
            "drive_c/Program Files/Steam",
        ] {
            let steam_root = prefix_root.join(relative);
            if steam_root.join("steam.exe").is_file() && !roots.contains(&steam_root) {
                roots.push(steam_root);
            }
        }
    }

    roots
}

pub(super) fn parse_manifest_value(content: &str, key: &str) -> Option<String> {
    let pattern = format!(r#""{}"\s+"([^"]+)""#, regex::escape(key));
    let re = regex::Regex::new(&pattern).ok()?;
    re.captures(content)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
}

pub(super) fn find_steam_app_id_for_game_path(
    steam_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Option<String> {
    for library_root in get_steam_library_folders(steam_root) {
        if let Some(app_id) = find_steam_app_id_for_library_root(&library_root, game_path) {
            return Some(app_id);
        }
    }

    None
}

pub(super) fn find_steam_app_id_for_game_path_any(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> Option<String> {
    for steam_root in get_steam_roots_for_platform(app, is_windows_profile) {
        if let Some(app_id) = find_steam_app_id_for_game_path(&steam_root, game_path) {
            return Some(app_id);
        }
    }

    if let Some(library_root) = find_embedded_steam_library_root_for_game_path(game_path) {
        if let Some(app_id) = find_steam_app_id_for_library_root(&library_root, game_path) {
            return Some(app_id);
        }
    }

    None
}

pub(super) fn find_matching_steam_root_for_game_path(
    app: &AppHandle,
    game_path: &std::path::Path,
    is_windows_profile: bool,
) -> Option<std::path::PathBuf> {
    let canonical_game = canonicalize_or_original(game_path);

    for steam_root in get_steam_roots_for_platform(app, is_windows_profile) {
        for library_root in get_steam_library_folders(&steam_root) {
            if find_steam_app_id_for_library_root(&library_root, &canonical_game).is_some() {
                return Some(steam_root);
            }
        }
    }

    if let Some(embedded_library_root) =
        find_embedded_steam_library_root_for_game_path(&canonical_game)
    {
        if embedded_library_root.join("steam.exe").exists() {
            return Some(embedded_library_root);
        }
        if let Some(parent) = embedded_library_root.parent() {
            if parent.join("steam.exe").exists() {
                return Some(parent.to_path_buf());
            }
        }
    }

    None
}

#[cfg(all(test, unix))]
mod issue_25_launch_routing_tests {
    use super::*;
    use crate::commands::game_commands::steam_state;

    /// A throwaway on-disk world: a fake home containing CrossOver bottles and
    /// Steam libraries. Everything the launch decision reads is a real file, so
    /// these tests exercise the same code the app runs, not a model of it.
    struct World {
        root: std::path::PathBuf,
        home: std::path::PathBuf,
    }

    impl World {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "r2modmac-{}-{}-{}",
                label,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let home = root.join("Users/ethan");
            std::fs::create_dir_all(&home).unwrap();
            World { root, home }
        }

        /// A CrossOver bottle, with `C:` mapped to its `drive_c` and `Z:` to the
        /// host root — which is how CrossOver actually exposes the Mac
        /// filesystem to the Windows programs inside it.
        fn crossover_bottle(&self, name: &str) -> std::path::PathBuf {
            let prefix_root = self
                .home
                .join("Library/Application Support/CrossOver/Bottles")
                .join(name);
            std::fs::create_dir_all(prefix_root.join("drive_c")).unwrap();
            std::fs::create_dir_all(prefix_root.join("dosdevices")).unwrap();
            std::os::unix::fs::symlink("../drive_c", prefix_root.join("dosdevices/c:")).unwrap();
            std::os::unix::fs::symlink("/", prefix_root.join("dosdevices/z:")).unwrap();
            prefix_root
        }

        fn steam_client(&self, prefix_root: &std::path::Path) -> std::path::PathBuf {
            let client_root = prefix_root.join("drive_c/Program Files (x86)/Steam");
            std::fs::create_dir_all(client_root.join("steamapps/common")).unwrap();
            std::fs::create_dir_all(client_root.join("logs")).unwrap();
            std::fs::write(client_root.join("steam.exe"), b"stub").unwrap();
            client_root
        }

        fn library(&self, library_root: &std::path::Path) -> std::path::PathBuf {
            std::fs::create_dir_all(library_root.join("steamapps/common")).unwrap();
            library_root.to_path_buf()
        }

        fn install_game(
            &self,
            library_root: &std::path::Path,
            app_id: &str,
            install_dir: &str,
            state_flags: u64,
        ) -> std::path::PathBuf {
            let game_path = library_root.join("steamapps/common").join(install_dir);
            std::fs::create_dir_all(&game_path).unwrap();
            std::fs::write(game_path.join(format!("{}.exe", install_dir)), b"stub").unwrap();
            std::fs::write(
                library_root
                    .join("steamapps")
                    .join(format!("appmanifest_{}.acf", app_id)),
                format!(
                    "\"AppState\"\n{{\n\t\"appid\"\t\t\"{}\"\n\t\"installdir\"\t\t\"{}\"\n\t\"StateFlags\"\t\t\"{}\"\n}}",
                    app_id, install_dir, state_flags
                ),
            )
            .unwrap();
            game_path
        }

        /// Record a library in the client's `libraryfolders.vdf` the way a
        /// bottled Steam does: as a `Z:` path with escaped separators.
        fn list_libraries_in_vdf(
            &self,
            client_root: &std::path::Path,
            libraries: &[&std::path::Path],
        ) {
            let mut vdf = String::from("\"libraryfolders\"\n{\n");
            for (index, library) in libraries.iter().enumerate() {
                let windows_path = if library.starts_with(client_root) {
                    "C:\\\\Program Files (x86)\\\\Steam".to_string()
                } else {
                    format!("Z:{}", library.to_string_lossy().replace('/', "\\\\"))
                };
                vdf.push_str(&format!(
                    "\t\"{}\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n",
                    index, windows_path
                ));
            }
            vdf.push('}');
            std::fs::write(client_root.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn canonical(path: &std::path::Path) -> std::path::PathBuf {
        std::fs::canonicalize(path).unwrap()
    }

    fn steam_target(plan: WindowsLaunchPlan) -> SteamLaunchTarget {
        match plan {
            WindowsLaunchPlan::ViaSteam(target) => target,
            other => panic!("expected a Steam launch, got {:?}", other),
        }
    }

    /// The reported layout, end to end: Steam in a CrossOver bottle, the game in
    /// `~/WindowsSteam` outside it. Before the fix the `Z:` library was
    /// unreadable, no client matched, and the game was handed to a bare
    /// `wine Lethal Company.exe` — the launch that "does nothing".
    #[test]
    fn issue_25_layout_launches_through_steam_rather_than_a_bare_wine_call() {
        let world = World::new("issue25");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let library = world.library(&world.home.join("WindowsSteam"));
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 4);
        world.list_libraries_in_vdf(&client_root, &[&client_root, &library]);

        let target = steam_target(plan_windows_launch(
            &discover_windows_steam_roots_in_home(&world.home),
            &game_path,
        ));

        assert_eq!(target.app_id, "1966720");
        assert_eq!(canonical(&target.client_root), canonical(&client_root));
        assert_eq!(canonical(&target.library_root), canonical(&library));
    }

    /// Same layout, but the client never wrote the library into its `.vdf`
    /// (a bottle restored from a backup, or a library added since). The
    /// manifest beside the game is enough to know Steam owns it.
    #[test]
    fn a_library_the_client_does_not_list_still_launches_through_steam() {
        let world = World::new("unlisted");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let library = world.library(&world.home.join("WindowsSteam"));
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 4);

        let target = steam_target(plan_windows_launch(
            &discover_windows_steam_roots_in_home(&world.home),
            &game_path,
        ));

        assert_eq!(target.app_id, "1966720");
        assert_eq!(canonical(&target.client_root), canonical(&client_root));
        assert_eq!(canonical(&target.library_root), canonical(&library));
    }

    /// The classic layout — game inside the client's own library — must keep
    /// working, and must report the client root as the library root.
    #[test]
    fn a_game_in_the_clients_own_library_is_unaffected() {
        let world = World::new("classic");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let game_path = world.install_game(&client_root, "1966720", "Lethal Company", 4);

        let target = steam_target(plan_windows_launch(std::slice::from_ref(&client_root), &game_path));

        assert_eq!(target.app_id, "1966720");
        assert_eq!(canonical(&target.client_root), canonical(&client_root));
        assert_eq!(canonical(&target.library_root), canonical(&client_root));
    }

    /// A Steam game with no client anywhere and no prefix around it. Running
    /// the exe would start Wine against the default prefix and exit instantly,
    /// so the plan has to say what is missing instead.
    #[test]
    fn a_steam_game_with_no_reachable_client_is_reported_not_launched() {
        let world = World::new("noclient");
        let library = world.library(&world.home.join("WindowsSteam"));
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 4);

        assert_eq!(
            plan_windows_launch(
                &discover_windows_steam_roots_in_home(&world.home),
                &game_path
            ),
            WindowsLaunchPlan::SteamClientMissing
        );
    }

    /// Inside a prefix the direct launch still has a runtime around it and
    /// works for DRM-free titles, so it must stay available.
    #[test]
    fn a_steam_game_inside_a_prefix_still_falls_back_to_a_direct_launch() {
        let world = World::new("inprefix");
        let bottle = world.crossover_bottle("Games");
        let library = world.library(&bottle.join("drive_c/SteamLibrary"));
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 4);

        assert_eq!(
            plan_windows_launch(&[], &game_path),
            WindowsLaunchPlan::Direct
        );
    }

    #[test]
    fn a_game_that_steam_never_installed_is_launched_directly() {
        let world = World::new("nonsteam");
        let game_path = world.home.join("Games/Some Game");
        std::fs::create_dir_all(&game_path).unwrap();
        std::fs::write(game_path.join("Some Game.exe"), b"stub").unwrap();

        assert_eq!(
            plan_windows_launch(
                &discover_windows_steam_roots_in_home(&world.home),
                &game_path
            ),
            WindowsLaunchPlan::Direct
        );
    }

    /// A `Z:` entry pointing at a library that no longer exists must not be
    /// mistaken for a launch target.
    #[test]
    fn a_library_that_no_longer_exists_is_not_offered_as_a_target() {
        let world = World::new("stalevdf");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let missing = world.home.join("OldDrive");
        world.list_libraries_in_vdf(&client_root, &[&client_root, &missing]);

        let folders = get_steam_library_folders(&client_root);

        assert!(
            !folders.iter().any(|folder| folder.ends_with("OldDrive")),
            "a deleted library must not be reported: {:?}",
            folders
        );
    }

    /// Two games in one library: the plan must carry the id of the one asked
    /// for, not whichever manifest happened to be read first.
    #[test]
    fn the_app_id_belongs_to_the_game_that_was_asked_for() {
        let world = World::new("twogames");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let library = world.library(&world.home.join("WindowsSteam"));
        world.install_game(&library, "1966720", "Lethal Company", 4);
        let peak = world.install_game(&library, "3527290", "PEAK", 4);
        world.list_libraries_in_vdf(&client_root, &[&client_root, &library]);

        let target = steam_target(plan_windows_launch(
            &discover_windows_steam_roots_in_home(&world.home),
            &peak,
        ));

        assert_eq!(target.app_id, "3527290");
    }

    /// The blocker detection added in 0.8.5 read the manifest from the client's
    /// own `steamapps`, so for a game in a secondary library it always found
    /// nothing — the exact case in issue #25. It has to read the library the
    /// game lives in.
    #[test]
    fn a_pending_update_is_detected_in_the_library_holding_the_game() {
        let world = World::new("blockedstate");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let library = world.library(&world.home.join("WindowsSteam"));
        // 1030 = Fully Installed + Update Required + Update Started.
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 1030);
        world.list_libraries_in_vdf(&client_root, &[&client_root, &library]);

        let target = steam_target(plan_windows_launch(
            &discover_windows_steam_roots_in_home(&world.home),
            &game_path,
        ));

        let blocker = steam_state::explain_stalled_launch(
            &target.client_root,
            &target.library_root,
            &target.app_id,
        )
        .expect("a pending update must be reported");
        assert!(blocker.contains("updating"), "{blocker}");

        // The pre-fix behaviour, kept as the contrast that makes the test mean
        // something: reading the client root sees no manifest at all.
        assert_eq!(
            steam_state::read_state_flags(&target.client_root, &target.app_id),
            None
        );
    }

    /// A `windows_steam_path` pointing one level off (at a library rather than
    /// at the folder with `steam.exe`) used to win purely by being first in the
    /// list, and the launch then died on "Steam executable not found".
    #[test]
    fn a_configured_path_without_steam_exe_does_not_shadow_the_real_client() {
        let world = World::new("misconfigured");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        let library = world.library(&world.home.join("WindowsSteam"));
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 4);
        world.list_libraries_in_vdf(&client_root, &[&client_root, &library]);

        // The user pointed Settings at the library itself — no steam.exe there.
        let mut roots = vec![library.clone()];
        roots.extend(discover_windows_steam_roots_in_home(&world.home));

        let target = steam_target(plan_windows_launch(&roots, &game_path));

        assert_eq!(canonical(&target.client_root), canonical(&client_root));
        assert_eq!(canonical(&target.library_root), canonical(&library));
    }

    /// …but when that misconfigured path is all there is, it still has to be
    /// used, so the user gets "Steam executable not found at X" instead of a
    /// launch that silently does nothing.
    #[test]
    fn a_misconfigured_path_is_still_used_when_no_real_client_exists() {
        let world = World::new("onlymisconfigured");
        let library = world.library(&world.home.join("WindowsSteam"));
        let game_path = world.install_game(&library, "1966720", "Lethal Company", 4);

        let target = steam_target(plan_windows_launch(std::slice::from_ref(&library), &game_path));

        assert_eq!(canonical(&target.client_root), canonical(&library));
        assert_eq!(target.app_id, "1966720");
    }

    #[test]
    fn discovery_finds_the_steam_client_inside_a_crossover_bottle() {
        let world = World::new("discovery");
        let bottle = world.crossover_bottle("Steam");
        let client_root = world.steam_client(&bottle);
        // A second bottle with no Steam in it must not produce a false hit.
        world.crossover_bottle("Empty");

        let discovered = discover_windows_steam_roots_in_home(&world.home);

        assert_eq!(discovered.len(), 1, "{:?}", discovered);
        assert_eq!(canonical(&discovered[0]), canonical(&client_root));
    }
}
