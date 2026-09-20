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

fn configured_doorstop_search_override(content: &str) -> Option<&str> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with(';')
            || trimmed.starts_with('[')
        {
            return None;
        }
        let (key, value) = trimmed.split_once('=')?;
        if key.trim().eq_ignore_ascii_case("dllSearchPathOverride")
            || key.trim().eq_ignore_ascii_case("dll_search_path_override")
        {
            Some(value.trim())
        } else {
            None
        }
    })
}

/// Only the ordinary BepInEx core search path follows an isolated runtime.
/// A different value belongs to the game pack (usually an unstripped corlib
/// directory beside the game) and must remain relative to the game executable.
fn doorstop_search_override_follows_bepinex_tree(content: &str) -> bool {
    configured_doorstop_search_override(content).map_or(true, |value| {
        let normalized = value.trim().replace('\\', "/").to_ascii_lowercase();
        normalized.is_empty()
            || normalized == "core"
            || normalized == "bepinex/core"
            || normalized.ends_with("/bepinex/core")
    })
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

/// `tree_root` is wherever the BepInEx folder lives — the game for a shared
/// install, the profile for an isolated one — which is not always where the
/// loader and this config sit.
pub(crate) fn configure_macos_doorstop_target_assembly(
    config_path: &std::path::Path,
    tree_root: &std::path::Path,
) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }

    // BepInEx 5 boots through BepInEx.Preloader.dll, but the BepInEx 6 packs we
    // can also install on macOS (see download_official_macos_bepinex6_pack) use
    // a runtime-specific entry point instead. Pointing Doorstop at a DLL that is
    // not on disk makes the game start unmodded with no visible error, so pick
    // the entry point that actually shipped and only fall back to the BepInEx 5
    // name when nothing is installed yet.
    let core_dir = tree_root.join("BepInEx").join("core");
    let preloader_name = [
        "BepInEx.Unity.IL2CPP.dll",
        "BepInEx.Unity.Mono.Preloader.dll",
        "BepInEx.Preloader.dll",
    ]
    .into_iter()
    .find(|name| core_dir.join(name).is_file())
    .unwrap_or("BepInEx.Preloader.dll");
    log::debug!(
        "[macos_doorstop] targetAssembly entry point for {:?}: {}",
        core_dir,
        preloader_name
    );
    let preloader_path = core_dir
        .join(preloader_name)
        .to_string_lossy()
        .replace('\\', "/");
    let core_path = tree_root
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

    if doorstop_search_override_follows_bepinex_tree(&content) {
        if let Ok(dll_search_re) =
            regex::Regex::new(r"(?mi)^(dllSearchPathOverride|dll_search_path_override)=.*$")
        {
            if dll_search_re.is_match(&updated) {
                updated = dll_search_re
                    .replace(&updated, dll_search_path_line.as_str())
                    .into_owned();
            } else {
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(&dll_search_path_line);
                updated.push('\n');
            }
        }
    } else if let Some(search_override) = configured_doorstop_search_override(&content) {
        log::info!(
            "[macos_doorstop] Preserving game-specific Mono search override while repairing runtime: {}",
            search_override
        );
    }

    if updated != content {
        fs::write(config_path, updated).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn isolated_macos_bepinex_runtime_is_complete(
    runtime_root: &std::path::Path,
    tree_root: &std::path::Path,
) -> bool {
    tree_root != runtime_root
        && has_complete_macos_bepinex_runtime_rooted(runtime_root, Some(tree_root))
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

    // An isolated profile already keeps a complete tree of its own, and copying
    // it beside the game would put back the shared installation this is meant
    // to avoid. Only the loader config next to the game needs adjusting, so it
    // points at the tree rather than at a folder that is no longer there.
    let tree_root =
        crate::commands::game_commands::bepinex_install_root(app, profile_id, &runtime_root)?;
    if isolated_macos_bepinex_runtime_is_complete(&runtime_root, &tree_root) {
        normalize_macos_doorstop_config_file(&runtime_root.join("doorstop_config.ini"))?;
        configure_macos_doorstop_target_assembly(
            &runtime_root.join("doorstop_config.ini"),
            &tree_root,
        )?;
        return Ok(());
    }

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

#[cfg(test)]
mod isolated_runtime_layout_tests {
    use super::{
        configure_macos_doorstop_target_assembly, isolated_macos_bepinex_runtime_is_complete,
    };

    fn world() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-isolated-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn loader_beside_game_and_core_in_profile_is_complete() {
        let root = world();
        let game = root.join("game");
        let profile = root.join("profile");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx/core")).unwrap();
        std::fs::write(game.join("libdoorstop.dylib"), b"loader").unwrap();
        std::fs::write(game.join("run_bepinex.sh"), b"#!/bin/sh\n").unwrap();
        std::fs::write(
            profile.join("BepInEx/core/BepInEx.Preloader.dll"),
            b"tailored runtime",
        )
        .unwrap();

        assert!(isolated_macos_bepinex_runtime_is_complete(&game, &profile));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn isolated_tree_without_game_loader_is_incomplete() {
        let root = world();
        let game = root.join("game");
        let profile = root.join("profile");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx/core")).unwrap();
        std::fs::write(
            profile.join("BepInEx/core/BepInEx.Preloader.dll"),
            b"tailored runtime",
        )
        .unwrap();

        assert!(!isolated_macos_bepinex_runtime_is_complete(&game, &profile));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_repair_preserves_a_pack_specific_corlib_override() {
        let root = world();
        let game = root.join("game");
        let profile = root.join("profile");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx/core")).unwrap();
        std::fs::create_dir_all(game.join("2020.3.34")).unwrap();
        std::fs::write(
            profile.join("BepInEx/core/BepInEx.Preloader.dll"),
            b"preloader",
        )
        .unwrap();
        let config = game.join("doorstop_config.ini");
        std::fs::write(
            &config,
            "[UnityDoorstop]\nenabled=true\ntargetAssembly=BepInEx\\core\\BepInEx.Preloader.dll\ndllSearchPathOverride=2020.3.34\n",
        )
        .unwrap();

        configure_macos_doorstop_target_assembly(&config, &profile).unwrap();

        let written = std::fs::read_to_string(&config).unwrap();
        assert!(written.contains("dllSearchPathOverride=2020.3.34"));
        assert!(written.contains(&format!(
            "targetAssembly={}/BepInEx/core/BepInEx.Preloader.dll",
            profile.display()
        )));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_repair_retargets_only_the_standard_core_override() {
        let root = world();
        let game = root.join("game");
        let profile = root.join("profile");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(profile.join("BepInEx/core")).unwrap();
        std::fs::write(
            profile.join("BepInEx/core/BepInEx.Preloader.dll"),
            b"preloader",
        )
        .unwrap();
        let config = game.join("doorstop_config.ini");
        std::fs::write(
            &config,
            "[UnityDoorstop]\nenabled=true\ntargetAssembly=BepInEx\\core\\BepInEx.Preloader.dll\ndllSearchPathOverride=BepInEx\\core\n",
        )
        .unwrap();

        configure_macos_doorstop_target_assembly(&config, &profile).unwrap();

        let written = std::fs::read_to_string(&config).unwrap();
        assert!(written.contains(&format!(
            "dllSearchPathOverride={}/BepInEx/core",
            profile.display()
        )));
        std::fs::remove_dir_all(root).unwrap();
    }
}
