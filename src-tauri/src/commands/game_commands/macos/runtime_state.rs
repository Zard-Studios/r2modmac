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

/// Rename the given directories and files to their `_DISABLED` twin, or back.
fn rename_runtime_items(
    root: &std::path::Path,
    disable: bool,
    dir_items: &[&str],
    file_items: &[&str],
) -> Result<(), String> {
    for item in dir_items {
        let active = root.join(item);
        let disabled = root.join(format!("{}_DISABLED", item));
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
        let active = root.join(item);
        let disabled = root.join(format!("{}_DISABLED", item));
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
    Ok(())
}

/// The same toggle with the BepInEx tree somewhere other than the game.
///
/// An isolated profile keeps the tree, while the loader that boots it stays
/// beside the game, so a vanilla run has to rename one of each in its own
/// place. Renaming only the loader would leave the tree under a name the
/// install and the scan do not expect.
pub(crate) fn sync_macos_runtime_disabled_state_rooted(
    game_path: &std::path::Path,
    disable: bool,
    tree_root: Option<&std::path::Path>,
) -> Result<(), String> {
    let runtime_root = resolve_macos_runtime_root(game_path);
    if let Some(tree_root) = tree_root {
        if tree_root != runtime_root {
            rename_runtime_items(tree_root, disable, &["BepInEx"], &[])?;
            return rename_runtime_items(
                &runtime_root,
                disable,
                &["doorstop_libs"],
                &[
                    "doorstop_config.ini",
                    "libdoorstop.dylib",
                    ".doorstop_version",
                ],
            );
        }
    }
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

#[cfg(test)]
mod isolated_vanilla_toggle_tests {
    use super::*;

    fn world(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-vanilla-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn isolated_world(label: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let root = world(label);
        let game = root.join("game");
        let profile = root.join("profiles/abc");
        std::fs::create_dir_all(game.join("doorstop_libs")).unwrap();
        std::fs::write(game.join("libdoorstop.dylib"), b"loader").unwrap();
        std::fs::create_dir_all(profile.join("BepInEx/plugins/Author-Mod")).unwrap();
        (root, game, profile)
    }

    #[test]
    fn going_vanilla_disables_the_tree_in_the_profile_and_the_loader_in_the_game() {
        let (root, game, profile) = isolated_world("disable");

        sync_macos_runtime_disabled_state_rooted(&game, true, Some(&profile)).unwrap();

        assert!(profile.join("BepInEx_DISABLED/plugins/Author-Mod").is_dir());
        assert!(!profile.join("BepInEx").exists());
        assert!(game.join("doorstop_libs_DISABLED").is_dir());
        assert!(game.join("libdoorstop.dylib_DISABLED").exists());
        // The mods are still on disk, only renamed.
        assert!(profile.join("BepInEx_DISABLED/plugins/Author-Mod").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn coming_back_from_vanilla_restores_both_sides() {
        let (root, game, profile) = isolated_world("enable");

        sync_macos_runtime_disabled_state_rooted(&game, true, Some(&profile)).unwrap();
        sync_macos_runtime_disabled_state_rooted(&game, false, Some(&profile)).unwrap();

        assert!(profile.join("BepInEx/plugins/Author-Mod").is_dir());
        assert!(!profile.join("BepInEx_DISABLED").exists());
        assert!(game.join("doorstop_libs").is_dir());
        assert!(game.join("libdoorstop.dylib").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_game_tree_is_never_touched_under_isolation() {
        let (root, game, profile) = isolated_world("untouched");
        // A leftover tree in the game, from before isolation.
        std::fs::create_dir_all(game.join("BepInEx/plugins/Old-Mod")).unwrap();

        sync_macos_runtime_disabled_state_rooted(&game, true, Some(&profile)).unwrap();

        assert!(game.join("BepInEx/plugins/Old-Mod").is_dir());
        assert!(!game.join("BepInEx_DISABLED").exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn without_isolation_the_toggle_behaves_as_it_always_did() {
        let root = world("classic");
        let game = root.join("game");
        std::fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        std::fs::create_dir_all(game.join("doorstop_libs")).unwrap();

        sync_macos_runtime_disabled_state_rooted(&game, true, None).unwrap();
        assert!(game.join("BepInEx_DISABLED").is_dir());
        assert!(game.join("doorstop_libs_DISABLED").is_dir());

        sync_macos_runtime_disabled_state_rooted(&game, false, None).unwrap();
        assert!(game.join("BepInEx").is_dir());
        assert!(game.join("doorstop_libs").is_dir());

        std::fs::remove_dir_all(root).unwrap();
    }
}
