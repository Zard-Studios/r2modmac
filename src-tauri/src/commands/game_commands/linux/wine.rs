pub(crate) fn find_linux_compat_runner_binary(
    prefix_root: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();

    if let Some(prefix_root) = prefix_root {
        for relative in ["bin/wine", "bin/wine64"] {
            let candidate = prefix_root.join(relative);
            if candidate.exists() && candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in ["wine", "wine64"] {
                let candidate = dir.join(name);
                if candidate.exists() && candidate.is_file() && !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
    }

    candidates.into_iter().next()
}

pub(crate) fn configure_linux_compat_runner_command(
    command: &mut std::process::Command,
    prefix_root: Option<&std::path::Path>,
) -> Result<(), String> {
    if let Some(prefix_root) = prefix_root {
        command.env("WINEPREFIX", prefix_root);
        eprintln!(
            "[compat_runner] Using Wine prefix {:?} with linux runner",
            prefix_root
        );
    }

    Ok(())
}
