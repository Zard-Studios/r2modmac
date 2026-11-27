//! `.r2mmignore`: a marker that keeps r2modmac's hands off a mod folder.
//!
//! A mod author working on a mod in place drops an empty `.r2mmignore` into
//! its folder (`Balatro/Mods/MyMod/.r2mmignore`, `OWML/Mods/MyMod/...`,
//! `BepInEx/plugins/MyMod/...`). From then on Apply neither removes, replaces
//! nor caches that folder, an install that would land in it is skipped, and
//! deleting the last profile leaves it where it is (issue #54).
//!
//! The marker lives in the folder rather than in the profile because the
//! folder is what the game loads. Where several profiles share one mods
//! folder, a per-profile list would let the next profile's Apply delete the
//! work; where a profile owns its own tree (isolated BepInEx) the folder, and
//! so the marker, is already that profile's alone.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const IGNORE_MARKER: &str = ".r2mmignore";

/// Whether `dir` is a mod folder that carries the marker.
pub fn is_ignored_mod_dir(dir: &Path) -> bool {
    dir.is_dir() && dir.join(IGNORE_MARKER).is_file()
}

/// The marked folders directly inside `mods_root`, as (folder name, path).
pub fn ignored_mod_folders(mods_root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(mods_root) else {
        return Vec::new();
    };
    let mut folders = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| is_ignored_mod_dir(&entry.path()))
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().to_string(),
                entry.path(),
            )
        })
        .collect::<Vec<_>>();
    folders.sort();
    folders
}

/// Whether `path` is a link r2modmac made for a mod in the Ignore folder: a
/// symbolic link, or on Windows a directory junction. Never followed.
pub fn is_dir_link(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

/// Remove a link without touching the folder it points at.
pub fn remove_dir_link(path: &Path) -> io::Result<()> {
    // A Unix symlink goes with remove_file; a Windows junction is a
    // directory entry and goes with remove_dir, which never recurses.
    fs::remove_file(path).or_else(|_| fs::remove_dir(path))
}

/// Link `link` to the folder `source`, so the game reads the source in place.
pub fn create_dir_link(source: &Path, link: &Path) -> io::Result<()> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, link)
    }
    #[cfg(windows)]
    {
        // A junction needs no administrator rights or Developer Mode, which a
        // directory symlink does, and every program reads it as a folder.
        // mklink takes a "/" in a path for one of its switches, and paths built
        // with `join("BepInEx/plugins")` carry one, so it gets backslashes only.
        let backslashes = |path: &Path| path.to_string_lossy().replace('/', "\\");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(backslashes(link))
            .arg(backslashes(source))
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "mklink /J failed for {}",
                link.display()
            )))
        }
    }
}

/// The plugin folder a path relative to a BepInEx root belongs to:
/// `BepInEx/plugins/<folder>`, `BepInEx_DISABLED/plugins/<folder>`, or the
/// bare `plugins/<folder>` some macOS layouts use.
pub fn plugin_folder_of(relative: &str) -> Option<PathBuf> {
    let parts = relative
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let lower = |index: usize| parts.get(index).map(|part| part.to_ascii_lowercase());
    // The trees have fixed names. A path can spell them in any case, but on
    // Linux the disk does not forgive that, so the real names are used.
    match (lower(0).as_deref(), lower(1).as_deref()) {
        (Some(tree @ ("bepinex" | "bepinex_disabled")), Some("plugins")) if parts.len() >= 3 => {
            let tree = if tree == "bepinex" {
                "BepInEx"
            } else {
                "BepInEx_DISABLED"
            };
            Some([tree, "plugins", parts[2]].iter().collect())
        }
        (Some("plugins"), Some(_)) => Some(["plugins", parts[1]].iter().collect()),
        _ => None,
    }
}

/// Whether a path relative to `root` falls inside a marked plugin folder.
pub fn is_in_ignored_plugin_folder(root: &Path, relative: &str) -> bool {
    plugin_folder_of(relative).is_some_and(|folder| is_ignored_mod_dir(&root.join(folder)))
}

/// The marked plugin folders under a BepInEx root, live and disabled trees.
pub fn ignored_plugin_folders(root: &Path) -> Vec<(String, PathBuf)> {
    ["BepInEx", "BepInEx_DISABLED"]
        .iter()
        .flat_map(|tree| ignored_mod_folders(&root.join(tree).join("plugins")))
        .collect()
}

/// `fs::remove_dir_all`, except that marked folders inside `path` survive,
/// along with the directories that lead to them. Symbolic links are removed,
/// never followed. Returns whether anything was kept.
pub fn remove_dir_all_keeping_ignored(path: &Path) -> io::Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || is_dir_link(path) {
        remove_dir_link(path)?;
        return Ok(false);
    }
    if !metadata.is_dir() {
        fs::remove_file(path)?;
        return Ok(false);
    }
    if is_ignored_mod_dir(path) {
        log::info!(
            "[r2mmignore] Keeping {} because it carries {}",
            path.display(),
            IGNORE_MARKER
        );
        return Ok(true);
    }
    let mut kept = false;
    for entry in fs::read_dir(path)? {
        kept |= remove_dir_all_keeping_ignored(&entry?.path())?;
    }
    if !kept {
        fs::remove_dir(path)?;
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "r2modmac-r2mmignore-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn mark(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(IGNORE_MARKER), b"").unwrap();
    }

    #[test]
    fn only_a_marker_file_marks_a_folder() {
        let root = temp_root("marker");
        let marked = root.join("Mine");
        mark(&marked);
        let plain = root.join("Theirs");
        fs::create_dir_all(plain.join(IGNORE_MARKER)).unwrap();

        assert!(is_ignored_mod_dir(&marked));
        assert!(
            !is_ignored_mod_dir(&plain),
            "a directory named like the marker is not one"
        );
        assert!(!is_ignored_mod_dir(&root.join("Missing")));
        assert_eq!(
            ignored_mod_folders(&root),
            vec![("Mine".to_string(), marked.clone())]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_paths_resolve_to_their_plugin_folder() {
        let root = temp_root("plugin-paths");
        mark(&root.join("BepInEx/plugins/Me-Wip"));
        mark(&root.join("BepInEx_DISABLED/plugins/Me-Off"));
        mark(&root.join("plugins/Me-Bare"));

        assert!(is_in_ignored_plugin_folder(
            &root,
            "BepInEx/plugins/Me-Wip/Wip.dll"
        ));
        assert!(is_in_ignored_plugin_folder(&root, "bepinex/plugins/Me-Wip"));
        assert!(is_in_ignored_plugin_folder(
            &root,
            "BepInEx_DISABLED/plugins/Me-Off/a/b.dll"
        ));
        assert!(is_in_ignored_plugin_folder(&root, "plugins/Me-Bare/x.dll"));
        assert!(!is_in_ignored_plugin_folder(
            &root,
            "BepInEx/plugins/Other/x.dll"
        ));
        assert!(!is_in_ignored_plugin_folder(
            &root,
            "BepInEx/config/Me-Wip.cfg"
        ));
        assert!(!is_in_ignored_plugin_folder(&root, "BepInEx/plugins"));
        assert_eq!(ignored_plugin_folders(&root).len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removal_keeps_marked_folders_and_their_parents_only() {
        let root = temp_root("remove");
        let mods = root.join("Mods");
        mark(&mods.join("Mine"));
        fs::write(mods.join("Mine/main.lua"), b"wip").unwrap();
        fs::create_dir_all(mods.join("Theirs/assets")).unwrap();
        fs::write(mods.join("Theirs/assets/a.png"), b"x").unwrap();
        fs::write(mods.join("loose.txt"), b"x").unwrap();

        assert!(remove_dir_all_keeping_ignored(&mods).unwrap());
        assert_eq!(fs::read(mods.join("Mine/main.lua")).unwrap(), b"wip");
        assert!(!mods.join("Theirs").exists());
        assert!(!mods.join("loose.txt").exists());

        fs::remove_file(mods.join("Mine").join(IGNORE_MARKER)).unwrap();
        assert!(!remove_dir_all_keeping_ignored(&mods).unwrap());
        assert!(!mods.exists());
        assert!(
            !remove_dir_all_keeping_ignored(&mods).unwrap(),
            "a missing path is not an error"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_link_comes_and_goes_without_touching_its_source() {
        let root = temp_root("link");
        let source = root.join("dev/MyMod");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("main.lua"), b"wip").unwrap();
        let link = root.join("Mods/MyMod");

        create_dir_link(&source, &link).unwrap();
        assert!(is_dir_link(&link));
        assert!(!is_dir_link(&source));
        assert_eq!(
            fs::read(link.join("main.lua")).unwrap(),
            b"wip",
            "the game reads through the link"
        );
        fs::write(source.join("main.lua"), b"edited").unwrap();
        assert_eq!(
            fs::read(link.join("main.lua")).unwrap(),
            b"edited",
            "edits show up at once"
        );

        remove_dir_link(&link).unwrap();
        assert!(!link.exists() && !is_dir_link(&link));
        assert_eq!(fs::read(source.join("main.lua")).unwrap(), b"edited");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn removal_unlinks_symlinks_without_entering_them() {
        let root = temp_root("symlink");
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"x").unwrap();
        let tree = root.join("BepInEx");
        fs::create_dir_all(&tree).unwrap();
        std::os::unix::fs::symlink(&outside, tree.join("link")).unwrap();

        assert!(!remove_dir_all_keeping_ignored(&tree).unwrap());
        assert!(!tree.exists());
        assert!(outside.join("keep.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
