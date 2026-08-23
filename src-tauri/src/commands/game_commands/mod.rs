use std::{
    collections::{HashSet, VecDeque},
    fs,
    sync::{Mutex, OnceLock},
};

use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter};

use crate::commands::mod_commands::{
    detect_unity_runtime_kind, download_official_macos_bepinex_runtime,
    extract_bepinex_pack_to_root, extract_version_number_from_full_name,
    normalize_macos_doorstop_config_file,
};
use crate::models::shared::*;
use crate::utils::file_ops::*;
use crate::utils::mod_manifest::{
    cleanup_owned_mod_manifests, load_owned_mod_manifests, manifest_matches_target_root,
    GAME_MANIFEST_SCOPE, PROFILE_MANIFEST_SCOPE,
};

const CANONICAL_MAC_BEPINEX_SCRIPT: &str = "run_bepinex.sh";
const BALATRO_LOVELY_SCRIPT: &str = "run_lovely_macos.sh";
const STEAM_LAUNCH_OPTIONS_RESTART_EVENT: &str = "steam-launch-options-restart";
const MACOS_LAUNCH_OBSERVE_TIMEOUT_MS: u64 = 60_000;
/// A Steam that has to boot and sign in before it can act on the request needs
/// far longer than a warm one; the same split the Windows launch path uses.
const MACOS_COLD_STEAM_LAUNCH_OBSERVE_TIMEOUT_MS: u64 = 180_000;

static LOGGED_GAME_PATH_OVERRIDES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(crate) mod apply_transaction;
pub(crate) mod install;
pub(crate) mod launch;
pub(crate) mod launch_cancel;
mod linux;
mod macos;
pub(crate) mod owml_patcher;
pub(crate) mod paths;
mod process;
pub(crate) mod runtime_health;
mod shared;
mod steam;
mod steam_state;
pub(crate) mod sync;
pub(crate) mod sync_state;
mod windows;
mod windows_support;

#[allow(unused_imports)]
pub(super) use self::linux::*;
#[allow(unused_imports)]
pub(super) use self::macos::*;
#[allow(unused_imports)]
pub(super) use self::process::*;
#[allow(unused_imports)]
pub(super) use self::shared::*;
#[allow(unused_imports)]
pub(super) use self::steam::*;
pub(crate) use self::windows::process::map_native_path_to_wine_path;
#[allow(unused_imports)]
pub(super) use self::windows::*;
#[allow(unused_imports)]
pub(super) use self::windows_support::*;

pub use self::apply_transaction::{
    begin_profile_apply_transaction, commit_profile_apply_transaction,
    rollback_profile_apply_transaction,
};
pub use self::install::install_to_game;
pub use self::launch::{is_game_running, launch_game_vanilla, launch_game_with_mods, stop_game};
pub use self::launch_cancel::cancel_game_launch;
pub(crate) use self::macos::ensure_macos_steam_launch_options;
pub use self::paths::{
    find_game_executable, get_game_path, get_game_source, open_game_folder, set_game_path,
};
pub use self::runtime_health::check_profile_runtime_health;
pub use self::shared::{restore_mscorlib_vanilla, restore_outerwilds_vanilla};
pub use self::sync::sync_profile_to_game;
pub use self::sync_state::inspect_profile_sync_state;
