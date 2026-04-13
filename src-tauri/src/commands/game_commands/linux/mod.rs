use super::*;

#[cfg(target_os = "linux")]
mod wine;

#[cfg(target_os = "linux")]
pub(crate) use self::wine::*;

#[allow(dead_code)]
pub(crate) fn find_linux_executable_path(
    game_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    if game_path.is_file() {
        return Some(game_path.to_path_buf());
    }

    let entries = fs::read_dir(game_path).ok()?;
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .metadata()
                    .map(|metadata| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            metadata.permissions().mode() & 0o111 != 0
                        }

                        #[cfg(not(unix))]
                        {
                            false
                        }
                    })
                    .unwrap_or(false)
        })
}
