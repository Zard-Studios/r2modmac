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
pub fn write_file<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: ?Sized + Serialize,
{
    let json = to_pretty_string(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
