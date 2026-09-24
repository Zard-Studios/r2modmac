use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path},
};

use sha2::{Digest, Sha256};

use tauri::{command, AppHandle};

use super::{
    choose_bepinex_root, detach_isolated_bepinex_link, get_game_path, is_game_running,
    resolve_macos_runtime_root, sync_isolated_bepinex_link,
};
use crate::utils::file_ops::copy_dir_recursive;
use crate::utils::mod_manifest::{
    load_owned_mod_manifests, manifest_matches_target_root, ModOwnershipManifest,
    StoredModOwnershipManifest, GAME_MANIFEST_SCOPE, PROFILE_MANIFEST_SCOPE,
};

fn file_digest(path: &Path) -> Result<[u8; 32], String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

/// Check that an isolated profile can be reconstructed entirely from local
/// files before changing any game-side path. This is deliberately strict:
/// old/incomplete ownership metadata needs a recovery path, not a download
/// hidden inside a storage-mode toggle.
fn preflight_local_bepinex_payload(
    profile_root: &Path,
    game_root: &Path,
    enabled_mods: &[String],
    manifests: &[ModOwnershipManifest],
) -> Result<(), String> {
    for full_name in enabled_mods {
        let matching = manifests
            .iter()
            .filter(|manifest| manifest.mod_full_name.eq_ignore_ascii_case(full_name))
            .collect::<Vec<_>>();
        if matching.len() != 1 || matching[0].files.is_empty() {
            return Err(format!(
                "Cannot migrate {full_name} offline: its local file inventory is missing or ambiguous"
            ));
        }
        for relative in &matching[0].files {
            let path = Path::new(relative);
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(format!(
                    "Cannot migrate {full_name}: unsafe inventory path {relative}"
                ));
            }
            let first = path.components().next();
            let source_root = if matches!(first, Some(Component::Normal(part)) if part == "BepInEx" || part == "BepInEx_DISABLED")
            {
                profile_root
            } else {
                // With isolation the loader entry points and non-BepInEx
                // root files were installed beside the game, not in the
                // profile-side BepInEx tree.
                game_root
            };
            let mut current = source_root.to_path_buf();
            for component in path.components() {
                current.push(component);
                let metadata = fs::symlink_metadata(&current).map_err(|error| {
                    format!(
                        "Cannot migrate {full_name} offline: missing {} ({error})",
                        current.display()
                    )
                })?;
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "Cannot migrate {full_name}: inventory traverses symlink {}",
                        current.display()
                    ));
                }
            }
            if !current.is_file() {
                return Err(format!(
                    "Cannot migrate {full_name}: inventory entry is not a file: {}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

fn enabled_profile_mod_names(profile: &serde_json::Value) -> Result<Vec<String>, String> {
    let mods = profile["mods"]
        .as_array()
        .ok_or_else(|| "Cannot migrate offline: profile mod list is missing".to_string())?;
    mods.iter()
        .filter(|item| item["enabled"].as_bool().unwrap_or(true))
        .map(|item| {
            item["fullName"]
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    "Cannot migrate offline: an enabled mod has no package identity".to_string()
                })
        })
        .collect()
}

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

fn verify_copied_tree(source: &Path, staged: &Path) -> Result<(), String> {
    let mut source_entries = fs::read_dir(source)
        .map_err(|error| error.to_string())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut staged_entries = fs::read_dir(staged)
        .map_err(|error| error.to_string())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    source_entries.sort();
    staged_entries.sort();
    if source_entries != staged_entries {
        return Err(format!(
            "Copied BepInEx tree differs from {}",
            source.display()
        ));
    }

    for name in source_entries {
        let original = source.join(&name);
        let copied = staged.join(&name);
        let original_type = fs::symlink_metadata(&original)
            .map_err(|error| error.to_string())?
            .file_type();
        let copied_type = fs::symlink_metadata(&copied)
            .map_err(|error| error.to_string())?
            .file_type();
        if original_type.is_dir() && copied_type.is_dir() {
            verify_copied_tree(&original, &copied)?;
        } else if original_type.is_file() && copied_type.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let original_mode = fs::metadata(&original)
                    .map_err(|error| error.to_string())?
                    .permissions()
                    .mode();
                let copied_mode = fs::metadata(&copied)
                    .map_err(|error| error.to_string())?
                    .permissions()
                    .mode();
                if original_mode & 0o111 != copied_mode & 0o111 {
                    return Err(format!(
                        "Copied BepInEx file lost executable permissions: {}",
                        original.display()
                    ));
                }
            }
            if file_digest(&original)? != file_digest(&copied)? {
                return Err(format!(
                    "Copied BepInEx file differs from {}",
                    original.display()
                ));
            }
        } else {
            return Err(format!(
                "Copied BepInEx entry has a different type: {}",
                original.display()
            ));
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
    if !has_source {
        return Err(format!(
            "Cannot migrate missing BepInEx tree at {}",
            source.display()
        ));
    }
    ensure_no_symlinks(&source)?;
    fs::create_dir_all(destination_root).map_err(|error| error.to_string())?;
    let marker = uuid::Uuid::new_v4();
    let staged = destination_root.join(format!(".{name}.r2modmac-{marker}.staging"));
    let backup = destination_root.join(format!("{name}.r2modmac-backup-{marker}"));
    if let Err(error) = copy_dir_recursive(&source, &staged) {
        let _ = fs::remove_dir_all(&staged);
        return Err(format!("Could not stage {}: {error}", source.display()));
    }
    if let Err(error) = verify_copied_tree(&source, &staged) {
        let _ = fs::remove_dir_all(&staged);
        return Err(error);
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
    if let Err(error) = fs::rename(&staged, &destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        return Err(format!(
            "Could not activate {}: {error}",
            destination.display()
        ));
    }
    Ok(())
}

fn game_local_layout_needs_reconciliation(profile_root: &Path, runtime_root: &Path) -> bool {
    ["BepInEx", "BepInEx_DISABLED"].iter().any(|name| {
        let game_tree = runtime_root.join(name);
        game_tree.is_symlink() || (!game_tree.is_dir() && profile_root.join(name).is_dir())
    })
}

fn manifest_needs_game_scope(
    profile: &StoredModOwnershipManifest,
    game_manifests: &[StoredModOwnershipManifest],
    game_root: &Path,
) -> bool {
    !game_manifests.iter().any(|game| {
        game.manifest.mod_full_name == profile.manifest.mod_full_name
            && game.manifest.mod_key == profile.manifest.mod_key
            && game.manifest.files == profile.manifest.files
            && game.manifest.backed_up_files == profile.manifest.backed_up_files
            && manifest_matches_target_root(&game.manifest, game_root)
    })
}

fn current_profile_manifest_names(profile: &serde_json::Value) -> HashSet<String> {
    profile["mods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["fullName"].as_str())
        .map(|name| name.to_ascii_lowercase())
        .collect()
}

pub(super) fn game_local_manifest_mismatch(
    app: &AppHandle,
    profile_id: &str,
    game_root: &Path,
) -> Result<bool, String> {
    let profiles = crate::commands::profile_commands::get_profiles(app.clone())?;
    let profile = profiles
        .iter()
        .find(|profile| profile["id"].as_str() == Some(profile_id))
        .ok_or("Profile not found")?;
    let current_mods = current_profile_manifest_names(profile);
    let profile_manifests = load_owned_mod_manifests(app, profile_id, PROFILE_MANIFEST_SCOPE)?
        .into_iter()
        .filter(|stored| current_mods.contains(&stored.manifest.mod_full_name.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    if profile_manifests.is_empty() {
        return Ok(false);
    }
    let game_manifests = load_owned_mod_manifests(app, profile_id, GAME_MANIFEST_SCOPE)?;
    Ok(profile_manifests
        .iter()
        .any(|profile| manifest_needs_game_scope(profile, &game_manifests, game_root)))
}

fn safe_inventory_path(relative: &Path) -> bool {
    !relative.as_os_str().is_empty()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn regular_inventory_file(root: &Path, relative: &Path) -> bool {
    if !safe_inventory_path(relative) {
        return false;
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    path.is_file()
}

/// Retain the original profile inventory. A partially copied game inventory
/// can be retried without deleting any original metadata or mod payload.
fn copy_manifests_to_game_scope(
    profile_root: &Path,
    game_root: &Path,
    profile_manifests: &[StoredModOwnershipManifest],
    game_manifests: &[StoredModOwnershipManifest],
) -> Result<(), String> {
    if profile_manifests.is_empty() {
        return Ok(());
    }
    let source_dir = profile_manifests[0]
        .manifest_path
        .parent()
        .ok_or("Invalid profile manifest location")?;
    let game_dir = source_dir
        .parent()
        .ok_or("Invalid manifest scope location")?
        .join(GAME_MANIFEST_SCOPE);
    if game_dir.is_symlink() {
        return Err(format!("Refusing to write through {}", game_dir.display()));
    }

    let target_hint = fs::canonicalize(game_root)
        .map_err(|error| format!("Cannot resolve game directory: {error}"))?
        .to_string_lossy()
        .to_string();
    let mut prepared = Vec::new();
    for stored in profile_manifests {
        if stored.manifest_path.is_symlink() {
            return Err(format!(
                "Refusing to read {}",
                stored.manifest_path.display()
            ));
        }
        let mut migrated = stored.manifest.clone();
        migrated.target_root_hint = Some(target_hint.clone());
        let name = stored
            .manifest_path
            .file_name()
            .ok_or("Invalid manifest filename")?;
        let destination = game_dir.join(name);
        if destination.is_symlink() {
            return Err(format!("Refusing to use {}", destination.display()));
        }
        if game_manifests.iter().any(|entry| {
            entry.manifest.mod_key == stored.manifest.mod_key && entry.manifest_path != destination
        }) {
            return Err(format!(
                "Game inventory already claims package {} under another manifest",
                stored.manifest.mod_full_name
            ));
        }
        if let Some(existing) = game_manifests
            .iter()
            .find(|entry| entry.manifest_path == destination)
        {
            if serde_json::to_value(&existing.manifest).map_err(|error| error.to_string())?
                != serde_json::to_value(&migrated).map_err(|error| error.to_string())?
            {
                return Err(format!(
                    "Game inventory conflicts with {}. No manifest was replaced",
                    destination.display()
                ));
            }
        } else if destination.exists() || destination.is_symlink() {
            return Err(format!(
                "Unrecognized game inventory at {}",
                destination.display()
            ));
        }
        let source_backup = &stored.backup_dir;
        let destination_backup = game_dir.join(
            source_backup
                .file_name()
                .ok_or("Invalid manifest backup location")?,
        );
        if source_backup.is_dir() {
            ensure_no_symlinks(source_backup)?;
            if destination_backup.exists() || destination_backup.is_symlink() {
                if destination_backup.is_symlink()
                    || !destination_backup.is_dir()
                    || verify_copied_tree(source_backup, &destination_backup).is_err()
                {
                    return Err(format!(
                        "Game inventory backup conflicts at {}",
                        destination_backup.display()
                    ));
                }
            }
        } else if source_backup.exists() || source_backup.is_symlink() {
            return Err(format!(
                "Invalid manifest backup at {}",
                source_backup.display()
            ));
        } else if !stored.manifest.backed_up_files.is_empty() {
            return Err(format!(
                "Missing manifest backup at {}",
                source_backup.display()
            ));
        }
        for relative in &stored.manifest.backed_up_files {
            if !regular_inventory_file(source_backup, Path::new(relative)) {
                return Err(format!("Missing or unsafe inventory backup: {relative}"));
            }
        }

        for relative in &stored.manifest.files {
            let path = Path::new(relative);
            let game_file = game_root.join(path);
            if !regular_inventory_file(game_root, path) {
                return Err(format!(
                    "Game file is missing or unsafe: {}",
                    game_file.display()
                ));
            }
            if matches!(path.components().next(), Some(Component::Normal(part)) if part == "BepInEx" || part == "BepInEx_DISABLED")
            {
                let source_file = profile_root.join(path);
                if !regular_inventory_file(profile_root, path)
                    || file_digest(&source_file)? != file_digest(&game_file)?
                {
                    return Err(format!(
                        "Game file differs from this profile: {}",
                        game_file.display()
                    ));
                }
            }
        }
        prepared.push((
            destination,
            destination_backup,
            source_backup.clone(),
            migrated,
        ));
    }

    fs::create_dir_all(&game_dir).map_err(|error| error.to_string())?;
    for (destination, destination_backup, source_backup, migrated) in prepared {
        if source_backup.is_dir() && !destination_backup.exists() {
            let staged_backup =
                game_dir.join(format!(".r2modmac-backup-{}.staging", uuid::Uuid::new_v4()));
            crate::utils::file_ops::copy_dir_recursive(&source_backup, &staged_backup)
                .map_err(|error| error.to_string())?;
            fs::rename(&staged_backup, &destination_backup).map_err(|error| error.to_string())?;
        }
        if !destination.exists() {
            let staged = game_dir.join(format!(
                ".r2modmac-manifest-{}.staging",
                uuid::Uuid::new_v4()
            ));
            crate::utils::stable_json::write_file(&staged, &migrated)?;
            fs::rename(&staged, &destination).map_err(|error| error.to_string())?;
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
    if current == isolated && isolated {
        return Ok(true);
    }
    let game_path = get_game_path(app.clone(), game_identifier.clone(), Some(platform.clone()))
        .await?
        .ok_or_else(|| "Set the game directory before changing BepInEx storage mode".to_string())?;
    let runtime_root = if platform == "mac" {
        resolve_macos_runtime_root(Path::new(&game_path))
    } else {
        Path::new(&game_path).to_path_buf()
    };
    if crate::models::loaders::resolve_loader(&game_identifier, &runtime_root)
        != crate::models::loaders::PackageLoader::BepInEx
    {
        return Err("BepInEx isolation is only available for BepInEx games".to_string());
    }
    if current && !isolated {
        if let Some(active) =
            super::profile_activation::read_active_profile(&runtime_root, &game_identifier)?
        {
            if active != profile_id {
                return Err("Another profile is recorded as active in this game. Select and apply that profile before changing this one's BepInEx storage mode.".to_string());
            }
        }
    }
    let profile_root = crate::utils::paths::app_data_dir(&app)
        .map_err(|error| error.to_string())?
        .join("profiles")
        .join(&profile_id);
    let current_mods = current_profile_manifest_names(profile);
    let profile_manifests = if isolated {
        Vec::new()
    } else {
        load_owned_mod_manifests(&app, &profile_id, PROFILE_MANIFEST_SCOPE)?
            .into_iter()
            .filter(|stored| {
                current_mods.contains(&stored.manifest.mod_full_name.to_ascii_lowercase())
            })
            .collect()
    };
    let game_manifests = if isolated {
        Vec::new()
    } else {
        load_owned_mod_manifests(&app, &profile_id, GAME_MANIFEST_SCOPE)?
    };
    let needs_layout_reconciliation =
        !isolated && game_local_layout_needs_reconciliation(&profile_root, &runtime_root);
    let needs_manifest_reconciliation = !isolated
        && profile_manifests
            .iter()
            .any(|manifest| manifest_needs_game_scope(manifest, &game_manifests, &runtime_root));
    let needs_reconciliation = needs_layout_reconciliation || needs_manifest_reconciliation;
    if current == isolated && !needs_reconciliation {
        return Ok(true);
    }
    if is_game_running(app.clone(), game_identifier.clone(), Some(platform.clone())).await? {
        return Err("Close the game before changing BepInEx storage mode".to_string());
    }
    if isolated && choose_bepinex_root(true, &profile_root, &runtime_root) == runtime_root {
        return Err(
            "This Wine bottle cannot access an isolated BepInEx tree; use game-local mode"
                .to_string(),
        );
    }
    // Wine may already have fallen back to the game-local tree even though an
    // older profile inherited the isolation flag. There is nothing to copy in
    // that case, and requiring a profile-side tree would make OFF impossible.
    if !isolated
        && !needs_reconciliation
        && choose_bepinex_root(current, &profile_root, &runtime_root) == runtime_root
    {
        profile["bepinexIsolation"] = serde_json::Value::Bool(false);
        crate::commands::profile_commands::save_profiles(app, profiles).await?;
        return Ok(true);
    }
    let (source, destination) = if isolated {
        (&runtime_root, &profile_root)
    } else {
        (&profile_root, &runtime_root)
    };
    if !isolated
        && !["BepInEx", "BepInEx_DISABLED"]
            .iter()
            .any(|name| source.join(name).is_dir())
    {
        return Err(
            "This profile has no local BepInEx files to migrate; its mode was not changed"
                .to_string(),
        );
    }
    if !isolated {
        let enabled_mods = enabled_profile_mod_names(profile)?;
        let enabled_to_preflight = if current || needs_layout_reconciliation {
            enabled_mods
        } else {
            enabled_mods
                .into_iter()
                .filter(|name| {
                    profile_manifests
                        .iter()
                        .any(|stored| stored.manifest.mod_full_name.eq_ignore_ascii_case(name))
                })
                .collect()
        };
        let manifests = profile_manifests
            .iter()
            .map(|stored| stored.manifest.clone())
            .collect::<Vec<_>>();
        preflight_local_bepinex_payload(
            &profile_root,
            &runtime_root,
            &enabled_to_preflight,
            &manifests,
        )?;
        // Validate every tree before replacing either one. In particular, a
        // disabled-tree symlink cannot be detached by migrate_tree; finding it
        // only after moving the active tree would leave a half-migrated game.
        for name in ["BepInEx", "BepInEx_DISABLED"] {
            let source_tree = profile_root.join(name);
            let game_tree = runtime_root.join(name);
            if source_tree.is_symlink() {
                return Err(format!(
                    "Refusing to copy through {}",
                    source_tree.display()
                ));
            }
            if source_tree.is_dir() {
                ensure_no_symlinks(&source_tree)?;
                if game_tree.is_symlink() && name != "BepInEx" {
                    return Err(format!(
                        "Refusing to replace the symlink {}",
                        game_tree.display()
                    ));
                }
            }
        }
    }
    for name in ["BepInEx", "BepInEx_DISABLED"] {
        // A profile already marked game-local may only lack metadata, or just
        // one of the two trees. Never replace a real game tree in either case.
        if !isolated && !current {
            let game_tree = runtime_root.join(name);
            if game_tree.is_dir() && !game_tree.is_symlink() {
                continue;
            }
        }
        // A game-side link may belong to another profile. It is never a
        // source for a migration into this profile.
        if isolated && source.join(name).is_symlink() {
            continue;
        }
        if !source.join(name).is_dir() && !source.join(name).is_symlink() {
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
        copy_manifests_to_game_scope(
            &profile_root,
            &runtime_root,
            &profile_manifests,
            &game_manifests,
        )?;
        if current || needs_layout_reconciliation {
            crate::commands::mod_commands::point_game_doorstop_ini_at_tree(
                &runtime_root,
                &runtime_root,
            )?;
        }
    }
    profile["bepinexIsolation"] = serde_json::Value::Bool(isolated);
    profile["needs_sync"] = serde_json::Value::Bool(true);
    crate::commands::profile_commands::save_profiles(app, profiles).await?;
    if current && !isolated {
        super::profile_activation::write_active_profile(
            &runtime_root,
            &game_identifier,
            &profile_id,
        )?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(full_name: &str, files: &[&str]) -> ModOwnershipManifest {
        ModOwnershipManifest {
            mod_full_name: full_name.to_string(),
            files: files.iter().map(|file| (*file).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn removed_mods_do_not_request_a_second_inventory_migration() {
        let current = current_profile_manifest_names(&serde_json::json!({
            "mods": [
                {"fullName": "Author-Current-1.0.0", "enabled": true},
                {"fullName": "Author-Disabled-1.0.0", "enabled": false}
            ]
        }));
        assert!(current.contains("author-current-1.0.0"));
        assert!(current.contains("author-disabled-1.0.0"));
        assert!(!current.contains("author-removed-1.0.0"));
    }

    #[test]
    fn game_scope_inventory_copy_is_verified_and_retryable() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-manifest-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let profile = root.join("profile");
        let game = root.join("game");
        let scope = profile.join(".r2modmac/manifests/profile");
        fs::create_dir_all(profile.join("BepInEx/plugins")).unwrap();
        fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        fs::create_dir_all(&scope).unwrap();
        fs::write(profile.join("BepInEx/plugins/mod.dll"), b"mod").unwrap();
        fs::write(game.join("BepInEx/plugins/mod.dll"), b"mod").unwrap();
        let mut owned = manifest("Author-Mod-1.0.0", &["BepInEx/plugins/mod.dll"]);
        owned.backed_up_files = vec!["BepInEx/plugins/mod.dll".to_string()];
        owned.match_terms = vec!["mod".to_string()];
        owned.target_root_hint = Some(profile.to_string_lossy().to_string());
        let original = scope.join("mod.json");
        crate::utils::stable_json::write_file(&original, &owned).unwrap();
        fs::create_dir_all(scope.join("mod_backup/BepInEx/plugins")).unwrap();
        fs::write(
            scope.join("mod_backup/BepInEx/plugins/mod.dll"),
            b"previous",
        )
        .unwrap();
        let stored = StoredModOwnershipManifest {
            manifest_path: original.clone(),
            backup_dir: scope.join("mod_backup"),
            manifest: owned.clone(),
        };

        assert!(manifest_needs_game_scope(&stored, &[], &game));
        copy_manifests_to_game_scope(&profile, &game, &[stored.clone()], &[]).unwrap();
        let copied_path = scope.parent().unwrap().join("game/mod.json");
        let copied: ModOwnershipManifest =
            serde_json::from_slice(&fs::read(&copied_path).unwrap()).unwrap();
        assert_eq!(copied.files, owned.files);
        assert_eq!(copied.backed_up_files, owned.backed_up_files);
        assert_eq!(
            fs::read(
                scope
                    .parent()
                    .unwrap()
                    .join("game/mod_backup/BepInEx/plugins/mod.dll")
            )
            .unwrap(),
            b"previous"
        );
        assert!(manifest_matches_target_root(&copied, &game));
        assert_eq!(
            fs::read(&original).unwrap(),
            crate::utils::stable_json::to_pretty_string(&owned)
                .unwrap()
                .as_bytes()
        );
        let copied_stored = StoredModOwnershipManifest {
            manifest_path: copied_path,
            backup_dir: scope.parent().unwrap().join("game/mod_backup"),
            manifest: copied,
        };
        assert!(!manifest_needs_game_scope(
            &stored,
            &[copied_stored.clone()],
            &game
        ));
        copy_manifests_to_game_scope(&profile, &game, &[stored.clone()], &[copied_stored.clone()])
            .unwrap();

        let mut conflicting = copied_stored.clone();
        conflicting.manifest.mod_full_name = "Other-Owner-9.0.0".to_string();
        crate::utils::stable_json::write_file(&conflicting.manifest_path, &conflicting.manifest)
            .unwrap();
        let conflicting_bytes = fs::read(&conflicting.manifest_path).unwrap();
        assert!(copy_manifests_to_game_scope(
            &profile,
            &game,
            &[stored.clone()],
            &[conflicting.clone()]
        )
        .unwrap_err()
        .contains("conflicts"));
        assert_eq!(
            fs::read(&conflicting.manifest_path).unwrap(),
            conflicting_bytes
        );
        crate::utils::stable_json::write_file(
            &copied_stored.manifest_path,
            &copied_stored.manifest,
        )
        .unwrap();

        fs::write(game.join("BepInEx/plugins/mod.dll"), b"other profile").unwrap();
        assert!(
            copy_manifests_to_game_scope(&profile, &game, &[stored], &[copied_stored.clone()])
                .unwrap_err()
                .contains("differs")
        );
        fs::write(game.join("BepInEx/plugins/mod.dll"), b"mod").unwrap();
        crate::utils::mod_manifest::cleanup_owned_mod_manifests(&game, &[copied_stored], &[])
            .unwrap();
        assert_eq!(
            fs::read(game.join("BepInEx/plugins/mod.dll")).unwrap(),
            b"previous"
        );
        assert!(original.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn game_local_reconciliation_detects_stale_link_without_switch_toggle() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-reconciliation-{}", uuid::Uuid::new_v4()));
        let profile = root.join("profile");
        let game = root.join("game");
        fs::create_dir_all(profile.join("BepInEx")).unwrap();
        fs::create_dir_all(&game).unwrap();
        assert!(game_local_layout_needs_reconciliation(&profile, &game));
        std::os::unix::fs::symlink(profile.join("BepInEx"), game.join("BepInEx")).unwrap();
        assert!(game_local_layout_needs_reconciliation(&profile, &game));
        fs::remove_file(game.join("BepInEx")).unwrap();
        fs::create_dir_all(game.join("BepInEx")).unwrap();
        assert!(!game_local_layout_needs_reconciliation(&profile, &game));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_preflight_checks_profile_payload_and_game_side_loader() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-profile-preflight-{}",
            uuid::Uuid::new_v4()
        ));
        let profile = root.join("profile");
        let game = root.join("game");
        fs::create_dir_all(profile.join("BepInEx/plugins")).unwrap();
        fs::create_dir_all(&game).unwrap();
        fs::write(profile.join("BepInEx/plugins/mod.dll"), b"mod").unwrap();
        fs::write(game.join("run_bepinex.sh"), b"loader").unwrap();
        let mods = vec!["Author-Mod-1.0.0".to_string()];
        let inventory = vec![manifest(
            "Author-Mod-1.0.0",
            &["BepInEx/plugins/mod.dll", "run_bepinex.sh"],
        )];
        assert!(preflight_local_bepinex_payload(&profile, &game, &mods, &inventory).is_ok());

        fs::remove_file(profile.join("BepInEx/plugins/mod.dll")).unwrap();
        assert!(
            preflight_local_bepinex_payload(&profile, &game, &mods, &inventory)
                .unwrap_err()
                .contains("missing")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_preflight_rejects_missing_inventory_and_path_traversal() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-profile-preflight-{}",
            uuid::Uuid::new_v4()
        ));
        let mods = vec!["Author-Mod-1.0.0".to_string()];
        assert!(preflight_local_bepinex_payload(&root, &root, &mods, &[]).is_err());
        let unsafe_inventory = vec![manifest("Author-Mod-1.0.0", &["../outside.dll"])];
        assert!(
            preflight_local_bepinex_payload(&root, &root, &mods, &unsafe_inventory)
                .unwrap_err()
                .contains("unsafe")
        );
    }

    #[test]
    fn offline_preflight_does_not_ignore_enabled_mod_without_identity() {
        assert!(enabled_profile_mod_names(&serde_json::json!({
            "mods": [{"enabled": true}]
        }))
        .is_err());
        assert!(enabled_profile_mod_names(&serde_json::json!({
            "mods": [{"enabled": false}]
        }))
        .unwrap()
        .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn offline_preflight_refuses_symlinked_payload() {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-profile-preflight-{}",
            uuid::Uuid::new_v4()
        ));
        let profile = root.join("profile");
        let game = root.join("game");
        fs::create_dir_all(profile.join("BepInEx/plugins")).unwrap();
        fs::create_dir_all(&game).unwrap();
        std::os::unix::fs::symlink(&game, profile.join("BepInEx/plugins/external")).unwrap();
        let inventory = vec![manifest(
            "Author-Mod-1.0.0",
            &["BepInEx/plugins/external/mod.dll"],
        )];
        assert!(preflight_local_bepinex_payload(
            &profile,
            &game,
            &["Author-Mod-1.0.0".to_string()],
            &inventory,
        )
        .unwrap_err()
        .contains("symlink"));
        fs::remove_dir_all(root).unwrap();
    }

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

    #[test]
    fn missing_source_never_displaces_existing_game_tree() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-profile-mode-{}", uuid::Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(destination.join("BepInEx/plugins")).unwrap();
        fs::write(destination.join("BepInEx/plugins/working.dll"), b"keep me").unwrap();

        assert!(migrate_tree(&source, &destination, "BepInEx").is_err());
        assert_eq!(
            fs::read(destination.join("BepInEx/plugins/working.dll")).unwrap(),
            b"keep me"
        );
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_tree_verification_detects_same_size_corruption() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-profile-mode-{}", uuid::Uuid::new_v4()));
        let source = root.join("source");
        let staged = root.join("staged");
        fs::create_dir_all(source.join("plugins")).unwrap();
        fs::create_dir_all(staged.join("plugins")).unwrap();
        fs::write(source.join("plugins/mod.dll"), b"correct").unwrap();
        fs::write(staged.join("plugins/mod.dll"), b"corrupt").unwrap();

        assert!(verify_copied_tree(&source, &staged).is_err());
        fs::write(staged.join("plugins/mod.dll"), b"correct").unwrap();
        assert!(verify_copied_tree(&source, &staged).is_ok());
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
