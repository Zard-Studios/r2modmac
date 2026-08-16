use super::*;
use crate::tracing::{perfetto_te_ns, scoped_track_event, EventContext, TrackEventDebugArg};
use tauri::command;

const APPLY_BACKUP_DIR: &str = "apply-transaction";
const APPLY_MARKER: &str = "ready";

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

    if crate::models::loaders::uses_return_of_modding(game_identifier, &game_root) {
        // The pack's proxy DLL is named per pack (version.dll for
        // ReturnOfModding, d3d12.dll for Hell2Modding), so every name a pack
        // can install has to be part of the transaction snapshot.
        let mut targets = vec![game_root.join("ReturnOfModding")];
        for name in crate::models::loaders::RETURN_OF_MODDING_PROXY_NAMES {
            targets.push(game_root.join(name));
            targets.push(game_root.join(format!("{name}_DISABLED")));
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

fn target_size(target: &std::path::Path) -> u64 {
    if target.is_dir() {
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
    if source.is_dir() {
        fs::create_dir_all(destination).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let source_path = entry.path();
            let destination_path = destination.join(entry.file_name());
            if entry
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
    scoped_track_event!("install", "restore_snapshot", |ctx: &mut EventContext| {
        ctx.add_debug_arg(
            "backup",
            TrackEventDebugArg::String(backup_root.to_string_lossy().as_ref()),
        );
    });
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
            restore_snapshot(&legacy_backup_root, &targets)?;
        }
        fs::remove_dir_all(&legacy_backup_root).map_err(|error| error.to_string())?;
    }
    if backup_root.exists() {
        if backup_root.join(APPLY_MARKER).is_file() {
            restore_snapshot(&backup_root, &targets)?;
        }
        fs::remove_dir_all(&backup_root).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&backup_root).map_err(|error| error.to_string())?;

    // Fast path: clone every target copy-on-write. When this works the snapshot
    // is metadata-only, so there is no point walking the tree to size it first —
    // that walk alone used to cost more than the whole operation now does.
    let mut pending: Vec<(usize, &std::path::PathBuf)> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        if !target.exists() {
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
            restore_snapshot(&backup_root, &targets)?;
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
    for backup_root in [
        transaction_dir(&targets, &profile_id)?,
        legacy_transaction_dir(&app, &profile_id)?,
    ] {
        if backup_root.exists() {
            fs::remove_dir_all(backup_root).map_err(|error| error.to_string())?;
        }
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
