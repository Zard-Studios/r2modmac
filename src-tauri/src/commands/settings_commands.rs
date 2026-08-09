use crate::models::shared::*;
use tauri::{command, AppHandle};

const DEFAULT_PREFERENCES_ACCELERATOR: &str = "Mod+,";

pub(crate) fn preferences_menu_accelerator(settings: &Settings) -> Option<String> {
    let accelerator = settings
        .keybinds
        .get("open-preferences")
        .map(String::as_str)
        .unwrap_or(DEFAULT_PREFERENCES_ACCELERATOR);

    if accelerator.is_empty() {
        None
    } else {
        Some(
            accelerator
                .split('+')
                .map(|part| {
                    if part == "Mod" {
                        "CommandOrControl"
                    } else {
                        part
                    }
                })
                .collect::<Vec<_>>()
                .join("+"),
        )
    }
}

pub(crate) fn sync_preferences_menu_accelerator(
    app: &AppHandle,
    settings: &Settings,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::menu::MenuItemKind;

        let accelerator = preferences_menu_accelerator(settings);
        if let Some(menu) = app.menu() {
            for item in menu.items().map_err(|error| error.to_string())? {
                if let MenuItemKind::Submenu(submenu) = item {
                    if let Some(MenuItemKind::MenuItem(preferences)) = submenu.get("preferences") {
                        preferences
                            .set_accelerator(accelerator.as_deref())
                            .map_err(|error| error.to_string())?;
                        break;
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = (app, settings);

    Ok(())
}

#[command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(load_settings_impl(&app))
}

#[command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    save_settings_impl(&app, &settings)?;
    sync_preferences_menu_accelerator(&app, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_accelerator_uses_default_when_no_override_exists() {
        assert_eq!(
            preferences_menu_accelerator(&Settings::default()).as_deref(),
            Some("CommandOrControl+,")
        );
    }

    #[test]
    fn preferences_accelerator_converts_mod_and_can_be_disabled() {
        let mut settings = Settings::default();
        settings
            .keybinds
            .insert("open-preferences".into(), "Mod+Shift+P".into());
        assert_eq!(
            preferences_menu_accelerator(&settings).as_deref(),
            Some("CommandOrControl+Shift+P")
        );

        settings
            .keybinds
            .insert("open-preferences".into(), String::new());
        assert_eq!(preferences_menu_accelerator(&settings), None);
    }
}
