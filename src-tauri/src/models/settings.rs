
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
}

impl Settings {
    pub fn default() -> Self {
        Self {
            steam_path: None,
            favorite_games: Vec::new(),
            game_paths: HashMap::new(),
            legacy_install_mode: false,
        }
    }
}
