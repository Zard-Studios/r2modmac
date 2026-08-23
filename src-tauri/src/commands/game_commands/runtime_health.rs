use super::*;
use serde::Serialize;
use tauri::command;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    runtime: String,
    status: String,
    missing_components: Vec<String>,
    repairable: bool,
}

fn profile_is_vanilla(app: &AppHandle, profile_id: &str) -> bool {
    let Ok(data_dir) = crate::utils::paths::app_data_dir(app) else {
        return false;
    };
    let Ok(data) = fs::read_to_string(data_dir.join("profiles.json")) else {
        return false;
    };
    serde_json::from_str::<Vec<serde_json::Value>>(&data)
        .ok()
        .and_then(|profiles| {
            profiles
                .into_iter()
                .find(|profile| profile["id"].as_str() == Some(profile_id))
        })
        .and_then(|profile| profile["is_vanilla"].as_bool())
        .unwrap_or(false)
}

fn health(runtime: &str, missing_components: Vec<String>) -> RuntimeHealth {
    let status = if missing_components.is_empty() {
        "healthy"
    } else if missing_components.len() == 1 && missing_components[0] == "runtime" {
        "missing"
    } else {
        "incomplete"
    };
    // The frontend blocks Apply/Launch and forces a repair on anything other
    // than "healthy", so always record the verdict that drove that decision.
    log::debug!(
        "[runtime_health] Verdict: runtime={} status={} missing={:?}",
        runtime,
        status,
        missing_components
    );
    RuntimeHealth {
        runtime: runtime.to_string(),
        status: status.to_string(),
        repairable: !missing_components.is_empty(),
        missing_components,
    }
}

fn core_directory_has_payload(core_dir: &std::path::Path) -> bool {
    fs::read_dir(core_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            entry.path().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "dll" | "so" | "dylib"
                        )
                    })
        })
}

fn inspect_owml(game_path: &std::path::Path, vanilla: bool) -> RuntimeHealth {
    let active = game_path.join("OWML");
    let disabled = game_path.join("OWML_DISABLED");
    let runtime_root = if active.exists() {
        active
    } else if vanilla && disabled.exists() {
        disabled
    } else {
        return health("owml", vec!["runtime".to_string()]);
    };
    let mut missing = Vec::new();
    if !runtime_root.join("OWML.Launcher.exe").is_file() {
        missing.push("launcher".to_string());
    }
    if !runtime_root.join("OWML.dll").is_file() && !runtime_root.join("OWML.Common.dll").is_file() {
        missing.push("core".to_string());
    }
    health("owml", missing)
}

fn inspect_lovely(game_path: &std::path::Path) -> RuntimeHealth {
    let mut missing = Vec::new();
    if !game_path.join(BALATRO_LOVELY_SCRIPT).is_file() {
        missing.push("launch-script".to_string());
    }
    if !game_path.join("liblovely.dylib").is_file() {
        missing.push("loader-library".to_string());
    }
    if missing.len() == 2 {
        return health("lovely", vec!["runtime".to_string()]);
    }
    health("lovely", missing)
}

/// ReturnOfModding is present when its proxy DLL sits next to the game exe.
///
/// The proxy's name is per-pack, not per-loader: ReturnOfModding ships
/// `version.dll` while Hell2Modding (Hades II) ships `d3d12.dll`, so the check
/// accepts any of the pack proxy names rather than the one Risk of Rain Returns
/// happens to use. A vanilla profile keeps the loader renamed to `*_DISABLED`,
/// which still counts as installed.
fn inspect_return_of_modding(game_path: &std::path::Path, vanilla: bool) -> RuntimeHealth {
    let proxies = crate::models::loaders::return_of_modding_proxies(game_path);
    let installed = proxies.iter().any(|proxy| {
        let disabled = proxy
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_DISABLED"));
        !disabled || vanilla
    });
    if installed {
        health("returnofmodding", Vec::new())
    } else {
        log::debug!(
            "[runtime_health] No ReturnOfModding proxy in {:?} (looked for {:?}); folder contains: {:?}",
            game_path,
            crate::models::loaders::RETURN_OF_MODDING_PROXY_NAMES,
            list_directory_entries(game_path, 40)
        );
        health("returnofmodding", vec!["runtime".to_string()])
    }
}

/// Shimloader is installed into the profile, not into the game folder.
///
/// `dwmapi.dll` and `ue4ss.dll` live in the profile root and only reach
/// `<game>/<data folder>/Binaries/Win64` when the profile is applied, so the
/// profile is where "is the loader installed" has to be answered: a game folder
/// without them may simply be a profile that has not been applied yet.
fn inspect_shimloader(profile_dir: &std::path::Path) -> RuntimeHealth {
    let present = |name: &str| profile_dir.join(name).is_file();
    if !present("dwmapi.dll") && !present("ue4ss.dll") {
        return health("shimloader", vec!["runtime".to_string()]);
    }
    let mut missing = Vec::new();
    if !present("dwmapi.dll") {
        missing.push("shim".to_string());
    }
    if !present("ue4ss.dll") {
        missing.push("ue4ss".to_string());
    }
    if !present("UE4SS-settings.ini") {
        missing.push("settings".to_string());
    }
    health("shimloader", missing)
}

/// BepInEx 5 ships the preloader as `BepInEx.Preloader.dll`, while BepInEx 6
/// (the IL2CPP/Unity.Mono line used by packs such as BepInExPack_GTFO) ships
/// `BepInEx.Preloader.Core.dll` plus a runtime-specific entry point instead.
pub(crate) fn core_directory_has_preloader(core_dir: &std::path::Path) -> bool {
    const PRELOADERS: [&str; 4] = [
        "BepInEx.Preloader.dll",
        "BepInEx.Preloader.Core.dll",
        "BepInEx.Unity.IL2CPP.dll",
        "BepInEx.Unity.Mono.Preloader.dll",
    ];
    if let Some(found) = PRELOADERS.iter().find(|name| core_dir.join(name).is_file()) {
        log::debug!(
            "[runtime_health] Preloader found: {} in {:?}",
            found,
            core_dir
        );
        return true;
    }
    // No preloader is the single most common cause of a stuck "incomplete"
    // runtime, so record what the folder actually holds - a pack shipping an
    // unknown layout is indistinguishable from a genuinely empty core here.
    log::debug!(
        "[runtime_health] No preloader in {:?} (looked for {:?}); core contains: {:?}",
        core_dir,
        PRELOADERS,
        list_directory_entries(core_dir, 40)
    );
    false
}

/// Directory listing for diagnostics, capped so a `core` folder full of DLLs
/// cannot flood the log.
fn list_directory_entries(dir: &std::path::Path, limit: usize) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![format!("<unreadable or missing: {}>", dir.display())];
    };
    let mut names = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    names.sort();
    let total = names.len();
    names.truncate(limit);
    if total > limit {
        names.push(format!("... and {} more", total - limit));
    }
    names
}

fn inspect_windows_bepinex(game_path: &std::path::Path, vanilla: bool) -> RuntimeHealth {
    let disabled = vanilla && game_path.join("BepInEx_DISABLED").exists();
    let bep_dir = game_path.join(if disabled {
        "BepInEx_DISABLED"
    } else {
        "BepInEx"
    });
    if !bep_dir.exists() {
        return health("bepinex", vec!["runtime".to_string()]);
    }
    let mut missing = Vec::new();
    if !bep_dir.join("core").is_dir() {
        missing.push("core".to_string());
    }
    if !core_directory_has_preloader(&bep_dir.join("core")) {
        missing.push("preloader".to_string());
    }
    let winhttp = if disabled {
        "winhttp.dll_DISABLED"
    } else {
        "winhttp.dll"
    };
    if !game_path.join(winhttp).is_file() {
        missing.push("doorstop".to_string());
    }
    health("bepinex", missing)
}

#[command]
pub async fn check_profile_runtime_health(
    app: AppHandle,
    profile_id: String,
    game_identifier: String,
    platform: Option<String>,
) -> Result<RuntimeHealth, String> {
    let profile_platform = platform.unwrap_or_else(|| get_profile_platform(&app, &profile_id));
    // Even the answers that stop short of looking at the game folder name the
    // right loader, so the sidebar does not tell a Hades II user that BepInEx
    // is what they are missing.
    let declared_runtime = if is_outerwilds_identifier(&game_identifier) {
        "owml".to_string()
    } else {
        crate::models::loaders::loader_for_community(&game_identifier)
            .unwrap_or(crate::models::loaders::PackageLoader::BepInEx)
            .runtime_name()
            .to_string()
    };
    log::debug!(
        "[runtime_health] Checking profile={} game={} platform={}",
        profile_id,
        game_identifier,
        profile_platform
    );
    if profile_platform != "mac" && profile_platform != "windows" {
        log::debug!(
            "[runtime_health] Platform {} is not runtime-managed; reporting unsupported",
            profile_platform
        );
        return Ok(RuntimeHealth {
            runtime: declared_runtime,
            status: "unsupported".to_string(),
            missing_components: Vec::new(),
            repairable: false,
        });
    }
    let Some(game_path_string) = get_game_path(
        app.clone(),
        game_identifier.clone(),
        Some(profile_platform.clone()),
    )
    .await?
    else {
        log::debug!(
            "[runtime_health] No game path configured for {} ({}); reporting unconfigured",
            game_identifier,
            profile_platform
        );
        return Ok(RuntimeHealth {
            runtime: declared_runtime,
            status: "unconfigured".to_string(),
            missing_components: Vec::new(),
            repairable: false,
        });
    };
    let game_path = std::path::Path::new(&game_path_string);

    let vanilla = profile_is_vanilla(&app, &profile_id);
    log::debug!(
        "[runtime_health] Resolved game path {:?} (vanilla_profile={})",
        game_path,
        vanilla
    );
    // Which loader the game needs is a property of the community, not of this
    // app's list of special cases: Hades II runs on ReturnOfModding through
    // Hell2Modding, and reporting a missing BepInEx for it sent the user into a
    // repair that had no package to install (issue #38).
    let loader = crate::models::loaders::resolve_loader(&game_identifier, game_path);
    log::debug!(
        "[runtime_health] Loader for {}: {}",
        game_identifier,
        loader.runtime_name()
    );

    match &loader {
        crate::models::loaders::PackageLoader::Owml => return Ok(inspect_owml(game_path, vanilla)),
        crate::models::loaders::PackageLoader::ReturnOfModding => {
            return Ok(inspect_return_of_modding(game_path, vanilla))
        }
        crate::models::loaders::PackageLoader::Shimloader => {
            let profile_dir = crate::utils::paths::app_data_dir(&app)
                .map_err(|error| error.to_string())?
                .join("profiles")
                .join(&profile_id);
            return Ok(inspect_shimloader(&profile_dir));
        }
        crate::models::loaders::PackageLoader::Lovely => {
            if profile_platform == "mac" {
                return Ok(inspect_lovely(game_path));
            }
        }
        crate::models::loaders::PackageLoader::Unsupported(slug) => {
            // Saying "unsupported" is the honest answer: r2modmac installs
            // BepInEx, ReturnOfModding, Lovely, OWML and shimloader, and
            // nothing it could download would make this game run on one of
            // them.
            log::info!(
                "[runtime_health] {} uses the {} loader, which r2modmac cannot install",
                game_identifier,
                slug
            );
            return Ok(RuntimeHealth {
                runtime: slug.clone(),
                status: "unsupported".to_string(),
                missing_components: Vec::new(),
                repairable: false,
            });
        }
        crate::models::loaders::PackageLoader::BepInEx => {}
    }

    if profile_platform == "mac" {
        let runtime_root = resolve_macos_runtime_root(game_path);
        // Under isolation the tree sits in the profile while the loader stays
        // beside the game, so the two are looked for separately.
        let tree_root = bepinex_install_root(&app, &profile_id, &runtime_root)?;
        let disabled = vanilla && tree_root.join("BepInEx_DISABLED").exists();
        let bep_dir = tree_root.join(if disabled {
            "BepInEx_DISABLED"
        } else {
            "BepInEx"
        });
        let complete = if vanilla {
            has_complete_disabled_macos_bepinex_runtime(game_path)
                || has_complete_macos_bepinex_runtime_rooted(game_path, Some(&tree_root))
        } else {
            has_complete_macos_bepinex_runtime_rooted(game_path, Some(&tree_root))
        } && core_directory_has_payload(&bep_dir.join("core"));
        if complete {
            return Ok(health("bepinex", Vec::new()));
        }
        if !bep_dir.exists() {
            return Ok(health("bepinex", vec!["runtime".to_string()]));
        }
        let mut missing = Vec::new();
        if !core_directory_has_payload(&bep_dir.join("core")) {
            missing.push("core".to_string());
        }
        let doorstop = runtime_root.join(if disabled {
            "doorstop_libs_DISABLED"
        } else {
            "doorstop_libs"
        });
        let dylib = runtime_root.join(if disabled {
            "libdoorstop.dylib_DISABLED"
        } else {
            "libdoorstop.dylib"
        });
        if !doorstop.is_dir() && !dylib.is_file() {
            missing.push("doorstop".to_string());
        }
        if find_bepinex_script_in_dir(&runtime_root).is_none() {
            missing.push("launch-script".to_string());
        }
        return Ok(health("bepinex", missing));
    }

    Ok(inspect_windows_bepinex(game_path, vanilla))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "r2modmac-runtime-health-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn windows_runtime_reports_missing_and_incomplete_components() {
        let root = test_dir("windows-incomplete");
        fs::create_dir_all(root.join("BepInEx/core")).unwrap();
        let result = inspect_windows_bepinex(&root, false);
        assert_eq!(result.status, "incomplete");
        assert_eq!(result.missing_components, vec!["preloader", "doorstop"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_bepinex6_il2cpp_runtime_is_healthy() {
        // BepInExPack_GTFO ships BepInEx 6: no BepInEx.Preloader.dll exists.
        let root = test_dir("windows-bepinex6");
        fs::create_dir_all(root.join("BepInEx/core")).unwrap();
        fs::write(root.join("BepInEx/core/BepInEx.Preloader.Core.dll"), b"").unwrap();
        fs::write(root.join("BepInEx/core/BepInEx.Unity.IL2CPP.dll"), b"").unwrap();
        fs::write(root.join("winhttp.dll"), b"").unwrap();
        let result = inspect_windows_bepinex(&root, false);
        assert_eq!(result.status, "healthy");
        assert!(result.missing_components.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_windows_runtime_is_healthy_only_for_vanilla_profile() {
        let root = test_dir("windows-disabled");
        fs::create_dir_all(root.join("BepInEx_DISABLED/core")).unwrap();
        fs::write(
            root.join("BepInEx_DISABLED/core/BepInEx.Preloader.dll"),
            b"",
        )
        .unwrap();
        fs::write(root.join("winhttp.dll_DISABLED"), b"").unwrap();
        assert_eq!(inspect_windows_bepinex(&root, true).status, "healthy");
        assert_eq!(inspect_windows_bepinex(&root, false).status, "missing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn return_of_modding_accepts_active_and_intentionally_disabled_loader() {
        let root = test_dir("return-of-modding");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("version.dll"), b"loader").unwrap();
        assert_eq!(inspect_return_of_modding(&root, false).status, "healthy");
        fs::rename(root.join("version.dll"), root.join("version.dll_DISABLED")).unwrap();
        assert_eq!(inspect_return_of_modding(&root, true).status, "healthy");
        assert_eq!(inspect_return_of_modding(&root, false).status, "missing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn return_of_modding_accepts_a_pack_that_ships_another_proxy_name() {
        // Hades II is served by Hell2Modding, whose pack installs d3d12.dll.
        // Looking only for version.dll reported the runtime as missing and sent
        // the user into a BepInEx repair that had nothing to install (#38).
        let root = test_dir("hell2modding");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(inspect_return_of_modding(&root, false).status, "missing");
        fs::write(root.join("d3d12.dll"), b"loader").unwrap();
        assert_eq!(inspect_return_of_modding(&root, false).status, "healthy");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn owml_requires_launcher_and_core() {
        let root = test_dir("owml");
        fs::create_dir_all(root.join("OWML")).unwrap();
        fs::write(root.join("OWML/OWML.Launcher.exe"), b"").unwrap();
        let result = inspect_owml(&root, false);
        assert_eq!(result.status, "incomplete");
        assert_eq!(result.missing_components, vec!["core"]);
        fs::remove_dir_all(root).unwrap();
    }
}
