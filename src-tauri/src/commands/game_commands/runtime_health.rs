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
    RuntimeHealth {
        runtime: runtime.to_string(),
        status: if missing_components.is_empty() {
            "healthy".to_string()
        } else if missing_components.len() == 1 && missing_components[0] == "runtime" {
            "missing".to_string()
        } else {
            "incomplete".to_string()
        },
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

fn inspect_return_of_modding(game_path: &std::path::Path, vanilla: bool) -> RuntimeHealth {
    let loader = if vanilla && game_path.join("version.dll_DISABLED").is_file() {
        game_path.join("version.dll_DISABLED")
    } else {
        game_path.join("version.dll")
    };
    if loader.is_file() {
        health("returnofmodding", Vec::new())
    } else {
        health("returnofmodding", vec!["runtime".to_string()])
    }
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
    if !bep_dir.join("core").join("BepInEx.Preloader.dll").is_file() {
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
    if profile_platform != "mac" && profile_platform != "windows" {
        return Ok(RuntimeHealth {
            runtime: "bepinex".to_string(),
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
        return Ok(RuntimeHealth {
            runtime: "bepinex".to_string(),
            status: "unconfigured".to_string(),
            missing_components: Vec::new(),
            repairable: false,
        });
    };
    let game_path = std::path::Path::new(&game_path_string);

    let vanilla = profile_is_vanilla(&app, &profile_id);
    if is_outerwilds_identifier(&game_identifier) || is_outerwilds_game_path(game_path) {
        return Ok(inspect_owml(game_path, vanilla));
    }

    if is_risk_of_rain_returns_identifier(&game_identifier)
        || is_risk_of_rain_returns_game_path(game_path)
    {
        return Ok(inspect_return_of_modding(game_path, vanilla));
    }

    if profile_platform == "mac"
        && (is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_path))
    {
        return Ok(inspect_lovely(game_path));
    }

    if profile_platform == "mac" {
        let runtime_root = resolve_macos_runtime_root(game_path);
        let disabled = vanilla && runtime_root.join("BepInEx_DISABLED").exists();
        let bep_dir = runtime_root.join(if disabled {
            "BepInEx_DISABLED"
        } else {
            "BepInEx"
        });
        let complete = if vanilla {
            has_complete_disabled_macos_bepinex_runtime(game_path)
                || has_complete_macos_bepinex_runtime(game_path)
        } else {
            has_complete_macos_bepinex_runtime(game_path)
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
