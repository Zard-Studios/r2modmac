use super::*;
use tauri::command;

fn directory_contains_case_insensitive(directory: &std::path::Path, name: &str) -> bool {
    fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(|entry| entry.ok()).any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
    })
}

/// Hades II keeps its executable and Hell2Modding runtime in `Ship`, one level
/// below the Steam install directory. All install, inspection and launch code
/// consumes `get_game_path`, so resolving it here gives every phase the same
/// target instead of installing ReturnOfModding beside the wrong directory.
pub(crate) fn resolve_game_runtime_path(
    game_identifier: &str,
    install_path: &std::path::Path,
) -> std::path::PathBuf {
    if normalize_for_matching(game_identifier) != "hadesii" {
        return install_path.to_path_buf();
    }

    if directory_contains_case_insensitive(install_path, "Hades2.exe") {
        return install_path.to_path_buf();
    }

    let ship = fs::read_dir(install_path).ok().and_then(|entries| {
        entries.filter_map(|entry| entry.ok()).find_map(|entry| {
            let path = entry.path();
            let is_ship = entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case("Ship"));
            (is_ship && path.is_dir()).then_some(path)
        })
    });

    ship.filter(|path| directory_contains_case_insensitive(path, "Hades2.exe"))
        .unwrap_or_else(|| install_path.to_path_buf())
}

/// The first install in these clients' libraries whose folder matches the
/// game and that `accept` takes.
fn find_game_in_steam_roots(
    steam_roots: &[std::path::PathBuf],
    game_identifier: &str,
    accept: impl Fn(&std::path::Path) -> bool,
) -> Option<String> {
    let libraries: Vec<std::path::PathBuf> = steam_roots
        .iter()
        .flat_map(|root| get_steam_library_folders(root))
        .collect();
    find_game_in_libraries(&libraries, game_identifier, accept)
}

/// How well a `steamapps/common` folder fits a game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FolderMatch {
    /// The same name, ignoring case, spaces and punctuation.
    Exact,
    /// One name sits inside the other and they are mostly the same text,
    /// like "Lethal Company" and "Lethal Company Beta".
    Partial,
}

/// Compare two names already reduced by [`normalize_for_matching`].
///
/// A name merely containing another is not enough. "gladiomori" contains
/// "omori" and "superhotminddelete..." contains "control", and treating those
/// as the same game launched Omori for Gladio Mori and Control for Superhot.
/// So the shorter name must also make up most of the longer one.
fn folder_match_normalized(folder: &str, id: &str) -> Option<FolderMatch> {
    if folder.is_empty() || id.is_empty() {
        return None;
    }
    if folder == id {
        return Some(FolderMatch::Exact);
    }
    let (short, long) = if folder.chars().count() <= id.chars().count() {
        (folder, id)
    } else {
        (id, folder)
    };
    let mostly_the_same = short.chars().count() * 5 >= long.chars().count() * 3;
    (mostly_the_same && long.contains(short)).then_some(FolderMatch::Partial)
}

/// A saved folder that only fits the game because the matching used to be
/// too loose. Such a path was picked by mistake, not by the user, and must not
/// keep being used just because it was saved.
fn is_leftover_of_loose_matching(game_identifier: &str, path: &std::path::Path) -> bool {
    let Some(folder) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let folder = normalize_for_matching(folder);
    let id = normalize_for_matching(game_identifier);
    if folder.is_empty() || id.is_empty() || folder == id {
        return false;
    }
    (folder.contains(&id) || id.contains(&folder))
        && folder_match_normalized(&folder, &id).is_none()
}

/// [`find_game_in_steam_roots`] for library folders already known.
///
/// Folders named exactly like the game are looked at in every library before
/// any approximate match is considered, so a close name in an earlier library
/// never wins over the right folder in a later one.
fn find_game_in_libraries(
    libraries: &[std::path::PathBuf],
    game_identifier: &str,
    accept: impl Fn(&std::path::Path) -> bool,
) -> Option<String> {
    let normalized_id = normalize_for_matching(game_identifier);
    for wanted in [FolderMatch::Exact, FolderMatch::Partial] {
        for lib_folder in libraries {
            let common_path = lib_folder.join("steamapps").join("common");
            let Ok(entries) = fs::read_dir(&common_path) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let folder_name = entry.file_name().to_string_lossy().to_string();
                let normalized_folder = normalize_for_matching(&folder_name);
                if folder_match_normalized(&normalized_folder, &normalized_id) != Some(wanted) {
                    continue;
                }

                let game_path = resolve_game_runtime_path(game_identifier, &entry.path());
                if !accept(&game_path) {
                    continue;
                }
                log::debug!(
                    "[get_game_path] Found match: {} -> {}",
                    folder_name,
                    game_path.display()
                );
                return Some(game_path.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[command]
pub async fn get_game_path(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<Option<String>, String> {
    let settings = load_settings_impl(&app);
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");
    let cache_key = if let Some(p) = platform {
        format!("{}::{}", game_identifier, p)
    } else {
        game_identifier.clone()
    };

    // Check manual override first
    for key in manual_override_keys(&game_identifier, platform) {
        if let Some(path) = settings.game_paths.get(&key) {
            let path_obj = std::path::Path::new(path);
            if !path_obj.exists() {
                continue;
            }
            if is_leftover_of_loose_matching(&game_identifier, path_obj) {
                log::info!(
                    "[get_game_path] Ignoring the saved folder for {}: it was matched by mistake ({})",
                    game_identifier,
                    path
                );
                continue;
            }
            if platform.is_some()
                && key == game_identifier
                && !manual_path_matches_platform(path_obj, is_windows_profile)
            {
                log::warn!(
                    "[get_game_path] Ignoring legacy manual path due to platform mismatch: {}",
                    path
                );
                continue;
            }
            log_manual_override_once(&key, path);
            return Ok(Some(
                resolve_game_runtime_path(&game_identifier, path_obj)
                    .to_string_lossy()
                    .to_string(),
            ));
        }
    }

    let steam_paths_to_check = get_steam_roots_for_platform(&app, is_windows_profile);
    // A Windows profile can also find its game in the macOS client's own
    // libraries, as the Windows build NotProton runs. Bottles are searched
    // first, so a game that is in one keeps launching from it.
    let notproton_roots = if is_windows_profile {
        notproton_steam_roots()
    } else {
        Vec::new()
    };

    let linux_roots = linux_steam_roots();

    if steam_paths_to_check.is_empty()
        && notproton_roots.is_empty()
        && linux_roots.is_empty()
        && settings.extra_steam_libraries.is_empty()
    {
        if is_windows_profile {
            // A Windows profile means a compatibility prefix only when the host
            // is not Windows itself; on Windows the prefix wording is nonsense.
            return Err(if cfg!(target_os = "windows") {
                "No Steam path configured. Go to Settings and set your Steam directory (for example C:\\Program Files (x86)\\Steam), or set the game directory manually.".to_string()
            } else {
                "No Windows Steam path configured. Go to Settings and set your Steam directory inside the Wine/CrossOver/Wineskin prefix, or set the game directory manually.".to_string()
            });
        }
        return Err("No Steam installation found (Native macOS or CrossOver)".to_string());
    }

    let normalized_id = normalize_for_matching(&game_identifier);
    log::debug!(
        "[get_game_path] platform={:?} Looking for game: {} (normalized: {})",
        platform,
        game_identifier,
        normalized_id
    );

    let found = find_game_in_steam_roots(&steam_paths_to_check, &game_identifier, |_| true)
        .or_else(|| {
            find_game_in_steam_roots(&notproton_roots, &game_identifier, is_windows_build_dir)
        })
        .or_else(|| find_game_in_steam_roots(&linux_roots, &game_identifier, |_| true))
        .or_else(|| {
            // Libraries the user added by hand come last, so a game that
            // Steam itself lists is never shadowed by one of them.
            let extras: Vec<std::path::PathBuf> = settings
                .extra_steam_libraries
                .iter()
                .map(std::path::PathBuf::from)
                .collect();
            if is_windows_profile {
                find_game_in_libraries(&extras, &game_identifier, is_windows_build_dir)
            } else {
                find_game_in_libraries(&extras, &game_identifier, |_| true)
            }
        });
    if let Some(game_path) = found {
        if settings.game_paths.get(&cache_key) != Some(&game_path) {
            // Only this entry is written, onto the settings as they are now:
            // the search above can take a while, and the copy loaded before it
            // would undo anything changed meanwhile, such as the theme.
            let _ = update_settings_impl(&app, |current| {
                current
                    .game_paths
                    .insert(cache_key.clone(), game_path.clone());
            });
        }
        return Ok(Some(game_path));
    }

    log::debug!("[get_game_path] No match found for: {}", game_identifier);
    Ok(None)
}

/// Steam clients on a Linux desktop: the usual install, the `~/.steam`
/// symlinks it leaves behind, and the Flatpak and Snap packages.
///
/// Used only to find a game by name and to show what was found. Launching
/// does not go through these, so a Linux launch behaves exactly as it does
/// when the game folder is chosen by hand.
pub(super) fn linux_steam_roots() -> Vec<std::path::PathBuf> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    linux_steam_roots_in_home(&home)
}

fn linux_steam_roots_in_home(home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let candidates = [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.steam/steam"),
        home.join("snap/steam/common/.local/share/Steam"),
    ];

    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for candidate in candidates {
        if !candidate.join("steamapps").is_dir() {
            continue;
        }
        // The symlinks point back at the same install; keep it once.
        let resolved = fs::canonicalize(&candidate).unwrap_or(candidate);
        if !roots.contains(&resolved) {
            roots.push(resolved);
        }
    }
    roots
}

/// One Steam library folder, as Preferences lists it.
#[derive(serde::Serialize)]
pub struct SteamLibraryStatus {
    pub path: String,
    /// The folder is reachable now. A library on a drive that is not plugged
    /// in is still listed, so the user can see why its games are missing.
    pub mounted: bool,
    /// On another volume than the system one (macOS `/Volumes/...`).
    pub external: bool,
    /// Added by the user rather than listed by Steam.
    pub added_by_user: bool,
    pub games: usize,
}

/// What Steam setups this machine has, so Preferences can say what was found
/// on its own instead of asking for a folder the app can already locate.
#[derive(serde::Serialize)]
pub struct SteamSetupStatus {
    /// The native macOS Steam client is installed.
    pub native_steam_found: bool,
    /// Name of the NotProton tool registered with that client, if any.
    pub notproton: Option<String>,
    /// Windows Steam installs found in CrossOver, Whisky or similar bottles.
    pub bottle_steam_count: usize,
    pub libraries: Vec<SteamLibraryStatus>,
}

fn library_game_count(library: &std::path::Path) -> usize {
    fs::read_dir(library.join("steamapps").join("common"))
        .map(|entries| entries.filter_map(|entry| entry.ok()).count())
        .unwrap_or(0)
}

/// Library paths a client's `libraryfolders.vdf` lists, whether or not the
/// folder exists right now.
fn listed_library_paths(steam_root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(content) = fs::read_to_string(vdf) {
        if let Ok(re) = regex::Regex::new(r#""path"\s+"([^"]+)""#) {
            for cap in re.captures_iter(&content) {
                let path = std::path::PathBuf::from(&cap[1]);
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

#[command]
pub async fn get_steam_setup_status(app: AppHandle) -> Result<SteamSetupStatus, String> {
    let native_root = native_macos_steam_root();
    let notproton = native_root
        .as_deref()
        .and_then(|root| installed_notproton_tools(root).into_iter().last());
    let bottle_steam_count = if cfg!(target_os = "macos") {
        discover_windows_steam_roots().len()
    } else {
        0
    };

    let settings = load_settings_impl(&app);
    let mut libraries: Vec<SteamLibraryStatus> = Vec::new();
    let mut push = |path: std::path::PathBuf, added_by_user: bool| {
        if libraries.iter().any(|library| std::path::Path::new(&library.path) == path) {
            return;
        }
        let mounted = path.is_dir();
        libraries.push(SteamLibraryStatus {
            external: path.starts_with("/Volumes"),
            games: if mounted { library_game_count(&path) } else { 0 },
            path: path.to_string_lossy().to_string(),
            mounted,
            added_by_user,
        });
    };
    let mut roots = get_steam_roots_for_platform(&app, false);
    for root in get_steam_roots_for_platform(&app, true)
        .into_iter()
        .chain(linux_steam_roots())
    {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    for root in &roots {
        for library in listed_library_paths(root) {
            push(library, false);
        }
    }
    for extra in &settings.extra_steam_libraries {
        push(std::path::PathBuf::from(extra), true);
    }

    Ok(SteamSetupStatus {
        // Off macOS the only client is the Windows one, found among the roots.
        native_steam_found: if cfg!(target_os = "macos") {
            native_root.is_some()
        } else {
            !roots.is_empty()
        },
        notproton,
        bottle_steam_count,
        libraries,
    })
}

/// Which builds of a game are installed. Both can be true when the game sits
/// in two libraries, one per OS.
#[derive(serde::Serialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct InstalledBuilds {
    pub mac: bool,
    pub windows: bool,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq)]
pub struct InstalledGame {
    pub identifier: String,
    #[serde(flatten)]
    pub builds: InstalledBuilds,
}

/// Which build a game folder holds: a `.app` bundle is macOS, an `.exe` is
/// Windows. Anything inside a Wine prefix is Windows.
fn builds_in_folder(path: &std::path::Path) -> InstalledBuilds {
    let mut builds = InstalledBuilds::default();
    if find_wine_prefix_root(path).is_some() {
        builds.windows = true;
        return builds;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return builds;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.ends_with(".app") {
            builds.mac = true;
        } else if name.ends_with(".exe") && !name.contains("crashhandler") {
            builds.windows = true;
        }
    }
    builds
}

fn installed_games_among(
    libraries: &[std::path::PathBuf],
    identifiers: Vec<String>,
) -> Vec<InstalledGame> {
    let mut folders: Vec<(String, std::path::PathBuf)> = Vec::new();
    for library in libraries {
        let Ok(entries) = fs::read_dir(library.join("steamapps").join("common")) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            folders.push((
                normalize_for_matching(&entry.file_name().to_string_lossy()),
                entry.path(),
            ));
        }
    }
    if folders.is_empty() {
        return Vec::new();
    }

    identifiers
        .into_iter()
        .filter_map(|identifier| {
            let normalized = normalize_for_matching(&identifier);
            let mut builds = InstalledBuilds::default();
            let mut found = false;
            for (name, path) in &folders {
                if folder_match_normalized(name, &normalized).is_none() {
                    continue;
                }
                found = true;
                let here = builds_in_folder(&resolve_game_runtime_path(&identifier, path));
                builds.mac |= here.mac;
                builds.windows |= here.windows;
            }
            found.then_some(InstalledGame { identifier, builds })
        })
        .collect()
}

/// The Steam client on a Windows host: the default install folders, plus the
/// path Steam writes to the registry for custom locations. Libraries on other
/// drives come from the client's own `libraryfolders.vdf`.
#[cfg(windows)]
fn host_windows_steam_roots() -> Vec<std::path::PathBuf> {
    use std::os::windows::process::CommandExt;

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(std::path::PathBuf::from(base).join("Steam"));
        }
    }

    if let Ok(output) = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .creation_flags(0x08000000)
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(value) = text
            .lines()
            .find(|line| line.contains("SteamPath"))
            .and_then(|line| line.split("REG_SZ").nth(1))
        {
            candidates.push(std::path::PathBuf::from(value.trim()));
        }
    }

    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for candidate in candidates {
        if candidate.join("steamapps").is_dir() && !roots.contains(&candidate) {
            roots.push(candidate);
        }
    }
    roots
}

#[cfg(not(windows))]
fn host_windows_steam_roots() -> Vec<std::path::PathBuf> {
    Vec::new()
}

/// Mounted drives that may hold a Steam library: external and secondary
/// drives on macOS, Linux and Windows.
fn mounted_volume_bases() -> Vec<std::path::PathBuf> {
    let mut bases: Vec<std::path::PathBuf> = Vec::new();
    let mut add_children = |dir: std::path::PathBuf| {
        if let Ok(entries) = fs::read_dir(dir) {
            bases.extend(entries.filter_map(|e| e.ok()).map(|e| e.path()));
        }
    };

    if cfg!(target_os = "macos") {
        add_children(std::path::PathBuf::from("/Volumes"));
    } else if cfg!(target_os = "linux") {
        add_children(std::path::PathBuf::from("/mnt"));
        add_children(std::path::PathBuf::from("/media"));
        // /media/<user>/<drive> and /run/media/<user>/<drive>
        for parent in ["/media", "/run/media"] {
            if let Ok(users) = fs::read_dir(parent) {
                for user in users.filter_map(|e| e.ok()) {
                    add_children(user.path());
                }
            }
        }
    } else if cfg!(windows) {
        // C: is the system drive, whose Steam is found by its own roots.
        for letter in 'D'..='Z' {
            let drive = std::path::PathBuf::from(format!("{letter}:\\"));
            if drive.is_dir() {
                bases.push(drive);
            }
        }
    }
    bases
}

/// Steam libraries on a mounted drive that no Steam client lists, for example
/// a copied library, or one made by a client that isn't installed here.
fn libraries_on_mounted_volumes() -> Vec<std::path::PathBuf> {
    const LAYOUTS: [&str; 7] = [
        "",
        "SteamLibrary",
        "Steam",
        "Games/SteamLibrary",
        "Games/Steam",
        "Program Files (x86)/Steam",
        "Program Files/Steam",
    ];
    let mut found = Vec::new();
    for base in mounted_volume_bases() {
        for layout in LAYOUTS {
            let library = if layout.is_empty() {
                base.clone()
            } else {
                base.join(layout)
            };
            if library.join("steamapps").join("common").is_dir() && !found.contains(&library) {
                found.push(library);
            }
        }
    }
    found
}

/// Steam clients installed in a Wine prefix: `~/.wine` and the prefixes that
/// Bottles, Lutris and Heroic create. Their `libraryfolders.vdf` leads to
/// libraries on other drives.
fn wine_prefix_steam_roots() -> Vec<std::path::PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };

    let mut prefixes: Vec<std::path::PathBuf> = vec![home.join(".wine")];
    for container in [
        home.join(".local/share/bottles/bottles"),
        home.join(".var/app/com.usebottles.bottles/data/bottles/bottles"),
        home.join("Games"),
        home.join("Games/Heroic/Prefixes"),
        home.join("Games/Heroic/Prefixes/default"),
        home.join(".local/share/lutris/runners/wine/prefixes"),
    ] {
        if let Ok(entries) = fs::read_dir(container) {
            prefixes.extend(entries.filter_map(|e| e.ok()).map(|e| e.path()));
        }
    }

    let mut roots = Vec::new();
    for prefix in prefixes {
        for relative in [
            "drive_c/Program Files (x86)/Steam",
            "drive_c/Program Files/Steam",
        ] {
            let root = prefix.join(relative);
            if root.join("steamapps").is_dir() && !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

/// Which of these games are installed on this machine, and as which build.
/// It lists each library's `steamapps/common` and looks inside only the
/// folders that match a game, so the cost doesn't grow with library size.
#[command]
pub async fn get_installed_games(
    app: AppHandle,
    identifiers: Vec<String>,
) -> Result<Vec<InstalledGame>, String> {
    let settings = load_settings_impl(&app);
    let mut roots = get_steam_roots_for_platform(&app, false);
    for root in get_steam_roots_for_platform(&app, true)
        .into_iter()
        .chain(host_windows_steam_roots())
        .chain(notproton_steam_roots())
        .chain(linux_steam_roots())
        .chain(wine_prefix_steam_roots())
    {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }

    tauri::async_runtime::spawn_blocking(move || {
        let mut libraries: Vec<std::path::PathBuf> = Vec::new();
        for library in roots
            .iter()
            .flat_map(|root| get_steam_library_folders(root))
            .chain(
                settings
                    .extra_steam_libraries
                    .iter()
                    .map(std::path::PathBuf::from),
            )
            .chain(libraries_on_mounted_volumes())
        {
            if !libraries.contains(&library) {
                libraries.push(library);
            }
        }
        let mut games = installed_games_among(&libraries, identifiers.clone());

        // Games with a saved folder count too, whether the user picked it or
        // we found it earlier (GOG, Epic, any store). A `game::mac` or
        // `game::windows` key names the build; a plain key is read from the
        // folder.
        for (key, path) in &settings.game_paths {
            let path = std::path::Path::new(path);
            if !path.exists() {
                continue;
            }
            let (id, platform) = match key.split_once("::") {
                Some((id, platform)) => (id, Some(platform)),
                None => (key.as_str(), None),
            };
            let Some(identifier) = identifiers
                .iter()
                .find(|candidate| candidate.as_str() == id)
            else {
                continue;
            };
            let here = match platform {
                Some("mac") => InstalledBuilds {
                    mac: true,
                    windows: false,
                },
                Some("windows") => InstalledBuilds {
                    mac: false,
                    windows: true,
                },
                _ => builds_in_folder(path),
            };
            match games.iter_mut().find(|game| &game.identifier == identifier) {
                Some(game) => {
                    game.builds.mac |= here.mac;
                    game.builds.windows |= here.windows;
                }
                None => games.push(InstalledGame {
                    identifier: identifier.clone(),
                    builds: here,
                }),
            }
        }
        games
    })
    .await
    .map_err(|err| err.to_string())
}

#[command]
pub async fn get_game_source(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<String, String> {
    let platform = normalized_platform(platform.as_deref());
    let is_windows_profile = platform == Some("windows");

    let Some(game_path_str) = get_game_path(
        app.clone(),
        game_identifier,
        platform.map(|p| p.to_string()),
    )
    .await?
    else {
        return Ok("unknown".to_string());
    };

    Ok(infer_distribution_from_game_path(
        &app,
        std::path::Path::new(&game_path_str),
        is_windows_profile,
    ))
}

/// The library folder itself, which a folder chooser hands back if the user
/// presses Open from inside it. A loader installed there would scatter across
/// every game in the library.
fn is_steam_library_root(path: &std::path::Path) -> bool {
    let is_named_common = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("common"));

    let parent_is_steamapps = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("steamapps"));

    is_named_common && parent_is_steamapps
}

#[command]
pub async fn set_game_path(
    app: AppHandle,
    game_identifier: String,
    path: String,
    platform: Option<String>,
) -> Result<(), String> {
    if is_steam_library_root(std::path::Path::new(&path)) {
        return Err(format!(
            "{} is the Steam library that holds all your games, not a game folder. Pick the game's own folder inside it.",
            path
        ));
    }

    let key = if let Some(p) = normalized_platform(platform.as_deref()) {
        format!("{}::{}", game_identifier, p)
    } else {
        game_identifier
    };
    update_settings_impl(&app, |settings| {
        settings.game_paths.insert(key, path);
    })?;
    Ok(())
}

#[command]
pub async fn open_game_folder(
    app: AppHandle,
    game_identifier: String,
    platform: Option<String>,
) -> Result<(), String> {
    let Some(game_path_str) = get_game_path(app.clone(), game_identifier, platform).await? else {
        return Err("Game directory not found".to_string());
    };

    open::that(std::path::Path::new(&game_path_str))
        .map_err(|e| format!("Failed to open game directory: {}", e))?;
    Ok(())
}

#[command]
pub async fn find_game_executable(game_path: String) -> Result<Option<String>, String> {
    let path = std::path::Path::new(&game_path);

    // On macOS, look for .app bundles first
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".app") {
                return Ok(Some(entry.path().to_string_lossy().to_string()));
            }
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(executable_path) = find_linux_executable_path(path) {
        return Ok(Some(executable_path.to_string_lossy().to_string()));
    }

    // Fallback: look for .exe (for Wine/Proton)
    if let Some(executable_path) = find_pe_game_executable_path(path) {
        return Ok(Some(executable_path.to_string_lossy().to_string()));
    }

    Ok(None)
}

#[cfg(test)]
mod manual_game_path_tests {
    use super::{is_steam_library_root, resolve_game_runtime_path};
    use std::fs;
    use std::path::Path;

    #[test]
    fn the_library_that_holds_every_game_is_not_a_game_folder() {
        for root in [
            "/Volumes/ExternalSSD/SteamLibrary/steamapps/common",
            "/Users/x/Library/Application Support/Steam/steamapps/common",
            "/Volumes/ExternalSSD/SteamLibrary/steamapps/Common",
        ] {
            assert!(is_steam_library_root(Path::new(root)), "{root}");
        }
    }

    #[test]
    fn a_game_inside_the_library_is_accepted() {
        for game in [
            "/Volumes/ExternalSSD/SteamLibrary/steamapps/common/Inscryption",
            "/Users/x/Library/Application Support/Steam/steamapps/common/Muck",
            "/Users/x/Games/common",
        ] {
            assert!(!is_steam_library_root(Path::new(game)), "{game}");
        }
    }

    #[test]
    fn hades_ii_uses_the_ship_runtime_directory() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-hades-ship-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let install = root.join("Hades II");
        let ship = install.join("Ship");
        fs::create_dir_all(&ship).unwrap();
        fs::write(ship.join("Hades2.exe"), b"stub").unwrap();

        assert_eq!(resolve_game_runtime_path("hades-ii", &install), ship);

        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod notproton_game_path_tests {
    use super::{find_game_in_steam_roots, is_windows_build_dir};
    use std::fs;

    fn world() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-notproton-paths-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn client(root: &std::path::Path, name: &str) -> std::path::PathBuf {
        let steam = root.join(name);
        fs::create_dir_all(steam.join("steamapps/common")).unwrap();
        steam
    }

    fn game(steam: &std::path::Path, dir: &str, files: &[&str]) -> std::path::PathBuf {
        let game = steam.join("steamapps/common").join(dir);
        fs::create_dir_all(&game).unwrap();
        for file in files {
            if file.ends_with(".app") {
                fs::create_dir_all(game.join(file)).unwrap();
            } else {
                fs::write(game.join(file), b"").unwrap();
            }
        }
        game
    }

    #[test]
    fn a_game_in_a_bottle_wins_over_the_notproton_copy() {
        let root = world();
        let bottle = client(&root, "bottle/drive_c/Program Files (x86)/Steam");
        let mac = client(&root, "mac");
        let in_bottle = game(&bottle, "Lethal Company", &["Lethal Company.exe"]);
        game(&mac, "Lethal Company", &["Lethal Company.exe"]);

        let found = find_game_in_steam_roots(&[bottle], "LethalCompany", |_| true)
            .or_else(|| find_game_in_steam_roots(&[mac], "LethalCompany", is_windows_build_dir));
        assert_eq!(found, Some(in_bottle.to_string_lossy().to_string()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_macos_library_offers_only_the_windows_build() {
        let root = world();
        let mac = client(&root, "mac");
        game(
            &mac,
            "Risk of Rain 2",
            &["Risk of Rain 2.app", "Risk of Rain 2.exe"],
        );
        let windows = game(&mac, "Lethal Company", &["Lethal Company.exe"]);

        assert_eq!(
            find_game_in_steam_roots(
                std::slice::from_ref(&mac),
                "RiskofRain2",
                is_windows_build_dir
            ),
            None
        );
        assert_eq!(
            find_game_in_steam_roots(&[mac], "LethalCompany", is_windows_build_dir),
            Some(windows.to_string_lossy().to_string())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_rejected_match_does_not_hide_a_later_one() {
        let root = world();
        let mac = client(&root, "mac");
        let other = client(&root, "other");
        game(&mac, "Lethal Company", &["Lethal Company.app"]);
        let windows = game(&other, "Lethal Company", &["Lethal Company.exe"]);

        assert_eq!(
            find_game_in_steam_roots(&[mac, other], "LethalCompany", is_windows_build_dir),
            Some(windows.to_string_lossy().to_string())
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod installed_games_tests {
    use super::installed_games_among;
    use std::fs;

    #[test]
    fn installed_games_are_matched_by_folder_name_across_libraries() {
        let root = std::env::temp_dir().join(format!("r2-installed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let lib_a = root.join("a");
        let lib_b = root.join("b");
        for dir in ["Lethal Company", "Steamworks Shared"] {
            fs::create_dir_all(lib_a.join("steamapps/common").join(dir)).unwrap();
        }
        fs::create_dir_all(lib_b.join("steamapps/common/Risk of Rain 2")).unwrap();
        fs::create_dir_all(lib_a.join("steamapps/common/Lethal Company/Lethal Company.app"))
            .unwrap();
        fs::write(
            lib_b.join("steamapps/common/Risk of Rain 2/Risk of Rain 2.exe"),
            b"",
        )
        .unwrap();

        let found = installed_games_among(
            &[lib_a, lib_b, root.join("unplugged")],
            ["lethal-company", "ror2", "risk-of-rain-2", "valheim", "go"]
                .map(String::from)
                .to_vec(),
        );
        let _ = fs::remove_dir_all(&root);
        let ids: Vec<&str> = found.iter().map(|g| g.identifier.as_str()).collect();
        assert_eq!(ids, vec!["lethal-company", "risk-of-rain-2"]);
        assert!(found[0].builds.mac && !found[0].builds.windows);
        assert!(!found[1].builds.mac && found[1].builds.windows);
    }
}

#[cfg(test)]
mod game_folder_matching_tests {
    use super::find_game_in_libraries;
    use std::fs;
    use std::path::PathBuf;

    /// A Steam library holding the named folders under `steamapps/common`.
    fn library(label: &str, folders: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-folder-match-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for folder in folders {
            fs::create_dir_all(root.join("steamapps/common").join(folder)).unwrap();
        }
        root
    }

    fn found(libraries: &[PathBuf], identifier: &str) -> Option<String> {
        find_game_in_libraries(libraries, identifier, |_| true).map(|path| {
            PathBuf::from(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
    }

    #[test]
    fn a_game_is_not_taken_for_another_whose_name_is_inside_its_own() {
        // Gladio Mori and Omori: "gladiomori" contains "omori". Likewise
        // "superhotminddelete..." contains "control".
        let libraries = [library("inside", &["OMORI", "Control"])];

        assert_eq!(found(&libraries, "gladio-mori"), None);
        assert_eq!(found(&libraries, "superhot-mind-control-delete"), None);
    }

    #[test]
    fn an_exact_folder_wins_over_one_that_comes_first_in_the_listing() {
        let libraries = [library("exact", &["OMORI", "Gladio Mori"])];

        assert_eq!(
            found(&libraries, "gladio-mori").as_deref(),
            Some("Gladio Mori")
        );
        assert_eq!(found(&libraries, "omori").as_deref(), Some("OMORI"));
    }

    #[test]
    fn an_exact_folder_in_a_later_library_beats_a_loose_one_in_an_earlier_one() {
        let libraries = [
            library("first", &["Lethal Company Demo"]),
            library("second", &["Lethal Company"]),
        ];

        assert_eq!(
            found(&libraries, "lethal-company").as_deref(),
            Some("Lethal Company")
        );
    }

    #[test]
    fn names_that_differ_only_in_spelling_still_match() {
        let libraries = [library(
            "spelling",
            &[
                "Risk of Rain 2",
                "Hollow Knight Silksong",
                "20 Minutes Till Dawn",
            ],
        )];

        assert_eq!(
            found(&libraries, "riskofrain2").as_deref(),
            Some("Risk of Rain 2")
        );
        assert_eq!(
            found(&libraries, "hollow-knight-silksong").as_deref(),
            Some("Hollow Knight Silksong")
        );
        assert_eq!(
            found(&libraries, "20-minutes-till-dawn").as_deref(),
            Some("20 Minutes Till Dawn")
        );
    }

    #[test]
    fn a_folder_saved_by_the_old_loose_matching_is_recognised_as_a_mistake() {
        use super::is_leftover_of_loose_matching as leftover;
        use std::path::Path;

        // Saved by mistake in the past.
        assert!(leftover(
            "gladio-mori",
            Path::new("/steam/steamapps/common/OMORI")
        ));
        assert!(leftover(
            "superhot-mind-control-delete",
            Path::new("/steam/steamapps/common/Control")
        ));
        // The right folder, or a folder the user chose with an unrelated name.
        assert!(!leftover(
            "gladio-mori",
            Path::new("/steam/steamapps/common/Gladio Mori")
        ));
        assert!(!leftover(
            "lethal-company",
            Path::new("/steam/steamapps/common/Lethal Company Beta")
        ));
        assert!(!leftover(
            "tcg-card-shop-simulator",
            Path::new("/Users/me/Downloads/bbb")
        ));
        assert!(!leftover(
            "hades-ii",
            Path::new("/steam/steamapps/common/Hades II/Ship")
        ));
        assert!(!leftover(
            "rv-there-yet",
            Path::new("/steam/steamapps/common/Ride")
        ));
    }

    #[test]
    fn a_sequel_or_edition_is_not_the_original() {
        let libraries = [library(
            "sequels",
            &["Subnautica Below Zero", "Valheim Dedicated Server"],
        )];

        assert_eq!(found(&libraries, "subnautica"), None);
        assert_eq!(found(&libraries, "valheim"), None);
    }
}
