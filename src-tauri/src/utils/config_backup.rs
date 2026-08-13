use crate::models::shared::{
    get_owml_dir, is_balatro_game_path, is_balatro_identifier, is_outerwilds_game_path,
    is_outerwilds_identifier,
};
use crate::tracing::{perfetto_te_ns, scoped_track_event, EventContext, TrackEventDebugArg};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const BACKUP_DIR_NAME: &str = "configs";
const OWNERS_FILE_NAME: &str = "config_owners.json";
const OWML_CONFIG_FILES: &[&str] = &["config.json"];

#[derive(Debug, Clone)]
pub struct ConfigRoot {
    pub key: String,
    pub live_dir: PathBuf,
    pub file_names: Option<&'static [&'static str]>,
}

pub fn config_roots(
    game_path: &Path,
    runtime_game_path: &Path,
    game_identifier: &str,
) -> Vec<ConfigRoot> {
    if is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(game_path) {
        let owml_dir = get_owml_dir(game_path).unwrap_or_else(|| game_path.join("OWML"));
        return vec![ConfigRoot {
            key: "owml".to_string(),
            live_dir: owml_dir.join("Mods"),
            file_names: Some(OWML_CONFIG_FILES),
        }];
    }

    if is_balatro_identifier(game_identifier) || is_balatro_game_path(game_path) {
        return Vec::new();
    }

    let bepinex_dir = if runtime_game_path.join("BepInEx").is_dir()
        || !runtime_game_path.join("BepInEx_DISABLED").is_dir()
    {
        runtime_game_path.join("BepInEx")
    } else {
        runtime_game_path.join("BepInEx_DISABLED")
    };

    vec![ConfigRoot {
        key: "bepinex".to_string(),
        live_dir: bepinex_dir.join("config"),
        file_names: None,
    }]
}

pub fn profile_backup_dir(app_data_dir: &Path, profile_id: &str) -> PathBuf {
    app_data_dir
        .join("profiles")
        .join(profile_id)
        .join(".r2modmac")
        .join(BACKUP_DIR_NAME)
}

pub fn apply_profile_configs(
    app_data_dir: &Path,
    profile_id: &str,
    game_path: &Path,
    runtime_game_path: &Path,
    game_identifier: &str,
) {
    scoped_track_event!(
        "config",
        "apply_profile_configs",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id));
            ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier));
        }
    );
    let roots = config_roots(game_path, runtime_game_path, game_identifier);
    if roots.is_empty() {
        return;
    }

    let owners_path = app_data_dir.join(OWNERS_FILE_NAME);
    let mut owners = read_owners(&owners_path);
    let mut changed = false;

    for root in &roots {
        let slot = owner_slot(root);
        let previous = owners.get(&slot).cloned();
        if previous.as_deref() == Some(profile_id) {
            continue;
        }

        if let Some(previous_id) = previous.as_deref() {
            if app_data_dir.join("profiles").join(previous_id).is_dir() {
                let previous_backup = profile_backup_dir(app_data_dir, previous_id).join(&root.key);
                let captured = capture_root(root, &previous_backup);
                log::info!(
                    "[config_backup] Backed up {} config file(s) for profile {}",
                    captured,
                    previous_id
                );
            }
        }

        let backup = profile_backup_dir(app_data_dir, profile_id).join(&root.key);
        let restored = restore_root(root, &backup, previous.is_some());
        if restored > 0 {
            log::info!(
                "[config_backup] Restored {} config file(s) for profile {}",
                restored,
                profile_id
            );
        }

        owners.insert(slot, profile_id.to_string());
        changed = true;
    }

    if changed {
        write_owners(&owners_path, &owners);
    }
}

pub fn capture_profile_configs(
    app_data_dir: &Path,
    profile_id: &str,
    game_path: &Path,
    runtime_game_path: &Path,
    game_identifier: &str,
) -> usize {
    scoped_track_event!(
        "config",
        "capture_profile_configs",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id));
            ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier));
        }
    );
    let roots = config_roots(game_path, runtime_game_path, game_identifier);
    if roots.is_empty() {
        return 0;
    }

    let mut captured = 0;
    let owners_path = app_data_dir.join(OWNERS_FILE_NAME);
    let mut owners = read_owners(&owners_path);
    let mut changed = false;

    for root in &roots {
        let backup = profile_backup_dir(app_data_dir, profile_id).join(&root.key);
        captured += capture_root(root, &backup);
        let slot = owner_slot(root);
        if owners.get(&slot).map(String::as_str) != Some(profile_id) {
            owners.insert(slot, profile_id.to_string());
            changed = true;
        }
    }

    if changed {
        write_owners(&owners_path, &owners);
    }
    captured
}

pub fn forget_profile_configs(app_data_dir: &Path, profile_id: &str) {
    let owners_path = app_data_dir.join(OWNERS_FILE_NAME);
    let mut owners = read_owners(&owners_path);
    let before = owners.len();
    owners.retain(|_, owner| owner != profile_id);
    if owners.len() != before {
        write_owners(&owners_path, &owners);
    }
}

pub fn backup_relative_files(app_data_dir: &Path, profile_id: &str) -> Vec<(PathBuf, PathBuf)> {
    let root = profile_backup_dir(app_data_dir, profile_id);
    if !root.is_dir() {
        return Vec::new();
    }
    collect_files(&root, None)
        .into_iter()
        .map(|relative| (root.join(&relative), relative))
        .collect()
}

fn owner_slot(root: &ConfigRoot) -> String {
    format!("{}|{}", root.key, root.live_dir.to_string_lossy())
}

fn read_owners(path: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_owners(path: &Path, owners: &BTreeMap<String, String>) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(owners) {
        if let Err(e) = fs::write(path, serialized) {
            log::warn!("[config_backup] Could not write {:?}: {}", path, e);
        }
    }
}

fn capture_root(root: &ConfigRoot, backup_dir: &Path) -> usize {
    if !root.live_dir.is_dir() {
        return 0;
    }
    mirror(&root.live_dir, backup_dir, root.file_names, true)
}

fn restore_root(root: &ConfigRoot, backup_dir: &Path, prune: bool) -> usize {
    if !backup_dir.is_dir() {
        return 0;
    }
    mirror(backup_dir, &root.live_dir, root.file_names, prune)
}

fn mirror(from: &Path, to: &Path, file_names: Option<&[&str]>, prune: bool) -> usize {
    scoped_track_event!("fs", "config_mirror", |ctx: &mut EventContext| {
        ctx.add_debug_arg(
            "from",
            TrackEventDebugArg::String(from.to_string_lossy().as_ref()),
        );
        ctx.add_debug_arg(
            "to",
            TrackEventDebugArg::String(to.to_string_lossy().as_ref()),
        );
        ctx.add_debug_arg("prune", TrackEventDebugArg::Bool(prune));
    });
    let sources = collect_files(from, file_names);
    let mut copied = 0;

    for relative in &sources {
        let destination = to.join(relative);
        if let Some(parent) = destination.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                log::warn!("[config_backup] Could not create {:?}: {}", parent, e);
                continue;
            }
        }
        match fs::copy(from.join(relative), &destination) {
            Ok(_) => copied += 1,
            Err(e) => log::warn!("[config_backup] Could not copy {:?}: {}", relative, e),
        }
    }

    if prune && to.is_dir() {
        let kept: HashSet<PathBuf> = sources.into_iter().collect();
        for relative in collect_files(to, file_names) {
            if !kept.contains(&relative) {
                let _ = fs::remove_file(to.join(&relative));
            }
        }
        remove_empty_directories(to);
    }

    copied
}

fn collect_files(base: &Path, file_names: Option<&[&str]>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(base, Path::new(""), file_names, &mut out);
    out
}

fn walk(base: &Path, prefix: &Path, file_names: Option<&[&str]>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(base.join(prefix)) else {
        return;
    };

    for entry in entries.filter_map(|entry| entry.ok()) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let relative = prefix.join(entry.file_name());
        if file_type.is_dir() {
            walk(base, &relative, file_names, out);
        } else if file_type.is_file() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let wanted = file_names
                .map(|names| names.iter().any(|candidate| *candidate == name))
                .unwrap_or(true);
            if wanted {
                out.push(relative);
            }
        }
    }
}

fn remove_empty_directories(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            remove_empty_directories(&entry.path());
            let _ = fs::remove_dir(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-config-backup-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn switching_profiles_keeps_each_set_of_configs() {
        let root = temp_dir("switch");
        let app_data = root.join("app");
        let game = root.join("game");
        let live = game.join("BepInEx").join("config");
        write(&live.join("Mod.cfg"), "first");
        fs::create_dir_all(app_data.join("profiles").join("first")).unwrap();
        fs::create_dir_all(app_data.join("profiles").join("second")).unwrap();

        apply_profile_configs(&app_data, "first", &game, &game, "lethal-company");
        capture_profile_configs(&app_data, "first", &game, &game, "lethal-company");

        apply_profile_configs(&app_data, "second", &game, &game, "lethal-company");
        write(&live.join("Other.cfg"), "second");
        capture_profile_configs(&app_data, "second", &game, &game, "lethal-company");

        apply_profile_configs(&app_data, "first", &game, &game, "lethal-company");
        assert_eq!(fs::read_to_string(live.join("Mod.cfg")).unwrap(), "first");
        assert!(!live.join("Other.cfg").exists());

        apply_profile_configs(&app_data, "second", &game, &game, "lethal-company");
        assert_eq!(
            fs::read_to_string(live.join("Other.cfg")).unwrap(),
            "second"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_first_apply_never_prunes_untracked_configs() {
        let root = temp_dir("untracked");
        let app_data = root.join("app");
        let game = root.join("game");
        let live = game.join("BepInEx").join("config");
        write(&live.join("Untracked.cfg"), "keep me");
        write(
            &profile_backup_dir(&app_data, "imported")
                .join("bepinex")
                .join("Imported.cfg"),
            "imported",
        );

        apply_profile_configs(&app_data, "imported", &game, &game, "lethal-company");

        assert!(live.join("Untracked.cfg").exists());
        assert_eq!(
            fs::read_to_string(live.join("Imported.cfg")).unwrap(),
            "imported"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_bepinex_root_follows_a_vanilla_toggled_install() {
        let root = temp_dir("roots");
        let game = root.join("game");
        fs::create_dir_all(game.join("BepInEx_DISABLED")).unwrap();

        let roots = config_roots(&game, &game, "lethal-company");
        assert_eq!(roots.len(), 1);
        assert_eq!(
            roots[0].live_dir,
            game.join("BepInEx_DISABLED").join("config")
        );

        fs::create_dir_all(game.join("BepInEx")).unwrap();
        let roots = config_roots(&game, &game, "lethal-company");
        assert_eq!(roots[0].live_dir, game.join("BepInEx").join("config"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_fresh_install_still_reports_the_bepinex_root() {
        let root = temp_dir("fresh");
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        let roots = config_roots(&game, &game, "lethal-company");

        assert_eq!(roots[0].live_dir, game.join("BepInEx").join("config"));
        assert!(roots[0].file_names.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_owml_root_follows_a_vanilla_toggled_install() {
        let root = temp_dir("owml-roots");
        let game = root.join("game");
        fs::create_dir_all(game.join("OWML_DISABLED")).unwrap();

        let roots = config_roots(&game, &game, "outerwilds");

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].live_dir, game.join("OWML_DISABLED").join("Mods"));
        assert_eq!(roots[0].file_names, Some(OWML_CONFIG_FILES));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn balatro_has_no_backed_up_config_root() {
        let root = temp_dir("balatro");
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        assert!(config_roots(&game, &game, "balatro").is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_game_without_a_config_root_is_left_alone() {
        let root = temp_dir("balatro-noop");
        let app_data = root.join("app");
        let game = root.join("game");
        fs::create_dir_all(&game).unwrap();

        apply_profile_configs(&app_data, "any", &game, &game, "balatro");
        let captured = capture_profile_configs(&app_data, "any", &game, &game, "balatro");

        assert_eq!(captured, 0);
        assert!(!app_data.join(OWNERS_FILE_NAME).exists());
        assert!(!profile_backup_dir(&app_data, "any").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleting_a_profile_releases_the_configs_it_owned() {
        let root = temp_dir("forget");
        let app_data = root.join("app");
        let game = root.join("game");
        let live = game.join("BepInEx").join("config");
        write(&live.join("Mod.cfg"), "owned");
        fs::create_dir_all(app_data.join("profiles").join("owner")).unwrap();

        capture_profile_configs(&app_data, "owner", &game, &game, "lethal-company");
        forget_profile_configs(&app_data, "owner");
        fs::remove_dir_all(app_data.join("profiles").join("owner")).unwrap();

        apply_profile_configs(&app_data, "newcomer", &game, &game, "lethal-company");

        assert!(live.join("Mod.cfg").exists());
        assert!(!profile_backup_dir(&app_data, "owner").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_config_deleted_in_game_stops_coming_back() {
        let root = temp_dir("prune");
        let app_data = root.join("app");
        let game = root.join("game");
        let live = game.join("BepInEx").join("config");
        write(&live.join("Kept.cfg"), "kept");
        write(&live.join("Dropped.cfg"), "dropped");
        fs::create_dir_all(app_data.join("profiles").join("solo")).unwrap();

        capture_profile_configs(&app_data, "solo", &game, &game, "lethal-company");
        fs::remove_file(live.join("Dropped.cfg")).unwrap();
        capture_profile_configs(&app_data, "solo", &game, &game, "lethal-company");

        let backup = profile_backup_dir(&app_data, "solo").join("bepinex");
        assert!(backup.join("Kept.cfg").exists());
        assert!(!backup.join("Dropped.cfg").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn outer_wilds_backs_up_only_mod_configs() {
        let root = temp_dir("owml");
        let app_data = root.join("app");
        let game = root.join("game");
        let mods = game.join("OWML").join("Mods").join("Author.Mod");
        write(&mods.join("config.json"), "{}");
        write(&mods.join("Author.Mod.dll"), "binary");

        capture_profile_configs(&app_data, "ow", &game, &game, "outerwilds");

        let backup = profile_backup_dir(&app_data, "ow").join("owml");
        assert!(backup.join("Author.Mod").join("config.json").exists());
        assert!(!backup.join("Author.Mod").join("Author.Mod.dll").exists());

        fs::remove_dir_all(root).unwrap();
    }
}
