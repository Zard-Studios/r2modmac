use std::fs;
use std::path::{Path, PathBuf};
use serde_json::Value;

#[derive(serde::Deserialize, Debug, Clone)]
struct SimpleManifest {
    mod_full_name: String,
    mod_key: String,
    files: Vec<String>,
}

fn extract_mod_key(name: &str) -> String {
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() >= 2 {
        format!("{}-{}", parts[0], parts[1]).to_lowercase()
    } else {
        name.to_lowercase()
    }
}

fn extract_version_suffix(name: &str) -> Option<String> {
    name.rsplit('-').next().and_then(|tail| {
        if tail.contains('.') && tail.chars().all(|c| c.is_ascii_digit() || c == '.') {
            Some(tail.to_lowercase())
        } else {
            None
        }
    })
}

fn is_metadata_only_plugin_file_name(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        ".ds_store"
            | "manifest.json"
            | "icon.png"
            | "readme"
            | "readme.md"
            | "readme.txt"
            | "changelog"
            | "changelog.md"
            | "changelog.txt"
            | "license"
            | "license.md"
            | "license.txt"
    )
}

fn path_has_plugin_payload(path: &Path, depth: usize) -> bool {
    if depth == 0 || !path.exists() {
        return false;
    }

    if path.is_file() {
        return path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|name| !is_metadata_only_plugin_file_name(name))
            .unwrap_or(false);
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry in entries.filter_map(|entry| entry.ok()) {
        let child_path = entry.path();
        if child_path.is_file() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !is_metadata_only_plugin_file_name(&file_name) {
                return true;
            }
        } else if child_path.is_dir() && path_has_plugin_payload(&child_path, depth - 1) {
            return true;
        }
    }

    false
}

fn normalize_mod_match_value(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn derive_mod_match_terms(value: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let stem = Path::new(value)
        .file_stem()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| value.to_string());

    let parts = value.split(|c: char| !c.is_ascii_alphanumeric());
    for part in parts {
        let norm = normalize_mod_match_value(part);
        if norm.len() >= 3 {
            terms.push(norm);
        }
    }

    let parts_stem = stem.split(|c: char| !c.is_ascii_alphanumeric()).collect::<Vec<_>>();
    if parts_stem.len() >= 2 {
        terms.push(normalize_mod_match_value(&format!(
            "{}{}",
            parts_stem[0], parts_stem[1]
        )));
    }

    terms.push(normalize_mod_match_value(&stem));
    terms.retain(|term| term.len() >= 3);
    terms.sort();
    terms.dedup();
    terms
}

fn entry_name_matches_mod_payload(
    entry_name: &str,
    folder_name: &str,
    mod_key: &str,
) -> bool {
    let entry_norm = normalize_mod_match_value(entry_name);
    if entry_norm.is_empty() {
        return false;
    }

    let mut terms = derive_mod_match_terms(folder_name);
    terms.extend(derive_mod_match_terms(mod_key));
    terms.sort();
    terms.dedup();

    terms
        .iter()
        .any(|term| entry_norm.contains(term) || term.contains(&entry_norm))
}

fn game_mod_folder_has_payload(
    plugins_root: &Path,
    folder_path: &Path,
    folder_name: &str,
    mod_key: &str,
) -> bool {
    if folder_path.join("manifest.json").exists() {
        return true;
    }

    if path_has_plugin_payload(folder_path, 6) {
        return true;
    }

    let entries = match fs::read_dir(plugins_root) {
        Ok(entries) => entries,
        Err(_) => return false,
    };

    for entry in entries.filter_map(|entry| entry.ok()) {
        let sibling_path = entry.path();
        if sibling_path == folder_path {
            continue;
        }

        let sibling_name = entry.file_name().to_string_lossy().to_string();
        if !entry_name_matches_mod_payload(&sibling_name, folder_name, mod_key) {
            continue;
        }

        if path_has_plugin_payload(&sibling_path, 6) {
            return true;
        }
    }

    false
}

fn read_manifest_version(dir: &Path) -> Option<String> {
    let manifest_path = dir.join("manifest.json");
    let data = fs::read_to_string(manifest_path).ok()?;
    let json: Value = serde_json::from_str(&data).ok()?;
    json.get("version_number")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("versionNumber").and_then(|v| v.as_str()))
        .map(|v| v.to_string())
}

fn main() {
    println!("--- DEBUG ALL SYNC START ---");
    let app_data = PathBuf::from("/Users/federicofeduzi/Library/Application Support/com.r2modmac");
    let settings_data = fs::read_to_string(app_data.join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&settings_data).unwrap();
    let game_paths = settings["game_paths"].as_object().unwrap();

    let profiles_data = fs::read_to_string(app_data.join("profiles.json")).unwrap();
    let profiles: Vec<Value> = serde_json::from_str(&profiles_data).unwrap();

    for profile in &profiles {
        let name = profile["name"].as_str().unwrap_or("unnamed");
        let game_id = profile["gameIdentifier"].as_str().unwrap_or("");
        let platform = profile["platform"].as_str().unwrap_or("windows");
        let is_vanilla = profile["is_vanilla"].as_bool().unwrap_or(false);

        // Find path
        let cache_key = format!("{}::{}", game_id, platform);
        let game_path_str = game_paths.get(&cache_key)
            .or_else(|| game_paths.get(game_id))
            .and_then(|v| v.as_str());

        let Some(game_path_str) = game_path_str else {
            println!("Profile '{}' ({}) has no game path configured.", name, game_id);
            continue;
        };

        let game_path = Path::new(game_path_str);
        if !game_path.exists() {
            println!("Profile '{}' ({}) game path does not exist: {}", name, game_id, game_path_str);
            continue;
        }

        println!("\n====================================");
        println!("PROFILE: '{}' (Game: {}, Platform: {}, Vanilla: {})", name, game_id, platform, is_vanilla);
        println!("Game path: {}", game_path_str);

        let game_plugins = if game_path.join("BepInEx_DISABLED").is_dir() {
            game_path.join("BepInEx_DISABLED").join("plugins")
        } else {
            game_path.join("BepInEx").join("plugins")
        };

        let profile_mod_full_names: Vec<String> = profile["mods"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter(|m| m["enabled"].as_bool().unwrap_or(true))
            .filter_map(|m| m["fullName"].as_str().map(|s| s.to_string()))
            .collect();

        let mut desired_key_set = std::collections::HashSet::new();
        let mut desired_full_by_key = std::collections::HashMap::new();
        let mut desired_version_by_key = std::collections::HashMap::new();
        for full_name in &profile_mod_full_names {
            let key = extract_mod_key(full_name);
            desired_key_set.insert(key.clone());
            desired_full_by_key.insert(key.clone(), full_name.to_lowercase());
            if let Some(version) = extract_version_suffix(full_name) {
                desired_version_by_key.insert(key, version);
            }
        }

        let mut game_mod_folders: Vec<(String, String)> = vec![];
        let mut invalid_game_mod_folders: Vec<String> = vec![];
        if game_plugins.exists() {
            if let Ok(entries) = fs::read_dir(&game_plugins) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.path().is_dir() {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        let mod_key = extract_mod_key(&folder_name);
                        let has_payload = game_mod_folder_has_payload(
                            &game_plugins,
                            &entry.path(),
                            &folder_name,
                            &mod_key,
                        );
                        if has_payload {
                            game_mod_folders.push((folder_name, mod_key));
                        } else {
                            invalid_game_mod_folders.push(folder_name);
                        }
                    }
                }
            }
        }

        let profile_id = profile["id"].as_str().unwrap_or("");
        let manifests_dir = app_data.join("profiles").join(profile_id).join(".r2modmac").join("manifests").join("game");
        let mut manifests_to_keep = Vec::new();
        if manifests_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(manifests_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if entry.path().extension().and_then(|ext| ext.to_str()) == Some("json") {
                        if let Ok(data) = fs::read_to_string(entry.path()) {
                            if let Ok(manifest) = serde_json::from_str::<SimpleManifest>(&data) {
                                let desired_full = desired_full_by_key.get(&manifest.mod_key);
                                let keep = match desired_full {
                                    Some(full) => full == &manifest.mod_full_name.to_lowercase(),
                                    None => false,
                                };
                                if keep {
                                    manifests_to_keep.push(manifest);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut to_remove: Vec<String> = invalid_game_mod_folders.clone();
        for (folder_name, gm_key) in &game_mod_folders {
            if !desired_key_set.contains(gm_key) {
                // Check if this folder is owned by any manifest we want to keep
                let folder_prefix_1 = format!("bepinex/plugins/{}/", folder_name.to_lowercase());
                let folder_prefix_2 = format!("bepinex_disabled/plugins/{}/", folder_name.to_lowercase());
                let folder_exact_1 = format!("bepinex/plugins/{}", folder_name.to_lowercase());
                let folder_exact_2 = format!("bepinex_disabled/plugins/{}", folder_name.to_lowercase());
                let is_owned_by_kept_manifest = manifests_to_keep.iter().any(|manifest| {
                    manifest.files.iter().any(|file| {
                        let file_lower = file.to_lowercase();
                        file_lower.starts_with(&folder_prefix_1)
                            || file_lower.starts_with(&folder_prefix_2)
                            || file_lower == folder_exact_1
                            || file_lower == folder_exact_2
                    })
                });
                if is_owned_by_kept_manifest {
                    continue;
                }

                to_remove.push(folder_name.clone());
                continue;
            }

            let desired_version = desired_version_by_key.get(gm_key);
            let desired_full = desired_full_by_key.get(gm_key);
            let game_version = extract_version_suffix(folder_name)
                .or_else(|| read_manifest_version(&game_plugins.join(folder_name)));
            let full_mismatch = desired_full
                .map(|full| folder_name.to_lowercase() != *full)
                .unwrap_or(false);
            let needs_replacement = match desired_version {
                Some(dv) => match game_version.as_ref() {
                    Some(gv) => gv != dv,
                    None => full_mismatch,
                },
                None => false,
            };

            if needs_replacement && full_mismatch {
                to_remove.push(folder_name.clone());
            }
        }

        let bepinex_installed = game_path.join("BepInEx").join("core").exists()
            || game_path.join("BepInEx_DISABLED").join("core").exists()
            || game_path.join("run_bepinex.sh").exists();

        let mut to_install: Vec<String> = desired_key_set
            .iter()
            .filter(|pm_key| {
                if pm_key.contains("bepinex") && bepinex_installed {
                    return false;
                }

                let desired_full = desired_full_by_key
                    .get(*pm_key)
                    .cloned()
                    .unwrap_or_default();
                let desired_version = desired_version_by_key.get(*pm_key);

                let has_exact_version = manifests_to_keep.iter().any(|manifest| {
                    manifest.mod_key == **pm_key
                        && manifest_files_exist(game_path, &manifest.files)
                }) || game_mod_folders.iter().any(|(folder_name, gm_key)| {
                    if gm_key != *pm_key {
                        return false;
                    }

                    if let Some(dv) = desired_version {
                        let game_version = extract_version_suffix(folder_name)
                            .or_else(|| read_manifest_version(&game_plugins.join(folder_name)));
                        if let Some(gv) = game_version {
                            return gv == *dv;
                        }
                        return folder_name.to_lowercase() == desired_full;
                    }

                    true
                });

                !has_exact_version
            })
            .map(|k| k.to_string())
            .collect();
        to_install.sort();

        println!("TO REMOVE: {:?}", to_remove);
        println!("TO INSTALL: {:?}", to_install);
    }
}

fn manifest_files_exist(target_root: &Path, files: &[String]) -> bool {
    if files.is_empty() {
        return false;
    }
    for file in files {
        let path = target_root.join(file);
        if !path.exists() {
            return false;
        }
    }
    true
}
