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
/// The folder must hold the whole name of the game. A folder named after only
/// part of it is another game: "Hollow Knight" is not Hollow Knight: Silksong
/// (#65), "OMORI" is not Gladio Mori, "Control" is not Superhot: Mind Control
/// Delete. A longer folder, like "Lethal Company Beta" for Lethal Company, still
/// counts when the game's name makes up most of it.
fn folder_match_normalized(folder: &str, id: &str) -> Option<FolderMatch> {
    if folder.is_empty() || id.is_empty() {
        return None;
    }
    if folder == id {
        return Some(FolderMatch::Exact);
    }
    let mostly_the_same = id.chars().count() * 5 >= folder.chars().count() * 3;
    (mostly_the_same && folder.contains(id)).then_some(FolderMatch::Partial)
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

/// Whether a folder still holds a game. Steam keeps `steamapps/common/<Game>`
/// after an uninstall when it has saves, logs or mod files in it, and a mod
/// manager can leave files of its own. None of that is a game, so such a
/// folder must not count as installed, whatever profile or saved path points
/// at it. A file path is taken as it is.
fn holds_a_game(path: &std::path::Path) -> bool {
    const LEFTOVER_DIRS: [&str; 9] = [
        "saves",
        "savesdir",
        "saveimages",
        "savegames",
        "logs",
        "log",
        "bepinex",
        "_commonredist",
        "crashes",
    ];
    const LEFTOVER_FILE_EXTENSIONS: [&str; 8] =
        ["log", "txt", "md", "xml", "json", "vdf", "bak", "ini"];

    if path.is_file() {
        return true;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.filter_map(|entry| entry.ok()).any(|entry| {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with('.') || name.starts_with("r2modmac") {
            return false;
        }
        // Steam keeps controller layouts in folders named by account number.
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if entry.path().is_dir() {
            return !LEFTOVER_DIRS.contains(&name.as_str());
        }
        let extension = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
        !LEFTOVER_FILE_EXTENSIONS.contains(&extension)
    })
}

/// The names an executable from the schema can have on disk. The schema mostly
/// lists the Windows file (`Muck.exe`); the macOS and Linux builds of the same
/// game are usually the same name as an app bundle or a bare binary
/// (`Muck.app`, `Muck.x86_64`), and are accepted too.
fn executable_candidates(exe: &str) -> Vec<String> {
    let exe = exe.trim().replace('\\', "/");
    let mut names = vec![exe.to_lowercase()];
    let stem = exe
        .strip_suffix(".exe")
        .or_else(|| exe.strip_suffix(".EXE"))
        .or_else(|| exe.strip_suffix(".app"))
        .unwrap_or(&exe);
    for variant in [
        stem.to_string(),
        format!("{stem}.exe"),
        format!("{stem}.app"),
        format!("{stem}.x86_64"),
        format!("{stem}.x86"),
    ] {
        let variant = variant.to_lowercase();
        if !names.contains(&variant) {
            names.push(variant);
        }
    }
    names
}

/// Whether a folder holds the game the schema describes: one of its
/// executables, in the folder or one level down (Hades II keeps `Hades2.exe`
/// in `Ship`). A macOS build is an app bundle, so any `.app` counts as well,
/// in case the bundle is named differently from the Windows file. A folder with
/// only saves, logs or a mod loader's files has none of these.
fn holds_schema_executable(folder: &std::path::Path, exe_names: &[String]) -> bool {
    let candidates: Vec<String> = exe_names
        .iter()
        .flat_map(|exe| executable_candidates(exe))
        .collect();
    let matches_in = |dir: &std::path::Path| -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        entries.filter_map(|entry| entry.ok()).any(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let is_dir = entry.path().is_dir();
            if name.ends_with(".app") {
                return is_dir;
            }
            // A bare name must be the binary, not a folder of the same name.
            !is_dir && candidates.contains(&name)
        })
    };
    // Names with a folder in them (`Ship/Hades2.exe`) are checked as written.
    if exe_names.iter().any(|exe| {
        let exe = exe.replace('\\', "/");
        exe.contains('/') && folder.join(&exe).is_file()
    }) {
        return true;
    }
    if matches_in(folder) {
        return true;
    }
    fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.')
                })
                .any(|entry| matches_in(&entry.path()))
        })
        .unwrap_or(false)
}

/// What one Steam library holds, read once: the folders under
/// `steamapps/common` (with their names reduced for comparing) and the app ids
/// Steam has a manifest for.
struct LibraryIndex {
    library: std::path::PathBuf,
    folders: Vec<(String, std::path::PathBuf)>,
    manifest_ids: HashSet<String>,
}

fn index_libraries(libraries: &[std::path::PathBuf]) -> Vec<LibraryIndex> {
    libraries
        .iter()
        .map(|library| {
            let steamapps = library.join("steamapps");
            let folders = fs::read_dir(steamapps.join("common"))
                .map(|entries| {
                    entries
                        .filter_map(|entry| entry.ok())
                        .map(|entry| {
                            (
                                normalize_for_matching(&entry.file_name().to_string_lossy()),
                                entry.path(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let manifest_ids = fs::read_dir(&steamapps)
                .map(|entries| {
                    entries
                        .filter_map(|entry| entry.ok())
                        .filter_map(|entry| {
                            let name = entry.file_name().to_string_lossy().to_string();
                            name.strip_prefix("appmanifest_")
                                .and_then(|rest| rest.strip_suffix(".acf"))
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            LibraryIndex {
                library: library.clone(),
                folders,
                manifest_ids,
            }
        })
        .collect()
}

/// The folder Steam recorded for an app, when the app is in this library.
fn installed_folder_for_app(index: &LibraryIndex, app_id: &str) -> Option<std::path::PathBuf> {
    if !index.manifest_ids.contains(app_id) {
        return None;
    }
    let manifest = index
        .library
        .join("steamapps")
        .join(format!("appmanifest_{app_id}.acf"));
    let install_dir = parse_manifest_value(&fs::read_to_string(manifest).ok()?, "installdir")?;
    let folder = index
        .library
        .join("steamapps")
        .join("common")
        .join(install_dir);
    folder.is_dir().then_some(folder)
}

/// Where a game's folder could be in these libraries, from the surest way of
/// knowing to the least sure:
///
/// 1. the app id: Steam's own record of where it put the game;
/// 2. the install folder the ecosystem schema names for the game;
/// 3. a folder named exactly like the game;
/// 4. a folder whose name is mostly the game's name.
///
/// Each step looks in every library before the next one is tried, so a close
/// name in an earlier library never beats the right folder in a later one.
fn game_folder_tiers(
    index: &[LibraryIndex],
    game_identifier: &str,
) -> [Vec<std::path::PathBuf>; 4] {
    let normalized_id = normalize_for_matching(game_identifier);
    let steam = crate::models::loaders::steam_game(game_identifier);

    let mut by_app_id: Vec<std::path::PathBuf> = Vec::new();
    let mut by_schema_folder: Vec<std::path::PathBuf> = Vec::new();
    if let Some(steam) = &steam {
        for library in index {
            for app_id in &steam.app_ids {
                if let Some(folder) = installed_folder_for_app(library, app_id) {
                    if !by_app_id.contains(&folder) {
                        by_app_id.push(folder);
                    }
                }
            }
        }
        let wanted: Vec<String> = steam
            .folders
            .iter()
            .map(|folder| normalize_for_matching(folder))
            .filter(|folder| !folder.is_empty())
            .collect();
        for library in index {
            for (name, path) in &library.folders {
                if wanted.contains(name) && !by_schema_folder.contains(path) {
                    by_schema_folder.push(path.clone());
                }
            }
        }
    }

    let named = |wanted: FolderMatch| -> Vec<std::path::PathBuf> {
        index
            .iter()
            .flat_map(|library| library.folders.iter())
            .filter(|(name, _)| folder_match_normalized(name, &normalized_id) == Some(wanted))
            .map(|(_, path)| path.clone())
            .collect()
    };
    // A folder Steam left behind after an uninstall is not the game.
    // When the schema knows the game's app id and folder, a name that merely
    // looks alike is not a reason to take another folder for it.
    let schema_knows_game = steam
        .as_ref()
        .is_some_and(|steam| !steam.app_ids.is_empty() || !steam.folders.is_empty());
    let mut tiers = [
        by_app_id,
        by_schema_folder,
        named(FolderMatch::Exact),
        if schema_knows_game {
            Vec::new()
        } else {
            named(FolderMatch::Partial)
        },
    ];
    for tier in &mut tiers {
        tier.retain(|folder| holds_a_game(folder));
    }
    // Steam's own record (the first step) says the game is installed. A folder
    // found any other way must also hold one of the game's executables, when
    // the schema names them: a folder can outlive the game, or belong to
    // another one.
    if let Some(steam) = steam.as_ref().filter(|steam| !steam.exe_names.is_empty()) {
        for tier in tiers.iter_mut().skip(1) {
            tier.retain(|folder| holds_schema_executable(folder, &steam.exe_names));
        }
    }
    tiers
}

/// [`find_game_in_steam_roots`] for library folders already known.
fn find_game_in_libraries(
    libraries: &[std::path::PathBuf],
    game_identifier: &str,
    accept: impl Fn(&std::path::Path) -> bool,
) -> Option<String> {
    let index = index_libraries(libraries);
    for tier in game_folder_tiers(&index, game_identifier) {
        for folder in tier {
            let game_path = resolve_game_runtime_path(game_identifier, &folder);
            if !accept(&game_path) {
                continue;
            }
            log::debug!(
                "[get_game_path] Found {} at {}",
                game_identifier,
                game_path.display()
            );
            return Some(game_path.to_string_lossy().to_string());
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
            if !holds_a_game(path_obj) {
                log::info!(
                    "[get_game_path] Ignoring the saved folder for {}: nothing of the game is in it ({})",
                    game_identifier,
                    path
                );
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
    /// Where the native macOS Steam client was found, or on other systems the
    /// Steam client that was.
    pub native_steam_path: Option<String>,
    /// Where each Windows Steam install in a bottle was found.
    pub bottle_steam_paths: Vec<String>,
    pub libraries: Vec<SteamLibraryStatus>,
}

fn library_game_count(library: &std::path::Path) -> usize {
    fs::read_dir(library.join("steamapps").join("common"))
        .map(|entries| entries.filter_map(|entry| entry.ok()).count())
        .unwrap_or(0)
}

/// The same folder, even when one path reaches it through a symlink.
fn same_folder(a: &std::path::Path, b: &std::path::Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// A library path as `libraryfolders.vdf` writes it, turned into one that
/// means something on this Mac.
///
/// A Steam inside a Wine bottle records its libraries as Windows paths
/// (`C:\\Program Files (x86)\\Steam`, with every backslash doubled). Left as
/// they are they look like folders that are not there; mapped through the
/// bottle's drives they are the real folder, often the one already listed.
fn library_path_from_vdf(prefix_root: Option<&std::path::Path>, raw: &str) -> std::path::PathBuf {
    let direct = std::path::PathBuf::from(raw);
    if direct.exists() {
        return direct;
    }
    if let Some(mapped) = prefix_root.and_then(|prefix| map_wine_path_to_native_path(prefix, raw)) {
        return mapped;
    }
    // Not reachable from here: show it the way a person would write it.
    std::path::PathBuf::from(raw.replace("\\\\", "\\"))
}

/// Library paths a client's `libraryfolders.vdf` lists, whether or not the
/// folder exists right now.
fn listed_library_paths(steam_root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = vec![steam_root.to_path_buf()];
    let prefix_root = find_wine_prefix_root(steam_root);
    let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(content) = fs::read_to_string(vdf) {
        if let Ok(re) = regex::Regex::new(r#""path"\s+"([^"]+)""#) {
            for cap in re.captures_iter(&content) {
                let path = library_path_from_vdf(prefix_root.as_deref(), &cap[1]);
                if !paths.iter().any(|known| same_folder(known, &path)) {
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
    let bottle_roots = if cfg!(target_os = "macos") {
        discover_windows_steam_roots()
    } else {
        Vec::new()
    };
    let bottle_steam_count = bottle_roots.len();

    let settings = load_settings_impl(&app);
    let mut libraries: Vec<SteamLibraryStatus> = Vec::new();
    let mut push = |path: std::path::PathBuf, added_by_user: bool| {
        if libraries
            .iter()
            .any(|library| same_folder(std::path::Path::new(&library.path), &path))
        {
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
        native_steam_path: if cfg!(target_os = "macos") {
            native_root.map(|root| root.to_string_lossy().to_string())
        } else {
            roots.first().map(|root| root.to_string_lossy().to_string())
        },
        notproton,
        bottle_steam_count,
        bottle_steam_paths: bottle_roots
            .iter()
            .map(|root| root.to_string_lossy().to_string())
            .collect(),
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
    let index = index_libraries(libraries);
    if index.iter().all(|library| library.folders.is_empty()) {
        return Vec::new();
    }

    identifiers
        .into_iter()
        .filter_map(|identifier| {
            // The surest way of knowing that finds anything decides: the
            // folders of one step, in every library, are the game's builds.
            let folders = game_folder_tiers(&index, &identifier)
                .into_iter()
                .find(|tier| !tier.is_empty())?;
            let mut builds = InstalledBuilds::default();
            for folder in folders {
                let here = builds_in_folder(&resolve_game_runtime_path(&identifier, &folder));
                builds.mac |= here.mac;
                builds.windows |= here.windows;
            }
            Some(InstalledGame { identifier, builds })
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

/// What the saved game folders say about which games are installed.
struct SavedFolders {
    /// Games with a saved folder that exists, and which build it holds.
    games: Vec<(String, InstalledBuilds)>,
    /// Saved folders that only fit their game under the old, looser matching.
    /// Gladio Mori saved as Omori's folder is the case that was reported.
    mistaken_keys: Vec<String>,
}

/// Read `game_paths` from the settings. A `game::mac` or `game::windows` key
/// names the build; a plain key is read from the folder.
fn saved_game_folders(
    game_paths: &std::collections::BTreeMap<String, String>,
    identifiers: &[String],
) -> SavedFolders {
    let mut saved = SavedFolders {
        games: Vec::new(),
        mistaken_keys: Vec::new(),
    };
    for (key, path) in game_paths {
        let (id, platform) = match key.split_once("::") {
            Some((id, platform)) => (id, Some(platform)),
            None => (key.as_str(), None),
        };
        let path = std::path::Path::new(path);
        if is_leftover_of_loose_matching(id, path) {
            saved.mistaken_keys.push(key.clone());
            continue;
        }
        if !path.exists()
            || !holds_a_game(path)
            || !identifiers.iter().any(|candidate| candidate == id)
        {
            continue;
        }
        // A saved folder without the game's executable is a leftover too.
        if path.is_dir() {
            if let Some(steam) =
                crate::models::loaders::steam_game(id).filter(|steam| !steam.exe_names.is_empty())
            {
                if !holds_schema_executable(path, &steam.exe_names) {
                    continue;
                }
            }
        }
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
        saved.games.push((id.to_string(), here));
    }
    saved
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

    let app_for_cleanup = app.clone();
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
        // we found it earlier (GOG, Epic, any store). Folders the old, looser
        // matching saved by mistake do not, and are dropped from the settings.
        let saved = saved_game_folders(&settings.game_paths, &identifiers);
        if !saved.mistaken_keys.is_empty() {
            let _ = update_settings_impl(&app_for_cleanup, |current| {
                for key in &saved.mistaken_keys {
                    current.game_paths.remove(key);
                }
            });
        }
        for (identifier, here) in saved.games {
            match games.iter_mut().find(|game| game.identifier == identifier) {
                Some(game) => {
                    game.builds.mac |= here.mac;
                    game.builds.windows |= here.windows;
                }
                None => games.push(InstalledGame {
                    identifier,
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
            let game = root.join("steamapps/common").join(folder);
            fs::create_dir_all(&game).unwrap();
            // A folder with nothing in it is a leftover, not an installed
            // game: put the game's executable in it, named like the folder.
            fs::write(game.join(format!("{folder}.exe")), b"stub").unwrap();
        }
        root
    }

    /// Put a game's real executable in a test folder, when it isn't named
    /// like the folder.
    fn with_exe(library: &PathBuf, folder: &str, exe: &str) {
        fs::write(
            library.join("steamapps/common").join(folder).join(exe),
            b"stub",
        )
        .unwrap();
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
    fn a_sequel_is_not_taken_for_the_game_it_follows() {
        // Issue #65: Hollow Knight installed, Silksong reported as installed.
        let libraries = [library("sequel", &["Hollow Knight"])];
        assert_eq!(found(&libraries, "hollow-knight-silksong"), None);
        assert_eq!(
            found(&libraries, "hollow-knight").as_deref(),
            Some("Hollow Knight")
        );
        let installed = super::installed_games_among(
            &libraries,
            vec![
                "hollow-knight-silksong".to_string(),
                "hollow-knight".to_string(),
            ],
        );
        let ids: Vec<&str> = installed
            .iter()
            .map(|game| game.identifier.as_str())
            .collect();
        assert_eq!(ids, vec!["hollow-knight"]);
    }

    #[test]
    fn the_sequel_is_found_in_its_own_folder() {
        let libraries = [library(
            "sequel-own",
            &["Hollow Knight", "Hollow Knight Silksong"],
        )];
        assert_eq!(
            found(&libraries, "hollow-knight-silksong").as_deref(),
            Some("Hollow Knight Silksong")
        );
    }

    #[test]
    fn a_shorter_folder_is_not_a_game_the_schema_does_not_know() {
        // Same rule without schema data: the folder must hold the whole name.
        // "foobar" makes up 60% of "foobarplus", which used to be enough.
        let libraries = [library("unknown", &["Foo Bar"])];
        assert_eq!(found(&libraries, "foo-bar-plus"), None);
        // A longer folder that holds the whole name still counts.
        let longer = [library("unknown-longer", &["Foo Bar Plus Beta"])];
        assert_eq!(
            found(&longer, "foo-bar-plus").as_deref(),
            Some("Foo Bar Plus Beta")
        );
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
        with_exe(&libraries[0], "20 Minutes Till Dawn", "MinutesTillDawn.exe");

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

    /// Steam's own record of where it put an app.
    fn with_manifest(library: &PathBuf, app_id: &str, install_dir: &str) {
        let steamapps = library.join("steamapps");
        fs::create_dir_all(&steamapps).unwrap();
        fs::write(
            steamapps.join(format!("appmanifest_{app_id}.acf")),
            format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"installdir\"\t\t\"{install_dir}\"\n}}\n"),
        )
        .unwrap();
    }

    #[test]
    fn a_folder_that_looks_nothing_like_the_name_is_found_through_the_schema() {
        // RV There Yet? is installed in "Ride".
        let libraries = [library("schema-folder", &["Ride", "OMORI"])];
        assert_eq!(found(&libraries, "rv-there-yet").as_deref(), Some("Ride"));

        // 20 Minutes Till Dawn is in "20MinuteTillDawn": no name comparison gets there.
        let libraries = [library("schema-typo", &["20MinuteTillDawn"])];
        with_exe(&libraries[0], "20MinuteTillDawn", "MinutesTillDawn.exe");
        assert_eq!(
            found(&libraries, "20-minutes-till-dawn").as_deref(),
            Some("20MinuteTillDawn")
        );
    }

    #[test]
    fn steams_own_record_of_the_folder_wins() {
        let libraries = [library("app-id", &["Ride", "Strange Dir"])];
        with_manifest(&libraries[0], "3949040", "Strange Dir");

        assert_eq!(
            found(&libraries, "rv-there-yet").as_deref(),
            Some("Strange Dir")
        );
    }

    #[test]
    fn a_game_whose_schema_folder_is_absent_is_still_found_by_name() {
        // The schema says "AGAINST_steam"; the folder on disk is just "AGAINST".
        let libraries = [library("fallback", &["AGAINST"])];

        assert_eq!(found(&libraries, "against").as_deref(), Some("AGAINST"));
    }

    #[test]
    fn the_installed_list_uses_the_same_knowledge() {
        let library_path = library("installed", &["Ride", "OMORI"]);
        fs::write(library_path.join("steamapps/common/Ride/Ride.exe"), b"").unwrap();

        let installed = super::installed_games_among(
            &[library_path],
            vec!["rv-there-yet".into(), "gladio-mori".into(), "omori".into()],
        );
        let names: Vec<&str> = installed
            .iter()
            .map(|game| game.identifier.as_str())
            .collect();

        assert_eq!(
            names,
            vec!["rv-there-yet", "omori"],
            "Gladio Mori is not installed"
        );
        assert!(installed[0].builds.windows && !installed[0].builds.mac);
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

#[cfg(all(test, unix))]
mod bottle_library_listing_tests {
    use super::listed_library_paths;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn libraries_a_bottle_names_as_windows_paths_are_shown_as_the_folders_they_are() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-bottle-libraries-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let prefix = root.join("Bottles/Steam");
        let steam_root = prefix.join("drive_c/Program Files (x86)/Steam");
        fs::create_dir_all(steam_root.join("steamapps")).unwrap();
        // The bottle's D: is a folder elsewhere on the Mac.
        let drive_d = root.join("DriveD");
        fs::create_dir_all(drive_d.join("SteamLibrary")).unwrap();
        fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        std::os::unix::fs::symlink("../drive_c", prefix.join("dosdevices/c:")).unwrap();
        std::os::unix::fs::symlink(&drive_d, prefix.join("dosdevices/d:")).unwrap();

        // Steam writes every backslash twice.
        fs::write(
            steam_root.join("steamapps/libraryfolders.vdf"),
            r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
	"2"
	{
		"path"		"E:\\Elsewhere\\Games"
	}
}
"#,
        )
        .unwrap();

        let listed = listed_library_paths(&steam_root);

        assert_eq!(
            listed.len(),
            3,
            "the bottle's own library is listed twice: {listed:?}"
        );
        assert_eq!(listed[0], steam_root);
        assert_eq!(
            fs::canonicalize(&listed[1]).unwrap(),
            fs::canonicalize(drive_d.join("SteamLibrary")).unwrap()
        );
        // E: leads nowhere from here: written the way a person would.
        assert_eq!(listed[2], PathBuf::from("E:\\Elsewhere\\Games"));
    }
}

#[cfg(test)]
mod saved_folder_tests {
    use super::saved_game_folders;
    use std::collections::BTreeMap;
    use std::fs;

    fn folder(label: &str, name: &str) -> String {
        let dir = std::env::temp_dir()
            .join(format!(
                "r2modmac-saved-{}-{}-{}",
                label,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("game.bin"), b"stub").unwrap();
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn a_folder_saved_by_mistake_does_not_make_the_game_installed() {
        // The old matching saved Omori's folder for Gladio Mori, and Control's
        // for Superhot: Mind Control Delete. Both folders exist.
        let mut paths = BTreeMap::new();
        paths.insert("gladio-mori::windows".to_string(), folder("a", "OMORI"));
        paths.insert(
            "superhot-mind-control-delete".to_string(),
            folder("b", "Control"),
        );
        let ids = vec![
            "gladio-mori".to_string(),
            "superhot-mind-control-delete".to_string(),
        ];

        let saved = saved_game_folders(&paths, &ids);

        assert!(saved.games.is_empty(), "{:?}", saved.games);
        assert_eq!(
            saved.mistaken_keys,
            vec!["gladio-mori::windows", "superhot-mind-control-delete"]
        );
    }

    #[test]
    fn a_folder_the_user_chose_still_counts() {
        let mut paths = BTreeMap::new();
        let gladio = folder("c", "Gladio Mori");
        fs::write(
            std::path::Path::new(&gladio).join("Gladio Mori.exe"),
            b"stub",
        )
        .unwrap();
        paths.insert("gladio-mori::windows".to_string(), gladio);
        // An unrelated name: a game kept outside Steam, in a folder of its own.
        let shop = folder("d", "bbb");
        fs::write(
            std::path::Path::new(&shop).join("Card Shop Simulator.exe"),
            b"stub",
        )
        .unwrap();
        paths.insert("tcg-card-shop-simulator".to_string(), shop);
        // A folder that is gone.
        paths.insert(
            "valheim::mac".to_string(),
            "/does/not/exist/Valheim".to_string(),
        );
        let ids = vec![
            "gladio-mori".to_string(),
            "tcg-card-shop-simulator".to_string(),
            "valheim".to_string(),
        ];

        let saved = saved_game_folders(&paths, &ids);
        let names: Vec<&str> = saved.games.iter().map(|(id, _)| id.as_str()).collect();

        assert_eq!(names, vec!["gladio-mori", "tcg-card-shop-simulator"]);
        assert!(saved.mistaken_keys.is_empty());
        assert!(saved.games[0].1.windows && !saved.games[0].1.mac);
    }

    #[test]
    fn a_saved_folder_with_only_leftovers_does_not_count() {
        // Steam keeps the folder of an uninstalled game when it holds saves.
        let dir = folder("e", "Crab Game");
        fs::remove_file(std::path::Path::new(&dir).join("game.bin")).unwrap();
        fs::create_dir_all(std::path::Path::new(&dir).join("SavesDir")).unwrap();
        let mut paths = BTreeMap::new();
        paths.insert("crab-game".to_string(), dir);

        let saved = saved_game_folders(&paths, &["crab-game".to_string()]);

        assert!(saved.games.is_empty(), "{:?}", saved.games);
    }
}

#[cfg(test)]
mod leftover_folder_tests {
    use super::{holds_a_game, installed_games_among};
    use std::fs;
    use std::path::PathBuf;

    fn folder_with(label: &str, entries: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-leftover-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        for entry in entries {
            if entry.ends_with('/') {
                fs::create_dir_all(dir.join(entry)).unwrap();
            } else {
                fs::write(dir.join(entry), b"x").unwrap();
            }
        }
        dir
    }

    #[test]
    fn what_an_uninstall_leaves_behind_is_not_a_game() {
        for entries in [
            vec![],
            vec![".DS_Store"],
            vec![".DS_Store", "SavesDir/"],
            vec!["saves/", "saveimages/"],
            vec!["logs/", "logConfig.xml"],
            vec!["BepInEx/", "PATRONS.md"],
            vec!["r2modmac_exec.log", ".r2modmac_codesign_state"],
            vec!["1023156944/"],
        ] {
            let dir = folder_with("gone", &entries);
            assert!(!holds_a_game(&dir), "{entries:?}");
        }
    }

    #[test]
    fn a_folder_with_game_files_is_a_game() {
        for entries in [
            vec!["Game.exe"],
            vec!["Game.app/"],
            vec!["bapbap_Data/", ".DS_Store"],
            vec!["Binaries/", "WillowGame/"],
            vec!["Game.x86_64", "SavesDir/"],
        ] {
            let dir = folder_with("here", &entries);
            assert!(holds_a_game(&dir), "{entries:?}");
        }
    }

    #[test]
    fn a_leftover_folder_does_not_make_the_game_installed() {
        let library = folder_with("lib", &[]);
        let common = library.join("steamapps/common");
        fs::create_dir_all(common.join("Crab Game/SavesDir")).unwrap();

        let found = installed_games_among(&[library], vec!["crab-game".to_string()]);

        assert!(found.is_empty(), "{:?}", found.len());
    }
}

#[cfg(test)]
mod schema_executable_tests {
    use super::{holds_schema_executable, installed_games_among};
    use std::fs;
    use std::path::PathBuf;

    fn dir_with(label: &str, entries: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-exe-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        for entry in entries {
            let path = dir.join(entry.trim_end_matches('/'));
            if entry.ends_with('/') {
                fs::create_dir_all(path).unwrap();
            } else {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(path, b"x").unwrap();
            }
        }
        dir
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn the_executable_the_schema_names_is_found() {
        // RV There Yet? lives in "Ride" and runs "Ride.exe".
        assert!(holds_schema_executable(
            &dir_with("ride", &["Ride.exe", "Ride/"]),
            &names(&["Ride.exe"])
        ));
        // Case doesn't matter: the schema says "Rounds.exe".
        assert!(holds_schema_executable(
            &dir_with("rounds", &["ROUNDS.exe"]),
            &names(&["Rounds.exe"])
        ));
    }

    #[test]
    fn the_macos_and_linux_builds_count_too() {
        // The schema only lists Muck.exe and Muck.x86_64.
        assert!(holds_schema_executable(
            &dir_with("muck-mac", &["Muck.app/"]),
            &names(&["Muck.exe", "Muck.x86_64"])
        ));
        assert!(holds_schema_executable(
            &dir_with("muck-linux", &["Muck.x86_64"]),
            &names(&["Muck.exe"])
        ));
        // A macOS bundle named differently is still a macOS build.
        assert!(holds_schema_executable(
            &dir_with("other-app", &["Something Else.app/"]),
            &names(&["Game.exe"])
        ));
    }

    #[test]
    fn an_executable_one_level_down_is_found() {
        // Hades II keeps Hades2.exe in "Ship".
        assert!(holds_schema_executable(
            &dir_with("hades", &["Ship/Hades2.exe"]),
            &names(&["Hades2.exe"])
        ));
    }

    #[test]
    fn what_a_mod_loader_leaves_behind_is_not_the_game() {
        // Balatro, uninstalled, with only Lovely's files left.
        assert!(!holds_schema_executable(
            &dir_with("balatro", &["liblovely.dylib", "run_lovely_macos.sh"]),
            &names(&["Balatro.exe"])
        ));
        // Cult of the Lamb with only BepInEx's launcher left.
        assert!(!holds_schema_executable(
            &dir_with(
                "cotl",
                &[
                    "doorstop_config.ini",
                    "run_bepinex.sh",
                    "icon.png",
                    "BepInEx/core/BepInEx.dll"
                ]
            ),
            &names(&["Cult Of The Lamb.exe"])
        ));
        // A folder named like the game is not its binary.
        assert!(!holds_schema_executable(
            &dir_with("same-name-dir", &["Ride/"]),
            &names(&["Ride.exe"])
        ));
    }

    #[test]
    fn a_leftover_folder_of_a_known_game_is_not_installed() {
        let library = dir_with("library", &[]);
        let common = library.join("steamapps/common");
        fs::create_dir_all(common.join("Balatro")).unwrap();
        fs::write(common.join("Balatro/liblovely.dylib"), b"x").unwrap();
        fs::write(common.join("Balatro/run_lovely_macos.sh"), b"x").unwrap();
        fs::create_dir_all(common.join("Muck/Muck.app")).unwrap();
        fs::write(common.join("Muck/run_bepinex.sh"), b"x").unwrap();

        let found =
            installed_games_among(&[library], vec!["balatro".to_string(), "muck".to_string()]);
        let ids: Vec<&str> = found.iter().map(|game| game.identifier.as_str()).collect();

        assert_eq!(ids, vec!["muck"]);
        assert!(found[0].builds.mac && !found[0].builds.windows);
    }
}
