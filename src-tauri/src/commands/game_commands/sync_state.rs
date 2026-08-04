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

fn runtime_name(game_identifier: &str) -> &'static str {
    let normalized = game_identifier.to_ascii_lowercase();
    if normalized.contains("outerwilds") || normalized.contains("outer-wilds") {
        "owml"
    } else if normalized.contains("balatro") {
        "lovely"
    } else if normalized.contains("riskofrainreturns")
        || normalized.contains("risk-of-rain-returns")
    {
        "returnofmodding"
    } else {
        "bepinex"
    }
}

#[tauri::command]
pub async fn inspect_profile_sync_state(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
    platform: Option<String>,
) -> Result<ProfileSyncInspection, String> {
    let runtime = runtime_name(&game_identifier).to_string();
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
        if !seen.insert(key.clone()) {
            continue;
        }
        let full_name = stored.manifest.mod_full_name;
        let version_number = extract_version_number_from_full_name(&full_name);
        let files_present = stored.manifest.files.iter().any(|relative| {
            let path = target_root.join(relative);
            path.exists() || path.is_symlink()
        });
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
    use super::runtime_name;

    #[test]
    fn identifies_supported_runtime_families() {
        assert_eq!(runtime_name("outerwilds"), "owml");
        assert_eq!(runtime_name("balatro"), "lovely");
        assert_eq!(runtime_name("risk-of-rain-returns"), "returnofmodding");
        assert_eq!(runtime_name("lethal-company"), "bepinex");
    }
}
