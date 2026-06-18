pub mod commands;
pub mod models;
pub mod utils;

// Use mimalloc as the global allocator. It returns freed memory pages back to
// the OS much more aggressively than the default macOS system allocator, which
// significantly reduces RSS after browsing large mod stores.
use mimalloc::MiMalloc;
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use tauri::Emitter;

#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        window.open_devtools();
        Ok(())
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = window;
        Err("DevTools are only available in dev builds.".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(models::shared::AppState::default())
        .setup(|app| {
            use chrono::Datelike;
            use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

            utils::volume_watcher::start_volume_watcher(app.handle().clone());

            // Migrate old directories
            #[cfg(target_os = "windows")]
            {
                if let Ok(new_data_dir) = utils::paths::app_data_dir(app) {
                    if let Some(parent_dir) = new_data_dir.parent() {
                        // Migrate com.r2modmac -> r2modmac.
                        // If r2modmac doesn't exist yet, a simple rename is enough.
                        // If r2modmac already exists (created empty on a previous run before
                        // migration ran), we fall back to a recursive merge so no user data
                        // is left stranded inside com.r2modmac.
                        for old_name in ["com.r2modmac", "com.r2modmac.app"] {
                            let old_dir = parent_dir.join(old_name);
                            if !old_dir.exists() {
                                continue;
                            }
                            if !new_data_dir.exists() {
                                // Simple atomic rename (fast path).
                                eprintln!(
                                    "[startup] Windows MIGRATION: renaming {:?} -> {:?}",
                                    old_dir, new_data_dir
                                );
                                if let Err(e) = std::fs::rename(&old_dir, &new_data_dir) {
                                    eprintln!("[startup] Windows MIGRATION rename failed: {}", e);
                                } else {
                                    eprintln!("[startup] Windows MIGRATION SUCCESS (rename)");
                                }
                            } else {
                                // r2modmac already exists — merge contents from old dir into it.
                                // This happens when the app already ran once and created an empty
                                // r2modmac before the migration had a chance to rename.
                                eprintln!(
                                    "[startup] Windows MIGRATION: {:?} exists, merging {:?} into it",
                                    new_data_dir, old_dir
                                );
                                fn merge_dirs(src: &std::path::Path, dst: &std::path::Path) {
                                    let Ok(entries) = std::fs::read_dir(src) else { return };
                                    let _ = std::fs::create_dir_all(dst);
                                    for entry in entries.filter_map(|e| e.ok()) {
                                        let src_path = entry.path();
                                        let dst_path = dst.join(entry.file_name());
                                        if src_path.is_dir() {
                                            if dst_path.exists() {
                                                // Recurse into existing directory.
                                                merge_dirs(&src_path, &dst_path);
                                            } else {
                                                // Destination subdirectory doesn't exist — rename it.
                                                if let Err(e) = std::fs::rename(&src_path, &dst_path) {
                                                    eprintln!(
                                                        "[startup] MIGRATION: failed to move dir {:?}: {}",
                                                        src_path, e
                                                    );
                                                }
                                            }
                                        } else if !dst_path.exists() {
                                            // Only move files that don't already exist in the destination.
                                            if let Err(e) = std::fs::rename(&src_path, &dst_path) {
                                                eprintln!(
                                                    "[startup] MIGRATION: failed to move file {:?}: {}",
                                                    src_path, e
                                                );
                                            }
                                        }
                                    }
                                }
                                merge_dirs(&old_dir, &new_data_dir);
                                // Remove the old directory if it is now empty.
                                if std::fs::read_dir(&old_dir).map(|mut d| d.next().is_none()).unwrap_or(false) {
                                    let _ = std::fs::remove_dir(&old_dir);
                                    eprintln!("[startup] Windows MIGRATION SUCCESS (merge+cleanup)");
                                } else {
                                    eprintln!(
                                        "[startup] Windows MIGRATION: old dir {:?} not empty after merge, leaving it",
                                        old_dir
                                    );
                                }
                            }
                            // Only migrate the first old directory that exists.
                            break;
                        }
                    }
                }
            }

            #[cfg(target_os = "macos")]
            {
                if let Ok(new_data_dir) = utils::paths::app_data_dir(app) {
                    if let Some(parent_dir) = new_data_dir.parent() {
                        let old_data_dir = parent_dir.join("com.r2modmac.app");
                        if old_data_dir.exists() && !new_data_dir.exists() {
                            eprintln!(
                                "[startup] macOS MIGRATION: Renaming old data dir {:?} to {:?}",
                                old_data_dir, new_data_dir
                            );
                            if let Err(e) = std::fs::rename(&old_data_dir, &new_data_dir) {
                                eprintln!("[startup] MIGRATION FAILED: {}", e);
                            } else {
                                eprintln!("[startup] MIGRATION SUCCESS");
                            }
                        }
                    }
                }
            }

            if let Ok(data_dir) = utils::paths::app_data_dir(app) {
                let mut settings = models::shared::load_settings_impl(&app.handle());

                if !settings.thunderstore_chunk_cache_migrated {
                    if let Ok(cache_dir) = utils::paths::app_cache_dir(app) {
                        let chunks_dir = cache_dir.join("chunks");
                        if chunks_dir.exists() {
                            let _ = std::fs::remove_dir_all(&chunks_dir);
                        }
                    }
                    settings.thunderstore_chunk_cache_migrated = true;
                    let _ = models::shared::save_settings_impl(&app.handle(), &settings);
                }

                if !settings.legacy_install_mode {
                    let profiles_dir = data_dir.join("profiles");
                    if profiles_dir.exists() {
                        if let Ok(entries) = std::fs::read_dir(&profiles_dir) {
                            for entry in entries.filter_map(|e| e.ok()) {
                                let profile_path = entry.path();
                                if profile_path.is_dir() {
                                    let bepinex_path = profile_path.join("BepInEx");
                                    if bepinex_path.exists() {
                                        eprintln!(
                                            "[startup] Cleaning old profile cache: {:?}",
                                            bepinex_path
                                        );
                                        let _ = std::fs::remove_dir_all(&bepinex_path);
                                    }
                                    let _ = std::fs::remove_file(profile_path.join("winhttp.dll"));
                                    let _ = std::fs::remove_file(
                                        profile_path.join("doorstop_config.ini"),
                                    );
                                }
                            }
                        }
                    }
                } else {
                    eprintln!("[startup] Legacy mode ON - keeping profile cache");
                }
            }

            // macOS: Create the bin folder and ensure run_bepinex.sh is executable
            #[cfg(target_os = "macos")]
            {
                use std::fs;
                use std::os::unix::fs::PermissionsExt;
                let bin_dir = utils::paths::app_data_dir(app).unwrap().join("bin");
                if !bin_dir.exists() {
                    let _ = fs::create_dir_all(&bin_dir);
                }

                let run_sh_path = bin_dir.join("run_bepinex.sh");
                if run_sh_path.exists() {
                    let mut perms = fs::metadata(&run_sh_path).unwrap().permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&run_sh_path, perms);
                }

                let current_year = chrono::Local::now().year();
                let copyright_text = format!("Copyright © {} Zard Studios", current_year);

                let report_issue =
                    MenuItemBuilder::with_id("report_issue", "Report an Issue").build(app)?;
                let github = MenuItemBuilder::with_id("github", "GitHub Repository").build(app)?;
                let kofi = MenuItemBuilder::with_id("kofi", "Support my project").build(app)?;

                let help_menu = SubmenuBuilder::new(app, "Help")
                    .item(&report_issue)
                    .item(&github)
                    .separator()
                    .item(&kofi)
                    .build()?;

                let preferences_item = MenuItemBuilder::with_id("preferences", "Preferences...")
                    .accelerator("CommandOrControl+,")
                    .build(app)?;

                let app_menu = SubmenuBuilder::new(app, "r2modmac")
                    .item(&PredefinedMenuItem::about(
                        app,
                        Some("About r2modmac"),
                        Some(
                            tauri::menu::AboutMetadataBuilder::new()
                                .copyright(Some(copyright_text))
                                .authors(Some(vec!["Zard Studios".to_string()]))
                                .build(),
                        ),
                    )?)
                    .separator()
                    .item(&preferences_item)
                    .separator()
                    .item(&PredefinedMenuItem::hide(app, Some("Hide r2modmac"))?)
                    .item(&PredefinedMenuItem::hide_others(app, Some("Hide Others"))?)
                    .item(&PredefinedMenuItem::show_all(app, Some("Show All"))?)
                    .separator()
                    .item(&PredefinedMenuItem::quit(app, Some("Quit r2modmac"))?)
                    .build()?;

                let edit_menu = SubmenuBuilder::new(app, "Edit")
                    .item(&PredefinedMenuItem::undo(app, None)?)
                    .item(&PredefinedMenuItem::redo(app, None)?)
                    .separator()
                    .item(&PredefinedMenuItem::cut(app, None)?)
                    .item(&PredefinedMenuItem::copy(app, None)?)
                    .item(&PredefinedMenuItem::paste(app, None)?)
                    .item(&PredefinedMenuItem::select_all(app, None)?)
                    .build()?;

                let window_menu = SubmenuBuilder::new(app, "Window")
                    .item(&PredefinedMenuItem::minimize(app, None)?)
                    .item(&PredefinedMenuItem::close_window(app, Some("Close"))?)
                    .build()?;

                let menu = MenuBuilder::new(app)
                    .item(&app_menu)
                    .item(&edit_menu)
                    .item(&window_menu)
                    .item(&help_menu)
                    .build()?;

                app.set_menu(menu)?;
            }
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "preferences" => {
                let _ = app.emit("show-preferences", ());
            }
            "report_issue" => {
                let _ = open::that("https://github.com/Zard-Studios/r2modmac/issues/new");
            }
            "github" => {
                let _ = open::that("https://github.com/Zard-Studios/r2modmac");
            }
            "kofi" => {
                let _ = open::that("https://ko-fi.com/zardstudios");
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::profile_commands::get_profiles,
            commands::profile_commands::save_profiles,
            commands::profile_commands::delete_profile_folder,
            commands::profile_commands::open_profile_folder,
            commands::profile_commands::clear_profile_cache,
            commands::profile_commands::toggle_profile_vanilla_mode,
            commands::system_commands::fetch_communities,
            commands::system_commands::fetch_community_images,
            commands::system_commands::fetch_text_content,
            commands::system_commands::resolve_community_platforms,
            commands::system_commands::confirm_dialog,
            commands::system_commands::alert_dialog,
            commands::system_commands::select_folder,
            commands::system_commands::get_username,
            commands::system_commands::select_file,
            commands::system_commands::select_import_path,
            commands::system_commands::read_image,
            commands::system_commands::check_update,
            commands::system_commands::install_update,
            commands::settings_commands::get_settings,
            commands::settings_commands::save_settings,
            commands::game_commands::paths::get_game_path,
            commands::game_commands::paths::get_game_source,
            commands::game_commands::paths::set_game_path,
            commands::game_commands::paths::open_game_folder,
            commands::game_commands::paths::find_game_executable,
            commands::game_commands::install::install_to_game,
            commands::game_commands::launch::is_game_running,
            commands::game_commands::launch::launch_game_with_mods,
            commands::game_commands::launch::launch_game_vanilla,
            commands::game_commands::launch::stop_game,
            commands::game_commands::sync::sync_profile_to_game,
            commands::mod_commands::install_mod,
            commands::mod_commands::inspect_custom_mod,
            commands::mod_commands::cancel_custom_mod_import,
            commands::mod_commands::import_custom_mod,
            commands::mod_commands::refresh_local_mod_metadata,
            commands::mod_commands::import_embedded_custom_mod,
            commands::mod_commands::install_local_mod,
            commands::mod_commands::delete_local_mod_payload,
            commands::mod_commands::open_mod_folder,
            commands::mod_commands::remove_mod,
            commands::mod_commands::toggle_mod,
            commands::mod_commands::copy_mod_from_cache,
            commands::mod_commands::fetch_packages,
            commands::mod_commands::get_available_categories,
            commands::mod_commands::get_packages,
            commands::mod_commands::lookup_packages_by_names,
            commands::mod_commands::fetch_package_by_name,
            commands::export_import::export_profile,
            commands::export_import::share_profile,
            commands::export_import::import_profile,
            commands::export_import::import_profile_from_file,
            commands::profile_commands::list_profile_config_files,
            commands::profile_commands::read_profile_config_file,
            commands::profile_commands::write_profile_config_file,
            commands::profile_commands::reveal_profile_config_file,
            commands::profile_commands::open_profile_config_file,
            open_devtools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
