use super::*;

pub(crate) fn copy_macos_bepinex_runtime_root(
    source_root: &std::path::Path,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let root_dirs = ["BepInEx", "doorstop_libs"];
    for item in root_dirs {
        let src = source_root.join(item);
        let dst = game_path.join(item);
        if !src.exists() {
            continue;
        }
        if dst.exists() {
            let _ = fs::remove_dir_all(&dst);
        }
        copy_dir_recursive(&src, &dst).map_err(|e| format!("Failed to copy {}: {}", item, e))?;
    }

    let root_files = [
        "doorstop_config.ini",
        "libdoorstop.dylib",
        CANONICAL_MAC_BEPINEX_SCRIPT,
    ];
    for item in root_files {
        let src = source_root.join(item);
        let dst = game_path.join(item);
        if !src.exists() {
            continue;
        }
        if dst.exists() {
            let _ = fs::remove_file(&dst);
        }
        fs::copy(&src, &dst).map_err(|e| format!("Failed to copy {}: {}", item, e))?;
        if item == "doorstop_config.ini" {
            normalize_macos_doorstop_config_file(&dst)?;
            configure_macos_doorstop_target_assembly(&dst, game_path)?;
        }
    }

    Ok(())
}

pub(crate) fn configure_macos_doorstop_target_assembly(
    config_path: &std::path::Path,
    game_path: &std::path::Path,
) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }

    let preloader_path = game_path
        .join("BepInEx")
        .join("core")
        .join("BepInEx.Preloader.dll")
        .to_string_lossy()
        .replace('\\', "/");
    let core_path = game_path
        .join("BepInEx")
        .join("core")
        .to_string_lossy()
        .replace('\\', "/");
    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let target_line = format!("targetAssembly={}", preloader_path);
    let dll_search_path_line = format!("dllSearchPathOverride={}", core_path);

    let mut updated = if let Ok(target_re) = regex::Regex::new(r"(?m)^targetAssembly=.*$") {
        target_re
            .replace(&content, target_line.as_str())
            .into_owned()
    } else {
        content.clone()
    };

    if let Ok(dll_search_re) = regex::Regex::new(r"(?m)^dllSearchPathOverride=.*$") {
        updated = dll_search_re
            .replace(&updated, dll_search_path_line.as_str())
            .into_owned();
    }

    if updated != content {
        fs::write(config_path, updated).map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub(crate) async fn ensure_macos_bepinex_runtime_present(
    app: &AppHandle,
    profile_id: &str,
    game_path: &std::path::Path,
) -> Result<(), String> {
    let runtime_root = resolve_macos_runtime_root(game_path);

    if has_complete_macos_bepinex_runtime(&runtime_root) {
        normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
        configure_macos_doorstop_target_assembly(
            &runtime_root.join("doorstop_config.ini"),
            &runtime_root,
        )?;
        return Ok(());
    }

    let profile_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id);

    if has_complete_macos_bepinex_runtime(&profile_dir) {
        normalize_macos_doorstop_config_file(&profile_dir.join("doorstop_config.ini"))?;
        copy_macos_bepinex_runtime_root(&profile_dir, &runtime_root)?;
        dequarantine_recursive(&runtime_root);
        if has_complete_macos_bepinex_runtime(&runtime_root) {
            return Ok(());
        }
    }

    let profiles_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles.json");
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or_else(|| "Profile not found while restoring macOS BepInEx runtime".to_string())?;

    let bepinex_full_name = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|m| m["enabled"].as_bool().unwrap_or(true))
        .filter_map(|m| m["fullName"].as_str())
        .find(|full_name| full_name.to_lowercase().contains("bepinexpack"))
        .map(|s| s.to_string());

    let Some(bepinex_full_name) = bepinex_full_name else {
        return Ok(());
    };

    let version_number = extract_version_number_from_full_name(&bepinex_full_name)
        .ok_or_else(|| format!("Could not parse BepInEx version from {}", bepinex_full_name))?;
    let runtime_kind = detect_unity_runtime_kind(&runtime_root);
    let runtime_bytes =
        download_official_macos_bepinex_runtime(&version_number, runtime_kind).await?;

    fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut game_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    extract_bepinex_pack_to_root(&mut game_archive, &runtime_root, true, false)?;
    normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
    configure_macos_doorstop_target_assembly(
        &runtime_root.join("doorstop_config.ini"),
        &runtime_root,
    )?;

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut profile_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    extract_bepinex_pack_to_root(&mut profile_archive, &profile_dir, true, false)?;
    normalize_macos_doorstop_config_file(&profile_dir.join("doorstop_config.ini"))?;

    dequarantine_recursive(&runtime_root);

    if has_complete_macos_bepinex_runtime(&runtime_root) {
        Ok(())
    } else {
        Err("No macOS BepInEx startup script found".to_string())
    }
}
