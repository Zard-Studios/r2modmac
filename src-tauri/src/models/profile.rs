
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mod {
    pub name: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mods: Vec<Mod>,
    #[serde(default)]
    pub is_vanilla: bool,
}
