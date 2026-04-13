use super::*;

fn push_unique_existing_path(paths: &mut Vec<std::path::PathBuf>, candidate: std::path::PathBuf) {
    if candidate.exists() && candidate.is_file() && !paths.contains(&candidate) {
        paths.push(candidate);
    }
}

fn find_named_file_under(
    root: &std::path::Path,
    names: &[&str],
    max_depth: usize,
) -> Option<std::path::PathBuf> {
    if !root.exists() || !root.is_dir() {
        return None;
    }

    let wanted: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_lowercase();

            if wanted.iter().any(|name| name == &file_name) && path.is_file() {
                return Some(path);
            }

            if depth < max_depth && entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                stack.push((path, depth + 1));
            }
        }
    }

    None
}

fn find_executables_in_path(names: &[&str]) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Some(path_var) = std::env::var_os("PATH") else {
        return found;
    };

    for dir in std::env::split_paths(&path_var) {
        for name in names {
            push_unique_existing_path(&mut found, dir.join(name));
        }
    }

    found
}

fn is_crossover_bottle(prefix_root: &std::path::Path) -> bool {
    prefix_root.join("cxbottle.conf").exists()
        || prefix_root
            .to_string_lossy()
            .to_lowercase()
            .contains("/crossover/bottles/")
}

fn is_crossover_runner(runner_path: &std::path::Path) -> bool {
    runner_path
        .to_string_lossy()
        .to_lowercase()
        .contains("crossover")
}

pub(crate) fn find_macos_compat_runner_binary(
    prefix_root: Option<&std::path::Path>,
    executable_path: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();

    if let Some(prefix_root) = prefix_root {
        for relative in ["bin/wine", "wine/bin/wine", "bin/wine64", "wine/bin/wine64"] {
            push_unique_existing_path(&mut candidates, prefix_root.join(relative));
        }
    }

    if let Some(bundle_path) = find_enclosing_app_bundle(executable_path)
        .or_else(|| prefix_root.and_then(find_enclosing_app_bundle))
    {
        for relative in [
            "Contents/Frameworks/wswine.bundle/bin/wine",
            "Contents/Resources/wine/bin/wine",
            "Contents/SharedSupport/wine/bin/wine",
            "Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine",
            "Contents/Frameworks/wswine.bundle/bin/wine64",
            "Contents/Resources/wine/bin/wine64",
            "Contents/SharedSupport/wine/bin/wine64",
            "Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine64",
        ] {
            push_unique_existing_path(&mut candidates, bundle_path.join(relative));
        }

        if let Some(found) =
            find_named_file_under(&bundle_path.join("Contents"), &["wine", "wine64"], 6)
        {
            push_unique_existing_path(&mut candidates, found);
        }
    }

    if let Some(home) = dirs::home_dir() {
        for app_path in [
            std::path::PathBuf::from("/Applications/CrossOver.app"),
            home.join("Applications").join("CrossOver.app"),
        ] {
            push_unique_existing_path(
                &mut candidates,
                app_path.join("Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine"),
            );
            push_unique_existing_path(
                &mut candidates,
                app_path
                    .join("Contents/SharedSupport/CrossOver/CrossOver-Hosted Application/wine64"),
            );
        }

        for root in [
            home.join("Library/Application Support/heroic/tools"),
            home.join("Library/Application Support/Whisky"),
        ] {
            if let Some(found) = find_named_file_under(&root, &["wine", "wine64"], 6) {
                push_unique_existing_path(&mut candidates, found);
            }
        }

        for app_root in [
            std::path::PathBuf::from("/Applications"),
            home.join("Applications"),
        ] {
            let entries = match fs::read_dir(&app_root) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.filter_map(|entry| entry.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if !path.is_dir() || !name.ends_with(".app") {
                    continue;
                }
                if !["crossover", "wine", "wineskin", "whisky", "heroic"]
                    .iter()
                    .any(|keyword| name.contains(keyword))
                {
                    continue;
                }
                if let Some(found) =
                    find_named_file_under(&path.join("Contents"), &["wine", "wine64"], 6)
                {
                    push_unique_existing_path(&mut candidates, found);
                }
            }
        }
    }

    for path in find_executables_in_path(&["wine", "wine64"]) {
        push_unique_existing_path(&mut candidates, path);
    }

    for candidate in [
        "/opt/homebrew/bin/wine",
        "/usr/local/bin/wine",
        "/opt/homebrew/bin/wine64",
        "/usr/local/bin/wine64",
    ] {
        push_unique_existing_path(&mut candidates, std::path::PathBuf::from(candidate));
    }

    candidates.into_iter().next()
}

pub(crate) fn configure_macos_compat_runner_command(
    command: &mut std::process::Command,
    runner_path: &std::path::Path,
    prefix_root: Option<&std::path::Path>,
) -> Result<(), String> {
    let use_crossover_bottle_mode = prefix_root
        .map(|prefix_root| is_crossover_bottle(prefix_root) && is_crossover_runner(runner_path))
        .unwrap_or(false);

    if use_crossover_bottle_mode {
        let bottle_path = prefix_root
            .ok_or_else(|| "CrossOver bottle path could not be determined.".to_string())?;
        command.arg("--bottle").arg(bottle_path);
        eprintln!(
            "[compat_runner] Using CrossOver bottle {:?} with runner {:?}",
            bottle_path, runner_path
        );
    } else if let Some(prefix_root) = prefix_root {
        command.env("WINEPREFIX", prefix_root);
        eprintln!(
            "[compat_runner] Using Wine prefix {:?} with runner {:?}",
            prefix_root, runner_path
        );
    } else {
        eprintln!(
            "[compat_runner] Using runner {:?} without explicit prefix",
            runner_path
        );
    }

    Ok(())
}
