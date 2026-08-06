use super::*;

const MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX: (u32, u32, u32, u32) = (5, 4, 23, 5);

fn parse_macos_bepinex5_runtime_version(
    runtime_root: &std::path::Path,
) -> Option<(u32, u32, u32, u32)> {
    let bepinex_core = runtime_root
        .join("BepInEx")
        .join("core")
        .join("BepInEx.dll");
    let bytes = fs::read(bepinex_core).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let version_re = regex::Regex::new(r"\b5\.(\d+)\.(\d+)(?:\.(\d+))?\b").ok()?;

    let mut best: Option<(u32, u32, u32, u32)> = None;
    for captures in version_re.captures_iter(&text) {
        let minor = captures.get(1)?.as_str().parse::<u32>().ok()?;
        let patch_major = captures.get(2)?.as_str().parse::<u32>().ok()?;
        let patch_minor = captures
            .get(3)
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        let candidate = (5, minor, patch_major, patch_minor);
        if best.map(|current| candidate > current).unwrap_or(true) {
            best = Some(candidate);
        }
    }

    best
}

fn macos_bepinex_runtime_requires_unity6_log_writer_fix(runtime_root: &std::path::Path) -> bool {
    if detect_unity_runtime_kind(runtime_root) != "mono" {
        return false;
    }

    let Some(version) = parse_macos_bepinex5_runtime_version(runtime_root) else {
        return false;
    };

    version.0 == 5 && version < MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX
}

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
    let profiles_path = crate::utils::paths::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("profiles.json");
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;
    let profile = profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or_else(|| "Profile not found while restoring macOS BepInEx runtime".to_string())?;

    // Keep explicit game/community BepInEx packs pinned. Older Intel-only games
    // can require pack-specific runtime layouts that break if we auto-refresh to
    // latest generic official runtime.
    let bepinex_full_name = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|m| m["enabled"].as_bool().unwrap_or(true))
        .filter_map(|m| m["fullName"].as_str())
        .find(|full_name| full_name.to_lowercase().contains("bepinexpack"))
        .map(|s| s.to_string());
    let has_explicit_bepinex_pack = bepinex_full_name.is_some();

    let runtime_root = resolve_macos_runtime_root(game_path);
    let runtime_has_complete = has_complete_macos_bepinex_runtime(&runtime_root);
    let runtime_requires_fix = runtime_has_complete
        && !has_explicit_bepinex_pack
        && macos_bepinex_runtime_requires_unity6_log_writer_fix(&runtime_root);

    if runtime_has_complete && !runtime_requires_fix {
        normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
        configure_macos_doorstop_target_assembly(
            &runtime_root.join("doorstop_config.ini"),
            &runtime_root,
        )?;
        return Ok(());
    } else if runtime_requires_fix {
        log::debug!(
            "[ensure_macos_bepinex_runtime_present] Existing macOS BepInEx runtime at {} is below {}.{}.{}.{}; attempting in-place refresh to include Unity 6 log-writer fix.",
            runtime_root.display(),
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.0,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.1,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.2,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.3
        );
    }

    let profile_dir = crate::utils::paths::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id);
    let profile_has_complete = has_complete_macos_bepinex_runtime(&profile_dir);
    let profile_requires_fix = profile_has_complete
        && !has_explicit_bepinex_pack
        && macos_bepinex_runtime_requires_unity6_log_writer_fix(&profile_dir);

    if profile_has_complete && !profile_requires_fix {
        normalize_macos_doorstop_config_file(&profile_dir.join("doorstop_config.ini"))?;
        copy_macos_bepinex_runtime_root(&profile_dir, &runtime_root)?;
        dequarantine_recursive(&runtime_root);
        if has_complete_macos_bepinex_runtime(&runtime_root) {
            return Ok(());
        }
    } else if profile_requires_fix {
        log::debug!(
            "[ensure_macos_bepinex_runtime_present] Profile runtime at {} is below {}.{}.{}.{}; skipping direct copy and refreshing from official runtime source.",
            profile_dir.display(),
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.0,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.1,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.2,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.3
        );
    }

    let version_number = if let Some(bepinex_full_name) = bepinex_full_name {
        extract_version_number_from_full_name(&bepinex_full_name)
            .unwrap_or_else(|| {
                log::warn!(
                    "[ensure_macos_bepinex_runtime_present] Could not parse BepInEx version from {}; leaving current runtime untouched.",
                    bepinex_full_name
                );
                "".to_string()
            })
    } else if runtime_requires_fix {
        log::debug!(
            "[ensure_macos_bepinex_runtime_present] No explicit BepInExPack entry found in profile mods; forcing runtime refresh with minimum version {}.{}.{}.{}.",
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.0,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.1,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.2,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.3
        );
        format!(
            "{}.{}.{}.{}",
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.0,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.1,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.2,
            MIN_BEPINEX5_UNITY6_LOG_WRITER_FIX.3
        )
    } else {
        return Ok(());
    };
    if version_number.is_empty() {
        return Ok(());
    }
    let runtime_kind = detect_unity_runtime_kind(&runtime_root);
    let runtime_bytes = match download_official_macos_bepinex_runtime(&version_number, runtime_kind)
        .await
    {
        Ok(bytes) => bytes,
        Err(error) if runtime_has_complete => {
            log::warn!(
                    "[ensure_macos_bepinex_runtime_present] Runtime refresh failed ({}), but an existing runtime is present. Continuing with existing runtime.",
                    error
                );
            normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
            configure_macos_doorstop_target_assembly(
                &runtime_root.join("doorstop_config.ini"),
                &runtime_root,
            )?;
            return Ok(());
        }
        Err(error) => return Err(error),
    };

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
