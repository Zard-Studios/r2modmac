use super::*;
use crate::tracing::{perfetto_te_ns, scoped_track_event, EventContext, TrackEventDebugArg};
use tauri::command;

const APPLY_BACKUP_DIR: &str = "apply-transaction";
const APPLY_MARKER: &str = "ready";
const APPLY_TARGETS: &str = "targets.json";

fn saved_transaction_targets(
    backup_root: &std::path::Path,
    current_targets: &[std::path::PathBuf],
    app_data_dir: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, String> {
    let plan_path = backup_root.join(APPLY_TARGETS);
    if !plan_path.is_file() {
        // Older snapshots did not record their target list.
        return Ok(current_targets.to_vec());
    }
    let targets: Vec<std::path::PathBuf> =
        serde_json::from_slice(&fs::read(&plan_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Invalid Apply snapshot plan: {error}"))?;
    if targets.first() != current_targets.first() {
        return Err(
            "Apply snapshot no longer matches this game path; refusing rollback".to_string(),
        );
    }
    let game_root = current_targets
        .first()
        .and_then(|target| target.parent())
        .ok_or_else(|| "Apply snapshot has no game root".to_string())?;
    if targets.iter().any(|target| {
        !target.is_absolute()
            || target
                .components()
                .any(|component| component == std::path::Component::ParentDir)
            || !(target.starts_with(game_root)
                || target.starts_with(app_data_dir)
                || current_targets.contains(target))
            || target == game_root
            || target == app_data_dir
    }) {
        return Err("Apply snapshot contains an unsafe target path; refusing rollback".to_string());
    }
    Ok(targets)
}

fn legacy_transaction_dir(app: &AppHandle, profile_id: &str) -> Result<std::path::PathBuf, String> {
    Ok(crate::utils::paths::app_data_dir(app)
        .map_err(|error| error.to_string())?
        .join("profiles")
        .join(profile_id)
        .join(".r2modmac")
        .join(APPLY_BACKUP_DIR))
}

fn transaction_dir(
    targets: &[std::path::PathBuf],
    profile_id: &str,
) -> Result<std::path::PathBuf, String> {
    let runtime_root = targets
        .first()
        .and_then(|target| target.parent())
        .ok_or_else(|| "Cannot resolve Apply snapshot location".to_string())?;
    Ok(runtime_root
        .join(".r2modmac")
        .join("apply-transactions")
        .join(profile_id))
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
    let app_data_dir = crate::utils::paths::app_data_dir(app).map_err(|error| error.to_string())?;
    let with_config_targets = |mut targets: Vec<std::path::PathBuf>,
                               tree_root: &std::path::Path| {
        // An isolated BepInEx tree lives under the profile. Snapshotting the
        // game-side link alone does not protect its live config directory.
        for root in
            crate::utils::config_backup::config_roots(&game_root, tree_root, game_identifier)
        {
            if !targets
                .iter()
                .any(|target| root.live_dir.starts_with(target))
            {
                targets.push(root.live_dir);
            }
        }
        targets.extend(crate::utils::config_backup::config_transaction_targets(
            &app_data_dir,
            profile_id,
            &game_root,
            tree_root,
            game_identifier,
        ));
        targets
    };

    if is_outerwilds_identifier(game_identifier) || is_outerwilds_game_path(&game_root) {
        return Ok(with_config_targets(
            vec![game_root.join("OWML"), game_root.join("OWML_DISABLED")],
            &game_root,
        ));
    }

    if is_balatro_identifier(game_identifier) || is_balatro_game_path(&game_root) {
        let mut targets = vec![
            game_root.join(BALATRO_LOVELY_SCRIPT),
            game_root.join("liblovely.dylib"),
        ];
        if let Some(mods_dir) = get_balatro_mods_dir() {
            targets.push(mods_dir);
        }
        return Ok(with_config_targets(targets, &game_root));
    }

    // A shimloader apply writes nothing into the game but the runtime, so that
    // is the whole of what a rollback has to put back.
    if crate::models::loaders::uses_shimloader(game_identifier) {
        let data_folder = crate::models::loaders::shimloader_game(game_identifier)
            .map(|game| game.data_folder)
            .unwrap_or_default();
        let Some(binaries_dir) =
            crate::models::loaders::shimloader_binaries_dir(&game_root, &data_folder)
        else {
            return Ok(with_config_targets(Vec::new(), &game_root));
        };
        return Ok(with_config_targets(
            crate::models::loaders::SHIMLOADER_RUNTIME_FILES
                .iter()
                .map(|name| binaries_dir.join(name))
                .collect(),
            &game_root,
        ));
    }

    if crate::models::loaders::uses_return_of_modding(game_identifier, &game_root) {
        // The pack's proxy DLL is named per pack (version.dll for
        // ReturnOfModding, d3d12.dll for Hell2Modding), so every name a pack
        // can install has to be part of the transaction snapshot.
        let mut targets = vec![
            game_root.join("ReturnOfModding"),
            game_root.join("mods.yml"),
        ];
        for name in crate::models::loaders::RETURN_OF_MODDING_PROXY_NAMES {
            targets.push(game_root.join(name));
            targets.push(game_root.join(format!("{name}_DISABLED")));
        }
        return Ok(with_config_targets(targets, &game_root));
    }

    let runtime_root = if platform == "mac" {
        resolve_macos_runtime_root(&game_root)
    } else {
        game_root.clone()
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
    let tree_root = bepinex_install_root(app, profile_id, &runtime_root)?;
    let mut targets = with_config_targets(targets, &tree_root);
    if tree_root == runtime_root {
        let marker = super::profile_activation::marker_path(&runtime_root);
        let active =
            super::profile_activation::read_active_profile(&runtime_root, game_identifier)?
                .or_else(|| {
                    crate::utils::config_backup::active_config_owner(
                        &app_data_dir,
                        &game_root,
                        &tree_root,
                        game_identifier,
                    )
                });
        targets.push(marker);
        targets.push(
            app_data_dir
                .join("profiles")
                .join(profile_id)
                .join(".r2modmac/manifests/game"),
        );
        if let Some(outgoing) = active.filter(|active| active != profile_id) {
            targets.push(
                app_data_dir
                    .join("profiles")
                    .join(outgoing)
                    .join(".r2modmac/manifests/game"),
            );
        }
    }
    Ok(targets)
}

fn backup_name(index: usize) -> String {
    format!("target-{index}")
}

fn copy_target(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    if source.is_symlink() {
        copy_symlink(source, destination)
    } else if source.is_dir() {
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

fn copy_symlink(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let target = fs::read_link(source).map_err(|error| error.to_string())?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::os::unix::fs::symlink(target, destination).map_err(|error| error.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = (source, destination);
        Err("Cannot snapshot symbolic links on this platform".to_string())
    }
}

fn target_size(target: &std::path::Path) -> u64 {
    if target.is_symlink() {
        0
    } else if target.is_dir() {
        calculate_dir_size(target).unwrap_or(0)
    } else {
        fs::metadata(target)
            .map(|metadata| metadata.len())
            .unwrap_or(0)
    }
}

fn copy_target_with_progress(
    source: &std::path::Path,
    destination: &std::path::Path,
    on_copied: &mut impl FnMut(u64),
) -> Result<(), String> {
    scoped_track_event!(
        "fs",
        "copy_target_with_progress",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg(
                "src",
                TrackEventDebugArg::String(source.to_string_lossy().as_ref()),
            );
            ctx.add_debug_arg(
                "dst",
                TrackEventDebugArg::String(destination.to_string_lossy().as_ref()),
            );
        }
    );
    if source.is_symlink() {
        copy_symlink(source, destination)?;
    } else if source.is_dir() {
        fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            if source_path.is_symlink() {
                copy_symlink(&source_path, &destination_path)?;
            } else if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                copy_target_with_progress(&source_path, &destination_path, on_copied)?;
            } else {
                if let Some(parent) = destination_path.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                let copied =
                    fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
                on_copied(copied);
            }
        }
    } else if source.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let copied = fs::copy(source, destination).map_err(|error| error.to_string())?;
        on_copied(copied);
    }
    Ok(())
}

fn remove_target(target: &std::path::Path) -> Result<(), String> {
    if target.is_symlink() {
        fs::remove_file(target).map_err(|error| error.to_string())
    } else if target.is_dir() {
        fs::remove_dir_all(target).map_err(|error| error.to_string())
    } else if target.exists() || target.is_symlink() {
        fs::remove_file(target).map_err(|error| error.to_string())
    } else {
        Ok(())
    }
}

/// Config files under `target` that the snapshot never saw, as (path, bytes).
///
/// A rollback undoes an Apply, and an Apply does not write these: the game does,
/// while it runs. Restoring the snapshot over them deletes settings the user
/// spent time on and that no backup holds (issue #39).
fn configs_written_since_the_snapshot(
    target: &std::path::Path,
    backup: &std::path::Path,
) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    if target.is_symlink() || backup.is_symlink() {
        return Vec::new();
    }
    let live_config = target.join("config");
    if !live_config.is_dir() {
        return Vec::new();
    }

    let mut preserved = Vec::new();
    let mut pending = vec![live_config.clone()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(target) else {
                continue;
            };
            if backup.join(relative).exists() {
                continue;
            }
            if let Ok(bytes) = fs::read(&path) {
                preserved.push((relative.to_path_buf(), bytes));
            }
        }
    }
    preserved
}

fn restore_snapshot(
    backup_root: &std::path::Path,
    targets: &[std::path::PathBuf],
) -> Result<(), String> {
    scoped_track_event!("install", "restore_snapshot", |ctx: &mut EventContext| {
        ctx.add_debug_arg(
            "backup",
            TrackEventDebugArg::String(backup_root.to_string_lossy().as_ref()),
        );
    });
    for (index, target) in targets.iter().enumerate() {
        let backup = backup_root.join(backup_name(index));
        let preserved = configs_written_since_the_snapshot(target, &backup);
        if !preserved.is_empty() {
            log::info!(
                "[apply_transaction] Keeping {} config file(s) written since the snapshot in {:?}",
                preserved.len(),
                target
            );
        }

        remove_target(target)?;
        if backup.exists() || backup.is_symlink() {
            copy_target(&backup, target)?;
        }

        for (relative, bytes) in preserved {
            let destination = target.join(&relative);
            if destination.exists() {
                continue;
            }
            if let Some(parent) = destination.parent() {
                if fs::create_dir_all(parent).is_err() {
                    continue;
                }
            }
            if let Err(error) = fs::write(&destination, bytes) {
                log::warn!(
                    "[apply_transaction] Could not keep {}: {}",
                    destination.display(),
                    error
                );
            }
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
    scoped_track_event!(
        "install",
        "begin_profile_apply_transaction",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id.as_str()));
            ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier.as_str()));
        }
    );
    let targets = transaction_targets(&app, &profile_id, &game_identifier).await?;
    let backup_root = transaction_dir(&targets, &profile_id)?;
    let legacy_backup_root = legacy_transaction_dir(&app, &profile_id)?;
    if legacy_backup_root.exists() {
        if legacy_backup_root.join(APPLY_MARKER).is_file() {
            let saved = saved_transaction_targets(
                &legacy_backup_root,
                &targets,
                &crate::utils::paths::app_data_dir(&app).map_err(|error| error.to_string())?,
            )?;
            restore_snapshot(&legacy_backup_root, &saved)?;
        }
        fs::remove_dir_all(&legacy_backup_root).map_err(|error| error.to_string())?;
    }
    if backup_root.exists() {
        if backup_root.join(APPLY_MARKER).is_file() {
            let saved = saved_transaction_targets(
                &backup_root,
                &targets,
                &crate::utils::paths::app_data_dir(&app).map_err(|error| error.to_string())?,
            )?;
            restore_snapshot(&backup_root, &saved)?;
        }
        fs::remove_dir_all(&backup_root).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&backup_root).map_err(|error| error.to_string())?;
    crate::utils::stable_json::write_file(&backup_root.join(APPLY_TARGETS), &targets)?;

    // Fast path: clone every target copy-on-write. When this works the snapshot
    // is metadata-only, so there is no point walking the tree to size it first —
    // that walk alone used to cost more than the whole operation now does.
    let mut pending: Vec<(usize, &std::path::PathBuf)> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        if !target.exists() && !target.is_symlink() {
            continue;
        }
        if target.is_symlink() {
            pending.push((index, target));
            continue;
        }
        if try_clone_tree(target, &backup_root.join(backup_name(index))) {
            continue;
        }
        pending.push((index, target));
    }

    if pending.is_empty() {
        let _ = app.emit(
            "profile-apply-snapshot-progress",
            serde_json::json!({
                "copiedBytes": 0,
                "totalBytes": 0,
                "progressPercent": 100,
            }),
        );
        fs::write(backup_root.join(APPLY_MARKER), b"ready").map_err(|error| error.to_string())?;
        return Ok(true);
    }

    // Slow path: only the targets that could not be cloned are sized and copied.
    let total_bytes = pending
        .iter()
        .map(|(_, target)| target_size(target))
        .sum::<u64>();
    let mut copied_bytes = 0_u64;
    let mut last_percent = u64::MAX;
    let snapshot_result = pending.iter().try_for_each(|(index, target)| {
        let backup = backup_root.join(backup_name(*index));
        copy_target_with_progress(target, &backup, &mut |copied| {
            copied_bytes = copied_bytes.saturating_add(copied);
            let percent = if total_bytes == 0 {
                100
            } else {
                copied_bytes.saturating_mul(100) / total_bytes
            };
            if percent != last_percent {
                last_percent = percent;
                let _ = app.emit(
                    "profile-apply-snapshot-progress",
                    serde_json::json!({
                        "copiedBytes": copied_bytes,
                        "totalBytes": total_bytes,
                        "progressPercent": percent,
                    }),
                );
            }
        })
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
    scoped_track_event!(
        "install",
        "rollback_profile_apply_transaction",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id.as_str()));
            ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier.as_str()));
        }
    );
    let targets = transaction_targets(&app, &profile_id, &game_identifier).await?;
    let backup_roots = [
        transaction_dir(&targets, &profile_id)?,
        legacy_transaction_dir(&app, &profile_id)?,
    ];
    for backup_root in backup_roots {
        if backup_root.join(APPLY_MARKER).is_file() {
            // A rollback discards everything installed since the snapshot, so a
            // failed post-install check can silently undo a mod or runtime that
            // did install correctly. Log it at info level: this is the step that
            // makes an install "disappear".
            log::info!(
                "[apply_transaction] Rolling back profile {} ({}): restoring snapshot {:?}",
                profile_id,
                game_identifier,
                backup_root
            );
            let saved = saved_transaction_targets(
                &backup_root,
                &targets,
                &crate::utils::paths::app_data_dir(&app).map_err(|error| error.to_string())?,
            )?;
            restore_snapshot(&backup_root, &saved)?;
            fs::remove_dir_all(&backup_root).map_err(|error| error.to_string())?;
            log::info!(
                "[apply_transaction] Rollback complete for profile {}; game files are back at their pre-Apply state",
                profile_id
            );
            return Ok(true);
        }
    }
    log::debug!(
        "[apply_transaction] No rollback snapshot found for profile {} ({}); nothing to restore",
        profile_id,
        game_identifier
    );
    Ok(false)
}

#[command]
pub async fn commit_profile_apply_transaction(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
) -> Result<bool, String> {
    scoped_track_event!(
        "install",
        "commit_profile_apply_transaction",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("profile", TrackEventDebugArg::String(profile_id.as_str()));
            ctx.add_debug_arg("game", TrackEventDebugArg::String(game_identifier.as_str()));
        }
    );
    let targets = transaction_targets(&app, &profile_id, &game_identifier).await?;
    let backup_root = transaction_dir(&targets, &profile_id)?;
    let legacy_backup_root = legacy_transaction_dir(&app, &profile_id)?;
    if let Some(runtime_root) = targets.first().and_then(|target| target.parent()) {
        if targets.contains(&super::profile_activation::marker_path(runtime_root)) {
            if !backup_root.join(APPLY_MARKER).is_file()
                && !legacy_backup_root.join(APPLY_MARKER).is_file()
            {
                return Err(
                    "Cannot record active profile without a completed Apply snapshot".to_string(),
                );
            }
            super::profile_activation::write_active_profile(
                runtime_root,
                &game_identifier,
                &profile_id,
            )?;
        }
    }
    if backup_root.exists() {
        fs::remove_dir_all(&backup_root).map_err(|error| error.to_string())?;
    }
    if legacy_backup_root.exists() {
        if let Err(error) = fs::remove_dir_all(&legacy_backup_root) {
            // The completed game snapshot has already been committed. An old
            // metadata-only snapshot must not turn success into a fake failure.
            log::warn!("[apply_transaction] Could not remove legacy snapshot: {error}");
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_keeps_original_targets_when_config_owner_changes() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-target-plan-{}", uuid::Uuid::new_v4()));
        let app_data = root.join("app");
        let game = root.join("game");
        let snapshot = root.join("snapshot");
        fs::create_dir_all(&snapshot).unwrap();
        let bepinex = game.join("BepInEx");
        let outgoing = app_data.join("profiles/A/.r2modmac/configs/bepinex");
        let incoming = app_data.join("profiles/B/.r2modmac/configs/bepinex");
        let original = vec![bepinex.clone(), outgoing.clone(), incoming.clone()];
        crate::utils::stable_json::write_file(&snapshot.join(APPLY_TARGETS), &original).unwrap();
        let after_owner_change = vec![bepinex, incoming];
        assert_eq!(
            saved_transaction_targets(&snapshot, &after_owner_change, &app_data).unwrap(),
            original
        );
        fs::remove_dir_all(root).unwrap();
    }

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

    #[test]
    fn rollback_restores_config_owner_and_both_profile_backups() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-config-rollback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let game_config = root.join("game/BepInEx/config");
        let app_data = root.join("app");
        let owners = app_data.join("config_owners.json");
        let first_backup = app_data.join("profiles/first/.r2modmac/configs/bepinex");
        let second_backup = app_data.join("profiles/second/.r2modmac/configs/bepinex");
        fs::create_dir_all(&game_config).unwrap();
        fs::create_dir_all(&first_backup).unwrap();
        fs::create_dir_all(&second_backup).unwrap();
        fs::write(game_config.join("mod.cfg"), b"first live").unwrap();
        fs::write(&owners, b"first").unwrap();
        fs::write(first_backup.join("mod.cfg"), b"first saved").unwrap();
        fs::write(second_backup.join("mod.cfg"), b"second saved").unwrap();

        let targets = vec![
            game_config.clone(),
            owners.clone(),
            first_backup.clone(),
            second_backup.clone(),
        ];
        let backup = root.join("snapshot");
        for (index, target) in targets.iter().enumerate() {
            copy_target(target, &backup.join(backup_name(index))).unwrap();
        }

        fs::write(game_config.join("mod.cfg"), b"second live").unwrap();
        fs::write(&owners, b"second").unwrap();
        fs::write(first_backup.join("mod.cfg"), b"first overwritten").unwrap();
        fs::write(second_backup.join("mod.cfg"), b"second overwritten").unwrap();
        restore_snapshot(&backup, &targets).unwrap();

        assert_eq!(
            fs::read(game_config.join("mod.cfg")).unwrap(),
            b"first live"
        );
        assert_eq!(fs::read(owners).unwrap(), b"first");
        assert_eq!(
            fs::read(first_backup.join("mod.cfg")).unwrap(),
            b"first saved"
        );
        assert_eq!(
            fs::read(second_backup.join("mod.cfg")).unwrap(),
            b"second saved"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_restore_preserves_profile_tree_behind_bepinex_link() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-apply-link-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let profile_tree = root.join("profile/BepInEx");
        let game_link = root.join("game/BepInEx");
        let backup = root.join("backup");
        fs::create_dir_all(&profile_tree).unwrap();
        fs::create_dir_all(game_link.parent().unwrap()).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(profile_tree.join("marker"), b"profile-data").unwrap();
        fs::create_dir_all(profile_tree.join("config")).unwrap();
        fs::write(profile_tree.join("config/mod.cfg"), b"old config").unwrap();
        std::os::unix::fs::symlink(&profile_tree, &game_link).unwrap();
        copy_target_with_progress(&game_link, &backup.join(backup_name(0)), &mut |_| {}).unwrap();
        copy_target(&profile_tree.join("config"), &backup.join(backup_name(1))).unwrap();

        fs::remove_file(&game_link).unwrap();
        fs::create_dir_all(&game_link).unwrap();
        fs::write(game_link.join("partial"), b"failed install").unwrap();
        fs::write(profile_tree.join("config/mod.cfg"), b"new config").unwrap();
        restore_snapshot(&backup, &[game_link.clone(), profile_tree.join("config")]).unwrap();

        assert!(game_link.is_symlink());
        assert_eq!(fs::read_link(&game_link).unwrap(), profile_tree);
        assert_eq!(fs::read(game_link.join("marker")).unwrap(), b"profile-data");
        assert_eq!(
            fs::read(game_link.join("config/mod.cfg")).unwrap(),
            b"old config"
        );
        assert!(!game_link.join("partial").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_is_located_on_the_target_volume() {
        let target = std::path::PathBuf::from("/Volumes/Games/MyGame/BepInEx");
        let snapshot = transaction_dir(&[target], "profile-id").unwrap();
        assert_eq!(
            snapshot,
            std::path::PathBuf::from(
                "/Volumes/Games/MyGame/.r2modmac/apply-transactions/profile-id"
            )
        );
    }

    #[test]
    fn cloned_snapshot_survives_in_place_edits_to_the_live_tree() {
        // Apply rewrites doorstop_config.ini and run_bepinex.sh in place after
        // the snapshot is taken. A clone must keep the pre-Apply bytes; if this
        // ever regresses to sharing storage (e.g. hard links) rollback would
        // silently restore already-modified files.
        let root = std::env::temp_dir().join(format!(
            "r2modmac-apply-clone-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = root.join("BepInEx");
        let backup = root.join("backup");
        fs::create_dir_all(target.join("config")).unwrap();
        fs::write(target.join("config/doorstop_config.ini"), b"enabled=false").unwrap();

        if !try_clone_tree(&target, &backup) {
            // Non-APFS volume or a non-macOS host: the copy fallback is covered
            // by the other tests in this module.
            fs::remove_dir_all(&root).ok();
            return;
        }

        fs::write(target.join("config/doorstop_config.ini"), b"enabled=true").unwrap();

        assert_eq!(
            fs::read(backup.join("config/doorstop_config.ini")).unwrap(),
            b"enabled=false",
            "the clone must not observe the in-place edit"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_copy_reports_bytes_as_files_are_written() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-apply-progress-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("one.bin"), b"1234").unwrap();
        fs::write(source.join("nested/two.bin"), b"56789").unwrap();

        let mut copied = 0;
        copy_target_with_progress(&source, &destination, &mut |bytes| copied += bytes).unwrap();

        assert_eq!(copied, 9);
        assert_eq!(
            fs::read(destination.join("nested/two.bin")).unwrap(),
            b"56789"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod rollback_config_tests {
    use super::*;

    fn temp_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "r2modmac-rollback-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// Issue #39: an Apply that fails rolls back, and used to take the settings
    /// the game had written with it.
    #[test]
    fn a_rollback_keeps_the_configs_the_game_wrote_after_the_snapshot() {
        let root = temp_root("keeps");
        let bepinex = root.join("BepInEx");
        let backup = root.join("backup");
        fs::create_dir_all(bepinex.join("config")).unwrap();
        fs::create_dir_all(bepinex.join("plugins")).unwrap();
        fs::write(bepinex.join("config/Existing.cfg"), b"before").unwrap();
        copy_target(&bepinex, &backup.join(backup_name(0))).unwrap();

        fs::write(bepinex.join("config/Existing.cfg"), b"edited by the game").unwrap();
        fs::write(
            bepinex.join("config/xyz.alcan.comfortcalc.cfg"),
            b"user settings",
        )
        .unwrap();
        fs::write(bepinex.join("plugins/half-installed.dll"), b"partial").unwrap();

        restore_snapshot(&backup, std::slice::from_ref(&bepinex)).unwrap();

        assert_eq!(
            fs::read(bepinex.join("config/xyz.alcan.comfortcalc.cfg")).unwrap(),
            b"user settings",
            "a config the snapshot never saw belongs to the user, not to the Apply"
        );
        // A config that was in the snapshot is still rolled back: its contents
        // are part of what the Apply may have changed.
        assert_eq!(
            fs::read(bepinex.join("config/Existing.cfg")).unwrap(),
            b"before"
        );
        assert!(
            !bepinex.join("plugins/half-installed.dll").exists(),
            "everything outside config is still undone"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nested_config_folders_are_kept_too() {
        let root = temp_root("nested");
        let bepinex = root.join("BepInEx");
        let backup = root.join("backup");
        fs::create_dir_all(bepinex.join("config/SomeMod")).unwrap();
        copy_target(&bepinex, &backup.join(backup_name(0))).unwrap();
        fs::write(bepinex.join("config/SomeMod/settings.cfg"), b"deep").unwrap();

        restore_snapshot(&backup, std::slice::from_ref(&bepinex)).unwrap();

        assert_eq!(
            fs::read(bepinex.join("config/SomeMod/settings.cfg")).unwrap(),
            b"deep"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
