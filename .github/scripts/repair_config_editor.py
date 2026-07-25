from pathlib import Path
import re


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


profile_path = Path("src-tauri/src/commands/profile_commands.rs")
profile = profile_path.read_text()

profile = replace_once(
    profile,
    "use std::io::Write;\nuse std::sync::OnceLock;",
    "use std::io::Write;\nuse std::path::{Component, Path, PathBuf};\nuse std::sync::OnceLock;",
    "profile imports",
)

profile = replace_once(
    profile,
    """    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
""",
    """    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };

        // Never follow symlinks: a mod-controlled link could escape the
        // scanned root or create a recursive directory cycle.
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
""",
    "recursive scanner file type",
)
profile = replace_once(
    profile,
    "        } else if path.is_file() {",
    "        } else if file_type.is_file() {",
    "recursive scanner file branch",
)
profile = replace_once(
    profile,
    """    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
""",
    """    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
""",
    "flat scanner file type",
)
profile = replace_once(
    profile,
    """                        } else {
                            // BepInEx/config  — flat scan
                            let bep_config = game_path.join("BepInEx").join("config");
                            if bep_config.is_dir() {
                                collect_config_files_flat(&bep_config, game_path, &mut files);
                            }
""",
    """                        } else {
                            // BepInEx/config — recursive because some mods group
                            // their generated config files in nested folders.
                            let bep_config = game_path.join("BepInEx").join("config");
                            if bep_config.is_dir() {
                                collect_config_files(&bep_config, game_path, &mut files);
                            }
""",
    "recursive BepInEx config scan",
)

helpers = r'''
fn profile_config_root(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    Ok(crate::utils::paths::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id))
}

fn configured_game_roots(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let settings_path = crate::utils::paths::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if !settings_path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&settings_path)
        .map_err(|e| format!("Failed to read settings: {}", e))?;
    let settings: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse settings: {}", e))?;
    let Some(paths) = settings.get("game_paths").and_then(|value| value.as_object()) else {
        return Ok(Vec::new());
    };

    let mut roots = Vec::new();
    for value in paths.values().filter_map(|value| value.as_str()) {
        if let Ok(root) = PathBuf::from(value).canonicalize() {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    Ok(roots)
}

fn resolve_config_root(
    app: &AppHandle,
    profile_id: &str,
    requested_root: Option<String>,
) -> Result<PathBuf, String> {
    let profile_root = profile_config_root(app, profile_id)?;
    let requested = requested_root
        .map(PathBuf::from)
        .unwrap_or_else(|| profile_root.clone());
    let canonical_requested = requested
        .canonicalize()
        .map_err(|e| format!("Cannot resolve config root: {}", e))?;

    if let Ok(canonical_profile) = profile_root.canonicalize() {
        if canonical_requested == canonical_profile {
            return Ok(canonical_requested);
        }
    }

    if configured_game_roots(app)?
        .iter()
        .any(|root| root == &canonical_requested)
    {
        return Ok(canonical_requested);
    }

    Err("Unauthorized config root".to_string())
}

fn validate_relative_config_path(
    relative_path: &str,
    allow_empty: bool,
) -> Result<PathBuf, String> {
    if relative_path.trim().is_empty() {
        return if allow_empty {
            Ok(PathBuf::new())
        } else {
            Err("Config path cannot be empty".to_string())
        };
    }

    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err("Absolute config paths are not allowed".to_string());
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Path traversal detected".to_string());
            }
        }
    }

    if clean.as_os_str().is_empty() && !allow_empty {
        return Err("Config path cannot be empty".to_string());
    }
    Ok(clean)
}

fn is_supported_config_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "manifest.json" | "mods.yml" | "readme.md" | "readme.txt"
    ) {
        return false;
    }

    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            let extension = extension.to_ascii_lowercase();
            CONFIG_EXTENSIONS.contains(&extension.as_str())
        })
        .unwrap_or(false)
}

fn resolve_existing_config_target(
    app: &AppHandle,
    profile_id: &str,
    relative_path: &str,
    root: Option<String>,
    allow_directory: bool,
) -> Result<PathBuf, String> {
    let base_dir = resolve_config_root(app, profile_id, root)?;
    let relative = validate_relative_config_path(relative_path, allow_directory)?;
    let target = base_dir.join(relative);
    let canonical_target = target
        .canonicalize()
        .map_err(|e| format!("Cannot resolve config path: {}", e))?;

    if !canonical_target.starts_with(&base_dir) {
        return Err("Path traversal detected".to_string());
    }

    if allow_directory {
        if !canonical_target.is_file() && !canonical_target.is_dir() {
            return Err("Config path does not exist".to_string());
        }
    } else if !canonical_target.is_file() || !is_supported_config_file(&canonical_target) {
        return Err("Unsupported config file".to_string());
    }

    Ok(canonical_target)
}

'''
marker = "#[command]\npub fn list_profile_config_files("
if profile.count(marker) != 1:
    raise RuntimeError("list_profile_config_files marker not found exactly once")
profile = profile.replace(marker, helpers + marker, 1)

new_tail = r'''#[command]
pub fn read_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    root: Option<String>,
) -> Result<String, String> {
    let target = resolve_existing_config_target(&app, &profile_id, &relative_path, root, false)?;
    let content =
        fs::read_to_string(&target).map_err(|e| format!("Failed to read file: {}", e))?;

    // Strip UTF-8 BOM (Byte Order Mark) if present.
    let stripped = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
    Ok(stripped.to_string())
}

#[command]
pub fn write_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    content: String,
    root: Option<String>,
) -> Result<bool, String> {
    let target = resolve_existing_config_target(&app, &profile_id, &relative_path, root, false)?;
    fs::write(&target, content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(true)
}

#[command]
pub fn reveal_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    root: Option<String>,
) -> Result<(), String> {
    let target = resolve_existing_config_target(&app, &profile_id, &relative_path, root, true)?;

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if target.is_dir() {
            open::that(&target).map_err(|e| e.to_string())?;
        } else if let Some(parent) = target.parent() {
            open::that(parent).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[command]
pub fn open_profile_config_file(
    app: AppHandle,
    profile_id: String,
    relative_path: String,
    root: Option<String>,
) -> Result<(), String> {
    let target = resolve_existing_config_target(&app, &profile_id, &relative_path, root, false)?;
    open::that(&target).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod config_editor_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "r2modmac-{}-{}-{}",
            label,
            std::process::id(),
            nonce
        ))
    }

    #[test]
    fn validates_relative_config_paths() {
        assert_eq!(
            validate_relative_config_path("BepInEx/config/Author/mod.cfg", false).unwrap(),
            PathBuf::from("BepInEx/config/Author/mod.cfg")
        );
        assert!(validate_relative_config_path("../secret.cfg", false).is_err());
        assert!(validate_relative_config_path("", false).is_err());
        assert!(validate_relative_config_path("", true).is_ok());
    }

    #[test]
    fn recursive_scanner_finds_nested_config_files() {
        let root = temporary_directory("nested-configs");
        let nested = root.join("BepInEx/config/Author/Mod");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("settings.cfg"), "enabled = true").unwrap();
        fs::write(nested.join("plugin.dll"), b"not a config").unwrap();

        let mut files = Vec::new();
        collect_config_files(&root.join("BepInEx/config"), &root, &mut files);

        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].relative_path,
            "BepInEx/config/Author/Mod/settings.cfg"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn recursive_scanner_does_not_follow_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("symlink-root");
        let outside = temporary_directory("symlink-outside");
        fs::create_dir_all(root.join("BepInEx/config")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("outside.cfg"), "value = 1").unwrap();
        symlink(&outside, root.join("BepInEx/config/linked")).unwrap();

        let mut files = Vec::new();
        collect_config_files(&root.join("BepInEx/config"), &root, &mut files);

        assert!(files.is_empty());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
'''
tail_pattern = re.compile(r"#\[command\]\npub fn read_profile_config_file\([\s\S]*\Z")
profile, replacements = tail_pattern.subn(new_tail, profile, count=1)
if replacements != 1:
    raise RuntimeError(
        f"config command tail: expected one replacement, found {replacements}"
    )
profile_path.write_text(profile)

system_path = Path("src-tauri/src/commands/system_commands.rs")
system = system_path.read_text()
system = replace_once(
    system,
    "use std::os::unix::fs::PermissionsExt;\nuse tauri::{command, AppHandle, Emitter, Manager, State};",
    "use std::os::unix::fs::PermissionsExt;\nuse std::time::Duration;\nuse tauri::{command, AppHandle, Emitter, Manager, State};",
    "system imports",
)

old_fetch = '''#[command]
pub async fn fetch_text_content(url: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("r2modmac")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}
'''
new_fetch = '''const MAX_TEXT_CONTENT_BYTES: usize = 5 * 1024 * 1024;

fn validate_text_content_url(raw_url: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw_url).map_err(|_| "Invalid URL".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err("Only standard HTTPS URLs are allowed".to_string());
    }

    match url.host_str().map(|host| host.to_ascii_lowercase()) {
        Some(host)
            if matches!(
                host.as_str(),
                "api.github.com" | "thunderstore.io" | "www.thunderstore.io"
            ) => Ok(url),
        _ => Err("URL host is not allowed".to_string()),
    }
}

#[command]
pub async fn fetch_text_content(url: String) -> Result<String, String> {
    let url = validate_text_content_url(&url)?;
    let client = reqwest::Client::builder()
        .user_agent("r2modmac")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Remote server returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TEXT_CONTENT_BYTES as u64)
    {
        return Err("Remote text content is too large".to_string());
    }

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_TEXT_CONTENT_BYTES {
        return Err("Remote text content is too large".to_string());
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| "Remote response is not valid UTF-8 text".to_string())
}
'''
system = replace_once(system, old_fetch, new_fetch, "fetch_text_content")

test_marker = '''    #[test]
    fn retrieves_username() {
'''
security_tests = '''    #[test]
    fn validates_remote_text_content_urls() {
        assert!(validate_text_content_url(
            "https://api.github.com/repos/Zard-Studios/r2modmac/readme"
        )
        .is_ok());
        assert!(validate_text_content_url(
            "https://thunderstore.io/api/cyberstorm/package/owner/mod/v/1.0.0/readme/"
        )
        .is_ok());

        for blocked in [
            "http://api.github.com/repos/example/example",
            "https://127.0.0.1/private",
            "https://localhost/private",
            "https://api.github.com.evil.example/private",
            "https://api.github.com:444/private",
        ] {
            assert!(validate_text_content_url(blocked).is_err(), "{}", blocked);
        }
    }

    #[test]
    fn retrieves_username() {
'''
system = replace_once(system, test_marker, security_tests, "security URL tests")
system_path.write_text(system)
