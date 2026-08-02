use super::*;
use tauri::command;

const APPLY_BACKUP_DIR: &str = "apply-transaction";
const APPLY_MARKER: &str = "ready";

fn transaction_dir(app: &AppHandle, profile_id: &str) -> Result<std::path::PathBuf, String> {
    Ok(crate::utils::paths::app_data_dir(app)
        .map_err(|error| error.to_string())?
        .join("profiles")
        .join(profile_id)
        .join(".r2modmac")
        .join(APPLY_BACKUP_DIR))
}

async fn transaction_targets(
    app: &AppHandle,
    profile_id: &str,
    game_identifier: &str,
) -> Result<Vec<std::path::PathBuf>, String> {
    let platform = get_profile_platform(app, profile_id);
    let game_path = get_game_path(
        app.clone(),
        game_identifier.to_string(),
        Some(platform.clone()),
    )
    .await?
    .ok_or_else(|| "GAME_PATH_NOT_CONFIGURED".to_string())?;
    let game_root = std::path::PathBuf::from(game_path);

    if is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(&game_root) {
        return Ok(vec![
            game_root.join("OWML"),
            game_root.join("OWML_DISABLED"),
        ]);
    }

    if is_balatro_identifier(game_identifier) || is_balatro_game_path(&game_root) {
        let mut targets = vec![
            game_root.join(BALATRO_LOVELY_SCRIPT),
            game_root.join("liblovely.dylib"),
        ];
        if let Some(mods_dir) = get_balatro_mods_dir() {
            targets.push(mods_dir);
        }
        return Ok(targets);
    }

    let runtime_root = if platform == "mac" {
        resolve_macos_runtime_root(&game_root)
    } else {
        game_root
    };
    let targets = [
        "BepInEx",
        "BepInEx_DISABLED",
        "doorstop_libs",
        "doorstop_libs_DISABLED",
        "winhttp.dll",
        "winhttp.dll_DISABLED",
        "libdoorstop.dylib",
        "libdoorstop.dylib_DISABLED",
        "doorstop_config.ini",
        "run_bepinex.sh",
    ]
    .into_iter()
    .map(|name| runtime_root.join(name))
    .collect::<Vec<_>>();
    Ok(targets)
}

fn backup_name(index: usize) -> String {
    format!("target-{index}")
}

fn copy_target(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    if source.is_dir() {
        copy_dir_recursive(source, destination).map_err(|error| error.to_string())
    } else if source.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| error.to_string())
    } else {
        Ok(())
    }
}

fn remove_target(target: &std::path::Path) -> Result<(), String> {
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|error| error.to_string())
    } else if target.exists() || target.is_symlink() {
        fs::remove_file(target).map_err(|error| error.to_string())
    } else {
        Ok(())
    }
}

fn restore_snapshot(
    backup_root: &std::path::Path,
    targets: &[std::path::PathBuf],
) -> Result<(), String> {
    for (index, target) in targets.iter().enumerate() {
        remove_target(target)?;
        let backup = backup_root.join(backup_name(index));
        if backup.exists() {
            copy_target(&backup, target)?;
        }
    }
    Ok(())
}

#[command]
pub async fn begin_profile_apply_transaction(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
) -> Result<bool, String> {
    let backup_root = transaction_dir(&app, &profile_id)?;
    let targets = transaction_targets(&app, &profile_id, &game_identifier).await?;
    if backup_root.exists() {
        if backup_root.join(APPLY_MARKER).is_file() {
            restore_snapshot(&backup_root, &targets)?;
        }
        fs::remove_dir_all(&backup_root).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&backup_root).map_err(|error| error.to_string())?;

    let snapshot_result = targets.iter().enumerate().try_for_each(|(index, target)| {
        let backup = backup_root.join(backup_name(index));
        copy_target(target, &backup)
    });
    if let Err(error) = snapshot_result {
        let _ = fs::remove_dir_all(&backup_root);
        return Err(format!("Failed to create safe Apply snapshot: {error}"));
    }
    fs::write(backup_root.join(APPLY_MARKER), b"ready").map_err(|error| error.to_string())?;
    Ok(true)
}

#[command]
pub async fn rollback_profile_apply_transaction(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
) -> Result<bool, String> {
    let backup_root = transaction_dir(&app, &profile_id)?;
    if !backup_root.join(APPLY_MARKER).is_file() {
        return Ok(false);
    }
    let targets = transaction_targets(&app, &profile_id, &game_identifier).await?;
    restore_snapshot(&backup_root, &targets)?;
    fs::remove_dir_all(&backup_root).map_err(|error| error.to_string())?;
    Ok(true)
}

#[command]
pub fn commit_profile_apply_transaction(
    app: AppHandle,
    profile_id: String,
) -> Result<bool, String> {
    let backup_root = transaction_dir(&app, &profile_id)?;
    if backup_root.exists() {
        fs::remove_dir_all(backup_root).map_err(|error| error.to_string())?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_restore_replaces_partial_apply_contents() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-apply-transaction-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = root.join("target");
        let backup = root.join("backup");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("mod.dll"), b"old").unwrap();
        copy_target(&target, &backup.join(backup_name(0))).unwrap();

        fs::write(target.join("mod.dll"), b"partial-new").unwrap();
        fs::write(target.join("new-only.dll"), b"partial").unwrap();
        restore_snapshot(&backup, &[target.clone()]).unwrap();

        assert_eq!(fs::read(target.join("mod.dll")).unwrap(), b"old");
        assert!(!target.join("new-only.dll").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
