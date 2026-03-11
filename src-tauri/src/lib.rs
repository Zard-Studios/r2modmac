pub mod commands;
pub mod models;
pub mod utils;

use tauri::{Emitter, Manager};

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

            if let Ok(new_data_dir) = app.path().app_data_dir() {
                if let Some(parent_dir) = new_data_dir.parent() {
                    let old_data_dir = parent_dir.join("com.r2modmac.app");
                    if old_data_dir.exists() && !new_data_dir.exists() {
                        eprintln!(
                            "[startup] MIGRATION: Renaming old data dir {:?} to {:?}",
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

            if let Ok(cache_dir) = app.path().app_cache_dir() {
                let chunks_dir = cache_dir.join("chunks");
                if chunks_dir.exists() {
                    let _ = std::fs::remove_dir_all(&chunks_dir);
                }
            }

            if let Ok(data_dir) = app.path().app_data_dir() {
                let settings = models::shared::load_settings_impl(&app.handle());
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
                                    let _ = std::fs::remove_file(profile_path.join("doorstop_config.ini"));
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
                let bin_dir = app.path().app_data_dir().unwrap().join("bin");
                if !bin_dir.exists() {
                    let _ = fs::create_dir_all(&bin_dir);
                }
                
                let run_sh_path = bin_dir.join("run_bepinex.sh");
                if run_sh_path.exists() {
                    let mut perms = fs::metadata(&run_sh_path).unwrap().permissions();
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(&run_sh_path, perms);
                }
            }

            let current_year = chrono::Local::now().year();
            let copyright_text = format!("Copyright © {} Zard Studios", current_year);

            let report_issue = MenuItemBuilder::with_id("report_issue", "Report an Issue").build(app)?;
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
            commands::system_commands::select_file,
            commands::system_commands::read_image,
            commands::system_commands::check_update,
            commands::system_commands::install_update,
            
            commands::settings_commands::get_settings,
            commands::settings_commands::save_settings,
            
            commands::game_commands::get_game_path,
            commands::game_commands::get_game_source,
            commands::game_commands::set_game_path,
            commands::game_commands::open_game_folder,
            commands::game_commands::find_game_executable,
            commands::game_commands::install_to_game,
            commands::game_commands::launch_game_with_mods,
            commands::game_commands::sync_profile_to_game,
            
            commands::mod_commands::install_mod,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
