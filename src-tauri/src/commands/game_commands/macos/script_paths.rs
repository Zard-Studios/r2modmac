use super::*;

pub(crate) fn resolve_macos_executable_path(
    game_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    find_macos_executable_path(game_path).ok_or_else(|| {
        "No macOS executable found (supported locations include nested .app bundles such as Contents/game/*.app).".to_string()
    })
}

pub(crate) fn resolve_macos_launch_entry_path(
    game_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    find_macos_wrapper_launcher_path(game_path)
        .or_else(|| find_macos_executable_path(game_path))
        .ok_or_else(|| {
            "No macOS launch entry found (supported locations include wrapper launchers such as Contents/MacOS/load and nested .app bundles such as Contents/game/*.app).".to_string()
        })
}

pub(crate) fn has_macos_doorstop_support(script: &str) -> bool {
    let lower = script.to_lowercase();
    lower.contains("dyld_insert_libraries")
        && lower.contains("dylib")
        && (lower.contains("doorstop_enable") || lower.contains("doorstop_enabled"))
}
