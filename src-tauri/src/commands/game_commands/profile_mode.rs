use std::{fs, path::Path};

use tauri::{command, AppHandle};

use super::{
    choose_bepinex_root, detach_isolated_bepinex_link, get_game_path, is_game_running,
    resolve_macos_runtime_root, sync_isolated_bepinex_link,
};
use crate::utils::file_ops::copy_dir_recursive;

fn ensure_no_symlinks(directory: &Path) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err(format!(
                "Refusing to copy through the symlink {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            ensure_no_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

fn migrate_tree(source_root: &Path, destination_root: &Path, name: &str) -> Result<(), String> {
    let source = source_root.join(name);
    let destination = destination_root.join(name);
    if source.is_symlink() {
        return Err(format!(
            "Refusing to copy through the symlink {}",
            source.display()
        ));
    }
    let has_source = source.is_dir();
    if source.exists() && !has_source {
        return Err(format!("{} is not a directory", source.display()));
    }
    if has_source {
        ensure_no_symlinks(&source)?;
    }
    fs::create_dir_all(destination_root).map_err(|error| error.to_string())?;
    let marker = uuid::Uuid::new_v4();
    let staged = destination_root.join(format!(".{name}.r2modmac-{marker}.staging"));
    let backup = destination_root.join(format!("{name}.r2modmac-backup-{marker}"));
    if has_source {
        if let Err(error) = copy_dir_recursive(&source, &staged) {
            let _ = fs::remove_dir_all(&staged);
            return Err(format!("Could not stage {}: {error}", source.display()));
        }
    }
    if destination.is_symlink() {
        // Only r2modmac-owned profile links are eligible for replacement.
        if name != "BepInEx" {
            let _ = fs::remove_dir_all(&staged);
            return Err(format!(
                "Refusing to replace the symlink {}",
                destination.display()
            ));
        }
        if let Err(error) = detach_isolated_bepinex_link(destination_root) {
            let _ = fs::remove_dir_all(&staged);
            return Err(error);
        }
    } else if destination.exists() {
        if let Err(error) = fs::rename(&destination, &backup) {
            let _ = fs::remove_dir_all(&staged);
            return Err(error.to_string());
        }
        log::warn!(
            "[profile_mode] Preserved previous runtime at {}",
            backup.display()
        );
    }
    if has_source {
        if let Err(error) = fs::rename(&staged, &destination) {
            if backup.exists() {
                let _ = fs::rename(&backup, &destination);
            }
            return Err(format!(
                "Could not activate {}: {error}",
                destination.display()
            ));
        }
    }
    Ok(())
}

/// Switch one profile without deleting its old BepInEx installation. Existing
/// profiles with no override retain the setting under which they were created.
#[command]
pub async fn set_profile_bepinex_isolation(
    app: AppHandle,
    profile_id: String,
    isolated: bool,
) -> Result<bool, String> {
    let mut profiles = crate::commands::profile_commands::get_profiles(app.clone())?;
    let profile = profiles
        .iter_mut()
        .find(|profile| {
            profile.get("id").and_then(serde_json::Value::as_str) == Some(profile_id.as_str())
        })
        .ok_or_else(|| "Profile not found".to_string())?;
    let game_identifier = profile
        .get("gameIdentifier")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Profile has no game".to_string())?
        .to_string();
    let platform = profile
        .get("platform")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("windows")
        .to_string();
    let settings = crate::models::shared::load_settings_impl(&app);
    let current = profile
        .get("bepinexIsolation")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(settings.profile_isolation);
    if current == isolated {
        return Ok(true);
    }
    if is_game_running(app.clone(), game_identifier.clone(), Some(platform.clone())).await? {
        return Err("Close the game before changing BepInEx storage mode".to_string());
    }
    let game_path = get_game_path(app.clone(), game_identifier, Some(platform.clone()))
        .await?
        .ok_or_else(|| "Set the game directory before changing BepInEx storage mode".to_string())?;
    let runtime_root = if platform == "mac" {
        resolve_macos_runtime_root(Path::new(&game_path))
    } else {
        Path::new(&game_path).to_path_buf()
    };
    let profile_root = crate::utils::paths::app_data_dir(&app)
        .map_err(|error| error.to_string())?
        .join("profiles")
        .join(&profile_id);
    if isolated && choose_bepinex_root(true, &profile_root, &runtime_root) == runtime_root {
        return Err(
            "This Wine bottle cannot access an isolated BepInEx tree; use game-local mode"
                .to_string(),
        );
    }
    let (source, destination) = if isolated {
        (&runtime_root, &profile_root)
    } else {
        (&profile_root, &runtime_root)
    };
    for name in ["BepInEx", "BepInEx_DISABLED"] {
        // A game-side link may belong to another profile. It is never a
        // source for a migration into this profile.
        if isolated && source.join(name).is_symlink() {
            continue;
        }
        if isolated && !source.join(name).is_dir() {
            continue;
        }
        migrate_tree(source, destination, name)?;
        if isolated && source.join(name).is_dir() {
            let preserved =
                runtime_root.join(format!("{name}.r2modmac-backup-{}", uuid::Uuid::new_v4()));
            fs::rename(source.join(name), &preserved).map_err(|error| error.to_string())?;
            log::warn!(
                "[profile_mode] Preserved game-local runtime at {}",
                preserved.display()
            );
        }
    }
    if isolated {
        crate::commands::mod_commands::point_game_doorstop_ini_at_tree(
            &runtime_root,
            &profile_root,
        )?;
        if platform == "mac" && profile_root.join("BepInEx").is_dir() {
            sync_isolated_bepinex_link(&runtime_root, &profile_root, false)?;
        }
    } else {
        crate::commands::mod_commands::point_game_doorstop_ini_at_tree(
            &runtime_root,
            &runtime_root,
        )?;
    }
    profile["bepinexIsolation"] = serde_json::Value::Bool(isolated);
    profile["needs_sync"] = serde_json::Value::Bool(true);
    crate::commands::profile_commands::save_profiles(app, profiles).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_copies_without_deleting_source_and_backs_up_destination() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-profile-mode-{}", uuid::Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("BepInEx/plugins")).unwrap();
        fs::create_dir_all(destination.join("BepInEx/plugins")).unwrap();
        fs::write(source.join("BepInEx/plugins/new.dll"), b"new").unwrap();
        fs::write(destination.join("BepInEx/plugins/old.dll"), b"old").unwrap();

        migrate_tree(&source, &destination, "BepInEx").unwrap();

        assert_eq!(
            fs::read(source.join("BepInEx/plugins/new.dll")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(destination.join("BepInEx/plugins/new.dll")).unwrap(),
            b"new"
        );
        assert!(!destination.join("BepInEx/plugins/old.dll").exists());
        let backup = fs::read_dir(&destination)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("BepInEx.r2modmac-backup-")
            })
            .unwrap();
        assert_eq!(fs::read(backup.join("plugins/old.dll")).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn migration_refuses_nested_symlinks() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-profile-mode-{}", uuid::Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("BepInEx/plugins")).unwrap();
        std::os::unix::fs::symlink("/tmp", source.join("BepInEx/plugins/external")).unwrap();
        assert!(migrate_tree(&source, &destination, "BepInEx").is_err());
        assert!(!destination.join("BepInEx").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
