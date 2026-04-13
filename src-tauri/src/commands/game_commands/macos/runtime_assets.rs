use super::*;

pub(crate) fn has_balatro_lovely_runtime(game_path: &std::path::Path) -> bool {
    game_path.join(BALATRO_LOVELY_SCRIPT).exists() && game_path.join("liblovely.dylib").exists()
}

pub(crate) fn balatro_mods_disabled_dir() -> Result<std::path::PathBuf, String> {
    let mods_dir = get_balatro_mods_dir().ok_or_else(|| {
        "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string()
    })?;
    let parent = mods_dir
        .parent()
        .ok_or_else(|| "Could not resolve Balatro application support directory".to_string())?;
    Ok(parent.join("Mods_DISABLED"))
}

pub(crate) fn set_executable_if_present(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("Failed to inspect file permissions: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("Failed to update executable permissions: {}", e))?;
    }

    Ok(())
}

pub(crate) fn read_manifest_version(dir: &std::path::Path) -> Option<String> {
    let manifest_path = dir.join("manifest.json");
    let data = fs::read_to_string(manifest_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;
    json.get("version_number")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("versionNumber").and_then(|v| v.as_str()))
        .map(|v| v.to_string())
}
