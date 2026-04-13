use std::fs;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Instant,
};
use tauri::command;

static DEQUARANTINE_LAST_RUN_MS: OnceLock<Mutex<HashMap<String, u128>>> = OnceLock::new();
const DEQUARANTINE_COOLDOWN_MS: u128 = 10 * 60 * 1000;

pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    let mut files_copied = 0;
    let mut dirs_created = 0;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            if !dest_path.exists() {
                fs::create_dir_all(&dest_path)?;
                dirs_created += 1;
            }
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            // Check if file exists and has same size to skip copy
            let should_copy = if dest_path.exists() {
                if let (Ok(src_meta), Ok(dst_meta)) =
                    (fs::metadata(entry.path()), fs::metadata(&dest_path))
                {
                    // Start with size check - simple and fast
                    if src_meta.len() != dst_meta.len() {
                        true
                    } else {
                        // If size is same, check modification time?
                        // For now, size is a good enough heuristic for mod files (DLLs usually don't change without size change)
                        // And we want speed.
                        // eprintln!("[copy_dir_recursive] Skipping identical file: {:?}", entry.file_name());
                        false
                    }
                } else {
                    true
                }
            } else {
                true
            };

            if should_copy {
                // Remove first if exists to avoid permission issues
                if dest_path.exists() {
                    let _ = fs::remove_file(&dest_path);
                }
                fs::copy(&entry.path(), &dest_path)?;
                files_copied += 1;
            }
        }
    }

    if files_copied > 0 || dirs_created > 0 {
        eprintln!(
            "[copy_dir_recursive] {:?} -> {:?}: {} files, {} dirs",
            src.file_name().unwrap_or_default(),
            dst.file_name().unwrap_or_default(),
            files_copied,
            dirs_created
        );
    }

    Ok(())
}

/// Calculate directory size recursively
pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut size = 0;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                size += calculate_dir_size(&path)?;
            } else {
                size += entry.metadata()?.len();
            }
        }
    }
    Ok(size)
}

pub fn dequarantine_recursive(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        if !path.exists() {
            return;
        }

        let path_key = path.to_string_lossy().to_string();
        let now_ms = Instant::now();
        let now_epoch_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let cache = DEQUARANTINE_LAST_RUN_MS.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut cache_guard) = cache.lock() {
            if let Some(last_ms) = cache_guard.get(&path_key).copied() {
                if now_epoch_ms.saturating_sub(last_ms) < DEQUARANTINE_COOLDOWN_MS {
                    eprintln!(
                        "[dequarantine_recursive] skip (cooldown) path={} elapsed_since_last_ms={}",
                        path.display(),
                        now_epoch_ms.saturating_sub(last_ms)
                    );
                    return;
                }
            }

            let output = std::process::Command::new("/usr/bin/xattr")
                .args(["-r", "-d", "com.apple.quarantine"])
                .arg(path)
                .output();

            let elapsed_ms = now_ms.elapsed().as_millis();
            match output {
                Ok(result) => {
                    eprintln!(
                        "[dequarantine_recursive] path={} status={} elapsed_ms={} stdout_len={} stderr_len={}",
                        path.display(),
                        result.status.success(),
                        elapsed_ms,
                        result.stdout.len(),
                        result.stderr.len()
                    );
                }
                Err(error) => {
                    eprintln!(
                        "[dequarantine_recursive] path={} failed elapsed_ms={} error={}",
                        path.display(),
                        elapsed_ms,
                        error
                    );
                }
            }

            cache_guard.insert(path_key, now_epoch_ms);
            return;
        }

        // Fallback if cache lock fails: still execute once and log timing.
        let output = std::process::Command::new("/usr/bin/xattr")
            .args(["-r", "-d", "com.apple.quarantine"])
            .arg(path)
            .output();
        let elapsed_ms = now_ms.elapsed().as_millis();
        match output {
            Ok(result) => {
                eprintln!(
                    "[dequarantine_recursive] path={} status={} elapsed_ms={} stdout_len={} stderr_len={} cache_lock=false",
                    path.display(),
                    result.status.success(),
                    elapsed_ms,
                    result.stdout.len(),
                    result.stderr.len()
                );
            }
            Err(error) => {
                eprintln!(
                    "[dequarantine_recursive] path={} failed elapsed_ms={} error={} cache_lock=false",
                    path.display(),
                    elapsed_ms,
                    error
                );
            }
        }
    }
}

pub fn migrate_root_plugins_into_bepinex(game_path: &std::path::Path) -> Result<(), String> {
    let migrations = [
        ("plugins", "plugins"),
        ("patchers", "patchers"),
        ("monomod", "monomod"),
    ];

    for (legacy_name, target_name) in migrations {
        let legacy_dir = game_path.join(legacy_name);
        if !legacy_dir.is_dir() {
            continue;
        }

        let target_dir = game_path.join("BepInEx").join(target_name);
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to prepare BepInEx {} directory: {}", target_name, e))?;

        if let Ok(entries) = fs::read_dir(&legacy_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let src = entry.path();
                let dst = target_dir.join(entry.file_name());
                if dst.exists() || dst.is_symlink() {
                    if dst.is_dir() && !dst.is_symlink() {
                        let _ = fs::remove_dir_all(&dst);
                    } else {
                        let _ = fs::remove_file(&dst);
                    }
                }
                fs::rename(&src, &dst).map_err(|e| {
                    format!(
                        "Failed to move legacy {} entry {} -> {}: {}",
                        legacy_name,
                        src.display(),
                        dst.display(),
                        e
                    )
                })?;
            }
        }

        if legacy_dir.exists() {
            let _ = fs::remove_dir_all(&legacy_dir);
        }
    }

    Ok(())
}

pub fn repair_backslash_named_entries(root: &std::path::Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }

    let mut entries = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().contains('\\'))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    entries.sort_by_key(|path| std::cmp::Reverse(path.components().count()));

    for source in entries {
        if !source.exists() {
            continue;
        }

        let relative = source.strip_prefix(root).map_err(|e| {
            format!(
                "Failed to normalize malformed entry {}: {}",
                source.display(),
                e
            )
        })?;

        let mut normalized_relative = std::path::PathBuf::new();
        for component in relative.components() {
            if let std::path::Component::Normal(value) = component {
                for part in value
                    .to_string_lossy()
                    .split('\\')
                    .filter(|part| !part.is_empty())
                {
                    normalized_relative.push(part);
                }
            }
        }

        if normalized_relative == relative {
            continue;
        }

        let target = root.join(&normalized_relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("Failed to create repaired path {}: {}", parent.display(), e)
            })?;
        }

        if source.is_dir() {
            if target.exists() {
                copy_dir_recursive(&source, &target).map_err(|e| {
                    format!(
                        "Failed to merge repaired directory {} -> {}: {}",
                        source.display(),
                        target.display(),
                        e
                    )
                })?;
                fs::remove_dir_all(&source).map_err(|e| {
                    format!(
                        "Failed to remove malformed directory {}: {}",
                        source.display(),
                        e
                    )
                })?;
            } else if let Err(rename_err) = fs::rename(&source, &target) {
                copy_dir_recursive(&source, &target).map_err(|e| {
                    format!(
                        "Failed to repair directory {} -> {} after rename error {}: {}",
                        source.display(),
                        target.display(),
                        rename_err,
                        e
                    )
                })?;
                fs::remove_dir_all(&source).map_err(|e| {
                    format!(
                        "Failed to remove malformed directory {}: {}",
                        source.display(),
                        e
                    )
                })?;
            }
        } else {
            if target.exists() || target.is_symlink() {
                if target.is_dir() && !target.is_symlink() {
                    let _ = fs::remove_dir_all(&target);
                } else {
                    let _ = fs::remove_file(&target);
                }
            }

            if let Err(rename_err) = fs::rename(&source, &target) {
                fs::copy(&source, &target).map_err(|e| {
                    format!(
                        "Failed to repair file {} -> {} after rename error {}: {}",
                        source.display(),
                        target.display(),
                        rename_err,
                        e
                    )
                })?;
                fs::remove_file(&source).map_err(|e| {
                    format!(
                        "Failed to remove malformed file {}: {}",
                        source.display(),
                        e
                    )
                })?;
            }
        }
    }

    Ok(())
}

#[command]
pub fn check_directory_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}
