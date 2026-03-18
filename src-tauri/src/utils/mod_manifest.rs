use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const GAME_MANIFEST_SCOPE: &str = "game";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModOwnershipManifest {
    #[serde(default)]
    pub manifest_version: u32,
    pub mod_full_name: String,
    pub mod_key: String,
    pub files: Vec<String>,
    pub backed_up_files: Vec<String>,
    pub match_terms: Vec<String>,
    #[serde(default)]
    pub target_root_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredModOwnershipManifest {
    pub manifest_path: PathBuf,
    pub backup_dir: PathBuf,
    pub manifest: ModOwnershipManifest,
}

fn mod_key_from_full_name(full_name: &str) -> String {
    let parts: Vec<&str> = full_name.split('-').collect();
    if parts.len() >= 2 {
        format!("{}-{}", parts[0], parts[1]).to_lowercase()
    } else {
        full_name.to_lowercase()
    }
}

fn normalize_term(input: &str) -> String {
    let stem = Path::new(input)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());

    stem.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn extend_terms_from_string(terms: &mut HashSet<String>, value: &str) {
    let normalized = normalize_term(value);
    if normalized.len() >= 3 {
        terms.insert(normalized);
    }

    for part in value.split(|c: char| !c.is_ascii_alphanumeric()) {
        let normalized_part = normalize_term(part);
        if normalized_part.len() >= 3 {
            terms.insert(normalized_part);
        }
    }
}

fn should_skip_path_term(component: &str) -> bool {
    matches!(
        component.to_lowercase().as_str(),
        "bepinex"
            | "plugins"
            | "patchers"
            | "config"
            | "core"
            | "monomod"
            | "assets"
            | "manifest"
            | "readme"
            | "readme.md"
            | "icon"
            | "license"
            | "changelog"
    )
}

fn build_match_terms(mod_full_name: &str, files: &[String]) -> Vec<String> {
    let mut terms = HashSet::new();
    extend_terms_from_string(&mut terms, mod_full_name);

    for file in files {
        let path = Path::new(file);
        for component in path.components() {
            let value = component.as_os_str().to_string_lossy().to_string();
            if value.is_empty() || should_skip_path_term(&value) {
                continue;
            }
            extend_terms_from_string(&mut terms, &value);
        }
    }

    let mut sorted = terms.into_iter().collect::<Vec<_>>();
    sorted.sort();
    sorted
}

fn manifest_slug(mod_full_name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    mod_full_name.hash(&mut hasher);
    let hash = hasher.finish();
    let mut slug = mod_full_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    slug.truncate(80);
    format!("{}_{}", slug.trim_matches('_'), hash)
}

fn manifest_scope_dir(app: &AppHandle, profile_id: &str, scope: &str) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id)
        .join(".r2modmac")
        .join("manifests")
        .join(scope))
}

fn manifest_file_path(app: &AppHandle, profile_id: &str, scope: &str, mod_full_name: &str) -> Result<PathBuf, String> {
    Ok(manifest_scope_dir(app, profile_id, scope)?.join(format!("{}.json", manifest_slug(mod_full_name))))
}

fn backup_dir_path(app: &AppHandle, profile_id: &str, scope: &str, mod_full_name: &str) -> Result<PathBuf, String> {
    Ok(manifest_scope_dir(app, profile_id, scope)?.join(format!("{}_backup", manifest_slug(mod_full_name))))
}

fn normalize_relative_string(path: &Path) -> Option<String> {
    let parts = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn canonicalize_target_root_string(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn prune_empty_parent_dirs(path: &Path, stop_root: &Path) {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == stop_root || !dir.starts_with(stop_root) {
            break;
        }

        let is_empty = fs::read_dir(dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            break;
        }

        let _ = fs::remove_dir(dir);
        current = dir.parent();
    }
}

fn entry_matches_terms(name: &str, terms: &HashSet<String>) -> bool {
    let normalized = normalize_term(name);
    if normalized.is_empty() {
        return false;
    }

    terms
        .iter()
        .any(|term| normalized.contains(term) || term.contains(&normalized))
}

fn remove_path_if_present(path: &Path) -> std::io::Result<()> {
    if path.is_symlink() || path.is_file() {
        fs::remove_file(path)
    } else if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        Ok(())
    }
}

pub fn save_owned_mod_manifest(
    app: &AppHandle,
    profile_id: &str,
    scope: &str,
    mod_full_name: &str,
    target_root: &Path,
    relative_files: &[PathBuf],
    backed_up_files: &[String],
) -> Result<(), String> {
    let manifest_path = manifest_file_path(app, profile_id, scope, mod_full_name)?;
    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| "Invalid manifest parent directory".to_string())?;
    fs::create_dir_all(manifest_dir).map_err(|e| e.to_string())?;

    let mut file_strings = relative_files
        .iter()
        .filter_map(|path| normalize_relative_string(path))
        .collect::<Vec<_>>();
    file_strings.sort();
    file_strings.dedup();

    let mut backup_strings = backed_up_files.to_vec();
    backup_strings.sort();
    backup_strings.dedup();

    let manifest = ModOwnershipManifest {
        manifest_version: 1,
        mod_full_name: mod_full_name.to_string(),
        mod_key: mod_key_from_full_name(mod_full_name),
        match_terms: build_match_terms(mod_full_name, &file_strings),
        files: file_strings,
        backed_up_files: backup_strings,
        target_root_hint: Some(canonicalize_target_root_string(target_root)),
    };

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(&manifest_path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn backup_existing_mod_files(
    app: &AppHandle,
    profile_id: &str,
    scope: &str,
    mod_full_name: &str,
    target_root: &Path,
    relative_files: &[PathBuf],
) -> Result<Vec<String>, String> {
    let backup_dir = backup_dir_path(app, profile_id, scope, mod_full_name)?;
    if backup_dir.exists() {
        let _ = fs::remove_dir_all(&backup_dir);
    }

    let mut backed_up = Vec::new();
    for relative_path in relative_files {
        let Some(relative_string) = normalize_relative_string(relative_path) else {
            continue;
        };
        let target_path = target_root.join(relative_path);
        if !target_path.is_file() {
            continue;
        }

        let backup_path = backup_dir.join(relative_path);
        if let Some(parent) = backup_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&target_path, &backup_path)
            .map_err(|e| format!("Failed to back up {}: {}", target_path.display(), e))?;
        backed_up.push(relative_string);
    }

    Ok(backed_up)
}

pub fn load_owned_mod_manifests(
    app: &AppHandle,
    profile_id: &str,
    scope: &str,
) -> Result<Vec<StoredModOwnershipManifest>, String> {
    let manifests_dir = manifest_scope_dir(app, profile_id, scope)?;
    if !manifests_dir.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    let entries = fs::read_dir(&manifests_dir).map_err(|e| e.to_string())?;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let data = match fs::read_to_string(&path) {
            Ok(data) => data,
            Err(_) => continue,
        };
        let manifest = match serde_json::from_str::<ModOwnershipManifest>(&data) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };
        let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or_default();
        manifests.push(StoredModOwnershipManifest {
            backup_dir: manifests_dir.join(format!("{}_backup", stem)),
            manifest_path: path,
            manifest,
        });
    }

    Ok(manifests)
}

pub fn manifest_matches_target_root(manifest: &ModOwnershipManifest, target_root: &Path) -> bool {
    match manifest.target_root_hint.as_deref() {
        None => manifest.files.iter().any(|relative| {
            let candidate = target_root.join(relative);
            candidate.exists() || candidate.is_symlink()
        }),
        Some(hint) => {
            let target = canonicalize_target_root_string(target_root);
            hint == target
        }
    }
}

fn derive_mod_scoped_root(relative: &str) -> Option<String> {
    let parts = relative
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if parts.len() < 3 {
        return None;
    }

    match (parts[0].to_lowercase(), parts[1].to_lowercase()) {
        (a, b) if a == "bepinex" && (b == "plugins" || b == "patchers" || b == "translation") => {
            Some(format!("{}/{}/{}", parts[0], parts[1], parts[2]))
        }
        (a, b) if (a == "plugins" || a == "patchers" || a == "translation") && b.len() > 1 => {
            Some(format!("{}/{}", parts[0], parts[1]))
        }
        _ => None,
    }
}

fn best_effort_remove_generated_entries(
    target_root: &Path,
    manifests_to_remove: &[StoredModOwnershipManifest],
    manifests_to_keep: &[StoredModOwnershipManifest],
) -> Result<usize, String> {
    let removed_terms = manifests_to_remove
        .iter()
        .flat_map(|entry| entry.manifest.match_terms.iter().cloned())
        .collect::<HashSet<_>>();
    let kept_terms = manifests_to_keep
        .iter()
        .flat_map(|entry| entry.manifest.match_terms.iter().cloned())
        .collect::<HashSet<_>>();

    if removed_terms.is_empty() {
        return Ok(0);
    }

    let known_roots = [
        target_root.join("BepInEx").join("config"),
        target_root.join("BepInEx").join("cache"),
        target_root.join("BepInEx").join("plugins"),
        target_root.join("BepInEx").join("patchers"),
        target_root.join("BepInEx").join("monomod"),
        target_root.join("BepInEx").join("Translation"),
        target_root.join("config"),
        target_root.join("cache"),
        target_root.join("plugins"),
        target_root.join("patchers"),
        target_root.join("monomod"),
        target_root.join("Translation"),
    ];

    let mut removed = 0usize;
    for root in known_roots {
        if !root.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !entry_matches_terms(&name, &removed_terms) || entry_matches_terms(&name, &kept_terms) {
                continue;
            }

            remove_path_if_present(&path).map_err(|e| {
                format!(
                    "Failed to remove generated mod artifact {}: {}",
                    path.display(),
                    e
                )
            })?;
            prune_empty_parent_dirs(&path, target_root);
            removed += 1;
        }
    }

    Ok(removed)
}

pub fn cleanup_owned_mod_manifests(
    target_root: &Path,
    manifests_to_remove: &[StoredModOwnershipManifest],
    manifests_to_keep: &[StoredModOwnershipManifest],
) -> Result<usize, String> {
    let kept_paths = manifests_to_keep
        .iter()
        .flat_map(|entry| entry.manifest.files.iter().cloned())
        .collect::<HashSet<_>>();

    for entry in manifests_to_remove {
        let backup_paths = entry
            .manifest
            .backed_up_files
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        for relative in &entry.manifest.files {
            if kept_paths.contains(relative) {
                continue;
            }

            let target_path = target_root.join(relative);
            if backup_paths.contains(relative) {
                let backup_path = entry.backup_dir.join(relative);
                if backup_path.is_file() {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    fs::copy(&backup_path, &target_path).map_err(|e| {
                        format!(
                            "Failed to restore backup {} -> {}: {}",
                            backup_path.display(),
                            target_path.display(),
                            e
                        )
                    })?;
                    continue;
                }
            }

            if target_path.exists() || target_path.is_symlink() {
                remove_path_if_present(&target_path).map_err(|e| {
                    format!("Failed to remove managed mod file {}: {}", target_path.display(), e)
                })?;
            }
            prune_empty_parent_dirs(&target_path, target_root);
        }

        let scoped_roots = entry
            .manifest
            .files
            .iter()
            .filter_map(|relative| derive_mod_scoped_root(relative))
            .collect::<HashSet<_>>();
        for scoped_root in scoped_roots {
            let scoped_prefix = format!("{}/", scoped_root);
            if kept_paths
                .iter()
                .any(|path| path == &scoped_root || path.starts_with(&scoped_prefix))
            {
                continue;
            }

            let scoped_path = target_root.join(&scoped_root);
            if scoped_path.exists() || scoped_path.is_symlink() {
                remove_path_if_present(&scoped_path).map_err(|e| {
                    format!(
                        "Failed to remove managed mod directory {}: {}",
                        scoped_path.display(),
                        e
                    )
                })?;
                prune_empty_parent_dirs(&scoped_path, target_root);
            }
        }
    }

    let _ = best_effort_remove_generated_entries(target_root, manifests_to_remove, manifests_to_keep)?;

    let mut removed_count = 0usize;
    for entry in manifests_to_remove {
        if entry.manifest_path.exists() {
            let _ = fs::remove_file(&entry.manifest_path);
        }
        if entry.backup_dir.exists() {
            let _ = fs::remove_dir_all(&entry.backup_dir);
        }
        removed_count += 1;
    }

    Ok(removed_count)
}
