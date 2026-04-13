use super::*;

pub(crate) fn rename_path_if_present(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> Result<(), String> {
    if !src.exists() && !src.is_symlink() {
        return Ok(());
    }

    if dst.exists() || dst.is_symlink() {
        if dst.is_dir() && !dst.is_symlink() {
            let _ = fs::remove_dir_all(dst);
        } else {
            let _ = fs::remove_file(dst);
        }
    }

    fs::rename(src, dst).map_err(|e| {
        format!(
            "Failed to move {} -> {}: {}",
            src.display(),
            dst.display(),
            e
        )
    })
}

pub(crate) fn sync_macos_runtime_disabled_state(
    game_path: &std::path::Path,
    disable: bool,
) -> Result<(), String> {
    let runtime_root = resolve_macos_runtime_root(game_path);
    let dir_items = ["BepInEx", "doorstop_libs"];
    let file_items = [
        "doorstop_config.ini",
        "libdoorstop.dylib",
        ".doorstop_version",
    ];

    if disable {
        let stale_bepinex = runtime_root.join("BepInEx");
        if stale_bepinex.is_dir() {
            let mut can_remove = true;
            if let Ok(entries) = fs::read_dir(&stale_bepinex) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name != "r2modmac_bootstrap.log" && name != ".DS_Store" {
                        can_remove = false;
                        break;
                    }
                }
            } else {
                can_remove = false;
            }

            if can_remove {
                let _ = fs::remove_dir_all(&stale_bepinex);
            }
        }
    }

    for item in dir_items {
        let active = runtime_root.join(item);
        let disabled = runtime_root.join(format!("{}_DISABLED", item));
        if disable {
            if active.is_dir() && disabled.is_dir() {
                let _ = fs::remove_dir_all(&active);
                continue;
            }
            rename_path_if_present(&active, &disabled)?;
        } else if disabled.exists() {
            if !active.exists() {
                rename_path_if_present(&disabled, &active)?;
            } else if disabled.is_dir() && !disabled.is_symlink() {
                let _ = fs::remove_dir_all(&disabled);
            } else {
                let _ = fs::remove_file(&disabled);
            }
        }
    }

    for item in file_items {
        let active = runtime_root.join(item);
        let disabled = runtime_root.join(format!("{}_DISABLED", item));
        if disable {
            if (active.exists() || active.is_symlink())
                && (disabled.exists() || disabled.is_symlink())
            {
                let _ = fs::remove_file(&active);
                continue;
            }
            rename_path_if_present(&active, &disabled)?;
        } else if disabled.exists() {
            if !active.exists() {
                rename_path_if_present(&disabled, &active)?;
            } else {
                let _ = fs::remove_file(&disabled);
            }
        }
    }

    let active_script = runtime_root.join(CANONICAL_MAC_BEPINEX_SCRIPT);
    let disabled_script = runtime_root.join(format!("{}_DISABLED", CANONICAL_MAC_BEPINEX_SCRIPT));
    if !active_script.exists() && disabled_script.exists() {
        rename_path_if_present(&disabled_script, &active_script)?;
    } else if active_script.exists() && disabled_script.exists() {
        if disabled_script.is_dir() && !disabled_script.is_symlink() {
            let _ = fs::remove_dir_all(&disabled_script);
        } else {
            let _ = fs::remove_file(&disabled_script);
        }
    }

    if let Ok(entries) = fs::read_dir(&runtime_root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if disable {
                if path.is_file()
                    && is_bepinex_shell_script_name(&name)
                    && name != CANONICAL_MAC_BEPINEX_SCRIPT
                {
                    let disabled = runtime_root.join(format!("{}_DISABLED", name));
                    rename_path_if_present(&path, &disabled)?;
                }
            } else if name.ends_with("_DISABLED") {
                let active_name = name.trim_end_matches("_DISABLED").to_string();
                if is_bepinex_shell_script_name(&active_name) {
                    let active_path = runtime_root.join(&active_name);
                    if !active_path.exists() {
                        rename_path_if_present(&path, &active_path)?;
                    } else if path.is_dir() && !path.is_symlink() {
                        let _ = fs::remove_dir_all(&path);
                    } else {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    Ok(())
}
