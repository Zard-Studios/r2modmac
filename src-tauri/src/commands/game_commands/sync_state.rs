use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::AppHandle;

use super::{
    get_game_path, load_owned_mod_manifests, manifest_matches_target_root,
    resolve_macos_runtime_root, GAME_MANIFEST_SCOPE,
};
use crate::commands::mod_commands::extract_version_number_from_full_name;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSyncInspectionMod {
    package_key: String,
    full_name: String,
    version_number: Option<String>,
    enabled: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSyncInspection {
    status: String,
    runtime: String,
    mods: Vec<ProfileSyncInspectionMod>,
    unresolved_keys: Vec<String>,
}

/// The loader a community runs on, per the Thunderstore ecosystem schema.
///
/// Substring matching on the game identifier only ever knew the three games
/// that had been special-cased, so every other non-BepInEx community - Hades II
/// among them - was inspected as if it were a BepInEx game.
fn runtime_name(game_identifier: &str) -> String {
    // Outer Wilds is modded through OWML and is not a Thunderstore community,
    // so it has no schema entry to consult.
    if crate::models::shared::is_outerwilds_identifier(game_identifier) {
        return "owml".to_string();
    }
    crate::models::loaders::loader_for_community(game_identifier)
        .unwrap_or(crate::models::loaders::PackageLoader::BepInEx)
        .runtime_name()
        .to_string()
}

fn manifest_files_present(target_root: &Path, files: &[String]) -> bool {
    files.iter().any(|relative| {
        let path = target_root.join(relative);
        path.exists() || path.is_symlink()
    })
}

#[tauri::command]
pub async fn inspect_profile_sync_state(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
    platform: Option<String>,
) -> Result<ProfileSyncInspection, String> {
    let runtime = runtime_name(&game_identifier);
    let Some(game_path_string) =
        get_game_path(app.clone(), game_identifier, platform.clone()).await?
    else {
        return Ok(ProfileSyncInspection {
            status: "unconfigured".into(),
            runtime,
            mods: Vec::new(),
            unresolved_keys: Vec::new(),
        });
    };
    let game_path = Path::new(&game_path_string);
    let target_root = if platform.as_deref() == Some("mac") && runtime == "bepinex" {
        resolve_macos_runtime_root(game_path)
    } else {
        game_path.to_path_buf()
    };

    let profiles_path = crate::utils::paths::app_data_dir(&app)
        .map_err(|error| error.to_string())?
        .join("profiles.json");
    let profile_enabled: HashMap<String, bool> = std::fs::read_to_string(profiles_path)
        .ok()
        .and_then(|data| serde_json::from_str::<serde_json::Value>(&data).ok())
        .and_then(|profiles| profiles.as_array().cloned())
        .and_then(|profiles| {
            profiles
                .into_iter()
                .find(|profile| profile["id"].as_str() == Some(&profile_id))
        })
        .and_then(|profile| profile["mods"].as_array().cloned())
        .map(|mods| {
            mods.into_iter()
                .filter_map(|mod_value| {
                    let full_name = mod_value["fullName"].as_str()?;
                    let parts = full_name.split('-').collect::<Vec<_>>();
                    let key = if parts.len() >= 2 {
                        format!("{}-{}", parts[0], parts[1]).to_ascii_lowercase()
                    } else {
                        full_name.to_ascii_lowercase()
                    };
                    let enabled = mod_value["synced_enabled"]
                        .as_bool()
                        .or_else(|| mod_value["enabled"].as_bool())?;
                    Some((key, enabled))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut seen = HashSet::new();
    let mut mods = Vec::new();
    for stored in load_owned_mod_manifests(&app, &profile_id, GAME_MANIFEST_SCOPE)? {
        if !manifest_matches_target_root(&stored.manifest, &target_root) {
            continue;
        }
        let key = stored.manifest.mod_key.to_ascii_lowercase();
        let full_name = stored.manifest.mod_full_name;
        let version_number = extract_version_number_from_full_name(&full_name);
        let files_present = manifest_files_present(&target_root, &stored.manifest.files);
        // A manifest records what this profile used to own; it is not evidence
        // that those files still exist. Reporting stale manifests as installed
        // made a deleted runtime look synchronized and prevented the frontend
        // from creating any Sync entries.
        if !files_present {
            continue;
        }
        if !seen.insert(key.clone()) {
            continue;
        }
        let enabled = profile_enabled.get(&key).copied().or(Some(files_present));
        mods.push(ProfileSyncInspectionMod {
            package_key: key,
            full_name,
            version_number,
            enabled,
        });
    }
    mods.sort_by(|a, b| a.package_key.cmp(&b.package_key));
    Ok(ProfileSyncInspection {
        status: "ready".into(),
        runtime,
        mods,
        unresolved_keys: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::{manifest_files_present, runtime_name};
    use std::fs;

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-sync-inspection-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn identifies_supported_runtime_families() {
        assert_eq!(runtime_name("outerwilds"), "owml");
        assert_eq!(runtime_name("balatro"), "lovely");
        assert_eq!(runtime_name("risk-of-rain-returns"), "returnofmodding");
        assert_eq!(runtime_name("hades-ii"), "returnofmodding");
        assert_eq!(runtime_name("lethal-company"), "bepinex");
    }

    #[test]
    fn a_stale_manifest_does_not_claim_deleted_game_files() {
        let root = temp_root("missing");
        let files = vec!["BepInEx/plugins/Someone-ModA/Plugin.dll".to_string()];

        assert!(!manifest_files_present(&root, &files));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn an_existing_manifest_file_is_reported_as_installed() {
        let root = temp_root("present");
        let relative = "BepInEx/plugins/Someone-ModA/Plugin.dll";
        fs::create_dir_all(root.join("BepInEx/plugins/Someone-ModA")).unwrap();
        fs::write(root.join(relative), b"plugin").unwrap();

        assert!(manifest_files_present(&root, &[relative.to_string()]));
        fs::remove_dir_all(root).ok();
    }
}
