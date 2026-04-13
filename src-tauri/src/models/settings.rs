use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub steam_path: Option<String>,
    #[serde(default)]
    pub favorite_games: Vec<String>,
    #[serde(default)]
    pub game_paths: HashMap<String, String>,
    #[serde(default)]
    pub legacy_install_mode: bool,
    #[serde(default = "default_true")]
    pub ask_version_before_install: bool,
    #[serde(default = "default_true")]
    pub install_in_parallel: bool,
    #[serde(default)]
    pub confirm_before_apply_to_game: bool,
    #[serde(default = "default_true")]
    pub write_debug_logs_to_game: bool,
    #[serde(default = "default_mod_view_mode")]
    pub default_mod_view_mode: String,
    #[serde(default)]
    pub hide_crossover_guide: bool,
    #[serde(default)]
    pub hide_macos_guide: bool,
}

impl Settings {
    pub fn default() -> Self {
        Self {
            steam_path: None,
            favorite_games: Vec::new(),
            game_paths: HashMap::new(),
            legacy_install_mode: false,
            ask_version_before_install: true,
            install_in_parallel: true,
            confirm_before_apply_to_game: false,
            write_debug_logs_to_game: true,
            default_mod_view_mode: default_mod_view_mode(),
            hide_crossover_guide: false,
            hide_macos_guide: false,
        }
    }
}

fn default_mod_view_mode() -> String {
    "grid".to_string()
}

fn default_true() -> bool {
    true
}
