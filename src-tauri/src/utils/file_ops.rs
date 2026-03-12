use std::fs;
use tauri::command;

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
                if let (Ok(src_meta), Ok(dst_meta)) = (fs::metadata(entry.path()), fs::metadata(&dest_path)) {
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
        eprintln!("[copy_dir_recursive] {:?} -> {:?}: {} files, {} dirs", 
            src.file_name().unwrap_or_default(), 
            dst.file_name().unwrap_or_default(), 
            files_copied, dirs_created);
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

        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-r", "-d", "com.apple.quarantine"])
            .arg(path)
            .output();
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


#[command]
pub fn check_directory_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}
