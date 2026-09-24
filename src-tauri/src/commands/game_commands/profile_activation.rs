use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MARKER_FILE: &str = "active-bepinex-profile.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ActiveBepInExProfile {
    pub schema_version: u32,
    pub profile_id: String,
    pub game_identifier: String,
}

pub(super) fn marker_path(game_root: &Path) -> PathBuf {
    game_root.join(".r2modmac").join(MARKER_FILE)
}

pub(super) fn read_active_profile(
    game_root: &Path,
    game_identifier: &str,
) -> Result<Option<String>, String> {
    let path = marker_path(game_root);
    if path.parent().is_some_and(Path::is_symlink) {
        return Err("Unsafe active-profile metadata directory".to_string());
    }
    if !path.exists() {
        return Ok(None);
    }
    if path.is_symlink() || !path.is_file() {
        return Err(format!(
            "Unsafe active-profile marker at {}",
            path.display()
        ));
    }
    let marker: ActiveBepInExProfile =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Invalid active-profile marker: {error}"))?;
    if marker.schema_version != 1 || marker.game_identifier != game_identifier {
        return Err(
            "Active-profile marker does not match this game; Apply was stopped safely".to_string(),
        );
    }
    let id_path = Path::new(&marker.profile_id);
    if marker.profile_id.is_empty()
        || id_path.components().count() != 1
        || !matches!(
            id_path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err("Active-profile marker has an unsafe profile ID".to_string());
    }
    Ok(Some(marker.profile_id))
}

pub(super) fn write_active_profile(
    game_root: &Path,
    game_identifier: &str,
    profile_id: &str,
) -> Result<(), String> {
    let path = marker_path(game_root);
    if path.is_symlink() || path.parent().is_some_and(Path::is_symlink) {
        return Err(format!("Refusing to write through {}", path.display()));
    }
    crate::utils::stable_json::write_file(
        &path,
        &ActiveBepInExProfile {
            schema_version: 1,
            profile_id: profile_id.to_string(),
            game_identifier: game_identifier.to_string(),
        },
    )
}

pub(super) fn package_identity(mod_entry: &serde_json::Value) -> Option<(String, String)> {
    let full_name = mod_entry.get("fullName")?.as_str()?.to_ascii_lowercase();
    let source = mod_entry
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("thunderstore")
        .to_ascii_lowercase();
    let qualifier = if source == "local" {
        mod_entry
            .get("localId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase()
    } else {
        String::new()
    };
    Some((full_name, format!("{source}:{qualifier}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_version_from_different_stores_is_not_same_package() {
        let thunderstore = serde_json::json!({
            "fullName": "author-Shared-1.0.0",
            "source": "thunderstore"
        });
        let hexium = serde_json::json!({
            "fullName": "author-Shared-1.0.0",
            "source": "hexium"
        });
        assert_ne!(package_identity(&thunderstore), package_identity(&hexium));
    }

    #[test]
    fn marker_roundtrip_and_corruption_fail_closed() {
        let root =
            std::env::temp_dir().join(format!("r2modmac-active-profile-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        assert_eq!(read_active_profile(&root, "valheim").unwrap(), None);
        write_active_profile(&root, "valheim", "A").unwrap();
        assert_eq!(
            read_active_profile(&root, "valheim").unwrap(),
            Some("A".into())
        );
        assert!(read_active_profile(&root, "other-game").is_err());
        fs::write(marker_path(&root), b"broken").unwrap();
        assert!(read_active_profile(&root, "valheim").is_err());
        write_active_profile(&root, "valheim", "../other").unwrap();
        assert!(read_active_profile(&root, "valheim").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
