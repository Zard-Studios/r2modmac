//! Byte-for-byte reproducible JSON for everything the app stores on disk, so a
//! git repo of the app data directory shows only what changed (issue #37).
//!
//! Maps written through here must be a [`std::collections::BTreeMap`] — a
//! `HashMap` reorders on every run.

use serde::Serialize;
use std::path::Path;

/// Pretty-printed, with a trailing newline so git sees a normal text file.
pub fn to_pretty_string<T>(value: &T) -> Result<String, String>
where
    T: ?Sized + Serialize,
{
    let mut json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    json.push('\n');
    Ok(json)
}

/// Same, written to `path`, creating the parent directory if needed.
///
/// The file is replaced in one step, so something reading it at the same
/// moment sees the old content or the new content, never half of either.
pub fn write_file<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: ?Sized + Serialize,
{
    let json = to_pretty_string(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_atomic(path, json.as_bytes())
}

/// Write beside the target, then rename over it.
///
/// `std::fs::write` empties the file before filling it. A reader that arrives
/// in between gets an empty or cut-off file; for settings that read fails to
/// parse, falls back to the defaults, and the next save then writes those
/// defaults over everything the user had set.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    // A symlinked file (say, into a dotfiles repo) must stay a symlink, so the
    // rename goes onto what it points at.
    let target = if path.is_symlink() {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let Some(name) = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return std::fs::write(path, bytes).map_err(|e| e.to_string());
    };
    let temp = target.with_file_name(format!(
        ".{}.{}-{}.tmp",
        name,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    if std::fs::write(&temp, bytes).is_err() {
        // No room or no right to create a second file: write in place, as before.
        let _ = std::fs::remove_file(&temp);
        return std::fs::write(path, bytes).map_err(|e| e.to_string());
    }
    if std::fs::rename(&temp, &target).is_err() {
        // Some systems refuse to replace a file another program holds open.
        let _ = std::fs::remove_file(&temp);
        return std::fs::write(path, bytes).map_err(|e| e.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-stable-json-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_reader_never_sees_a_half_written_file() {
        let dir = temp_dir("atomic");
        let path = dir.join("settings.json");
        let big: Vec<String> = (0..3000).map(|i| format!("entry-{i}")).collect();
        write_file(&path, &big).unwrap();

        let writer_path = path.clone();
        let writer_data = big.clone();
        let writer = std::thread::spawn(move || {
            for _ in 0..200 {
                write_file(&writer_path, &writer_data).unwrap();
            }
        });
        for _ in 0..2000 {
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(
                serde_json::from_str::<Vec<String>>(&text).is_ok(),
                "read a file that was only partly written"
            );
        }
        writer.join().unwrap();
    }

    #[test]
    fn writing_leaves_no_temporary_files_behind() {
        let dir = temp_dir("leftovers");
        let path = dir.join("data.json");
        for round in 0..5 {
            write_file(&path, &round).unwrap();
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["data.json".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_stays_a_symlink() {
        let dir = temp_dir("symlink");
        let real = dir.join("real.json");
        let link = dir.join("link.json");
        write_file(&real, &1).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write_file(&link, &2).unwrap();

        assert!(link.is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap().trim(), "2");
    }

    #[test]
    fn output_ends_with_exactly_one_newline() {
        let json = to_pretty_string(&serde_json::json!({ "a": 1 })).unwrap();
        assert!(json.ends_with("}\n"));
        assert!(!json.ends_with("\n\n"));
    }

    #[test]
    fn map_keys_come_out_sorted_whatever_order_they_went_in() {
        let mut forwards = BTreeMap::new();
        forwards.insert("zelda", 1);
        forwards.insert("balatro", 2);
        forwards.insert("lethal-company", 3);

        let mut backwards = BTreeMap::new();
        backwards.insert("lethal-company", 3);
        backwards.insert("balatro", 2);
        backwards.insert("zelda", 1);

        let json = to_pretty_string(&forwards).unwrap();
        assert_eq!(json, to_pretty_string(&backwards).unwrap());
        assert!(
            json.find("balatro") < json.find("lethal-company")
                && json.find("lethal-company") < json.find("zelda"),
            "keys are not in sorted order: {}",
            json
        );
    }

    #[test]
    fn json_values_keep_their_object_keys_sorted_too() {
        // The guarantee that profiles.json does not churn either.
        let value: serde_json::Value =
            serde_json::from_str(r#"{"name":"a","id":"b","mods":[{"z":1,"a":2}]}"#).unwrap();
        assert_eq!(
            to_pretty_string(&value).unwrap(),
            "{\n  \"id\": \"b\",\n  \"mods\": [\n    {\n      \"a\": 2,\n      \"z\": 1\n    }\n  ],\n  \"name\": \"a\"\n}\n"
        );
    }

    #[test]
    fn write_file_creates_the_parent_directory() {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-stable-json-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("nested").join("settings.json");
        write_file(&path, &serde_json::json!({ "ok": true })).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"ok\": true\n}\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
