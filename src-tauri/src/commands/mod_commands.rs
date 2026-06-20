use crate::commands::game_commands::get_game_path;
use crate::models::shared::*;
use crate::utils::file_ops::*;
use crate::utils::mod_manifest::{
    backup_existing_mod_files, save_owned_mod_manifest, GAME_MANIFEST_SCOPE,
};
use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{command, AppHandle, Emitter};

const APP_USER_AGENT: &str = concat!("r2modmac/", env!("CARGO_PKG_VERSION"));
const NEWTONSOFT_JSON_VERSION: &str = "12.0.3";
const NEWTONSOFT_JSON_NETSTANDARD20_ENTRY: &str = "lib/netstandard2.0/Newtonsoft.Json.dll";
const ROR2_CROSSOVER_NEWTONSOFT_TARGET: &str = "BepInEx/core/Newtonsoft.Json.dll";
const CUSTOM_MOD_MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const CUSTOM_MOD_MAX_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const CUSTOM_MOD_MAX_SINGLE_FILE_BYTES: u64 = 768 * 1024 * 1024;
const CUSTOM_MOD_MAX_ENTRIES: usize = 4096;
const CUSTOM_MOD_MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const CUSTOM_MOD_MAX_README_BYTES: u64 = 512 * 1024;
const CUSTOM_MOD_MAX_ICON_BYTES: u64 = 2 * 1024 * 1024;
const CUSTOM_MOD_MAX_PATH_CHARS: usize = 240;
const CUSTOM_MOD_MAX_RATE_WINDOW: Duration = Duration::from_secs(60);
const CUSTOM_MOD_MAX_RATE_EVENTS: usize = 12;
const CUSTOM_MOD_CANCELLED_MESSAGE: &str = "Custom mod import cancelled.";

static CUSTOM_MOD_RATE_LIMITER: OnceLock<Mutex<HashMap<String, VecDeque<SystemTime>>>> =
    OnceLock::new();
static CUSTOM_MOD_IMPORT_ACTIVE: AtomicBool = AtomicBool::new(false);
static CUSTOM_MOD_IMPORT_CANCELLED: AtomicBool = AtomicBool::new(false);

// Shared HTTP client for all Thunderstore package fetching operations.
// A single client reuses the connection pool and TLS session, avoiding the
// overhead of creating a new pool (+ thread/socket/TLS resources) per call.
static THUNDERSTORE_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn thunderstore_client() -> &'static reqwest::Client {
    THUNDERSTORE_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(60))
            .gzip(true)
            .build()
            .expect("Failed to build Thunderstore HTTP client")
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomModSecurityReport {
    risk_level: String,
    warnings: Vec<String>,
    executable_files: Vec<String>,
    total_files: usize,
    total_uncompressed_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomModInspection {
    file_name: String,
    file_size: u64,
    sha256: String,
    manifest_sha256: Option<String>,
    content_fingerprint: String,
    suggested_name: String,
    suggested_description: Option<String>,
    suggested_author: String,
    suggested_version: String,
    readme: Option<String>,
    icon_data_url: Option<String>,
    platforms: Vec<String>,
    security_report: CustomModSecurityReport,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLocalModMetadata {
    local_id: String,
    full_name: String,
    display_name: String,
    author: String,
    version_number: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    readme: Option<String>,
    #[serde(default)]
    icon_data_url: Option<String>,
    file_name: String,
    file_size: u64,
    sha256: String,
    #[serde(default)]
    manifest_sha256: Option<String>,
    #[serde(default)]
    content_fingerprint: String,
    #[serde(default)]
    source_path: Option<String>,
    platforms: Vec<String>,
    imported_at: u128,
    security_report: CustomModSecurityReport,
}

#[derive(Clone)]
struct RuntimeCompatAsset {
    relative_path: std::path::PathBuf,
    bytes: Vec<u8>,
    label: &'static str,
}

fn guard_custom_mod_rate_limit(action: &str) -> Result<(), String> {
    let now = SystemTime::now();
    let limiter = CUSTOM_MOD_RATE_LIMITER.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = limiter
        .lock()
        .map_err(|_| "Custom mod safety limiter is unavailable".to_string())?;
    let events = guard
        .entry(action.to_string())
        .or_insert_with(VecDeque::new);
    while let Some(front) = events.front().copied() {
        if now.duration_since(front).unwrap_or_default() > CUSTOM_MOD_MAX_RATE_WINDOW {
            events.pop_front();
        } else {
            break;
        }
    }
    if events.len() >= CUSTOM_MOD_MAX_RATE_EVENTS {
        return Err(
            "Too many custom mod requests. Please wait a minute before trying again.".to_string(),
        );
    }
    events.push_back(now);
    Ok(())
}

struct CustomModImportGuard;

impl Drop for CustomModImportGuard {
    fn drop(&mut self) {
        CUSTOM_MOD_IMPORT_ACTIVE.store(false, Ordering::SeqCst);
        CUSTOM_MOD_IMPORT_CANCELLED.store(false, Ordering::SeqCst);
    }
}

fn begin_custom_mod_import() -> CustomModImportGuard {
    CUSTOM_MOD_IMPORT_ACTIVE.store(true, Ordering::SeqCst);
    CUSTOM_MOD_IMPORT_CANCELLED.store(false, Ordering::SeqCst);
    CustomModImportGuard
}

fn check_custom_mod_cancelled() -> Result<(), String> {
    if CUSTOM_MOD_IMPORT_CANCELLED.load(Ordering::SeqCst) {
        Err(CUSTOM_MOD_CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

fn sanitize_mod_slug(value: Option<String>, fallback: &str) -> String {
    let source = value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    let mut out = String::new();
    let mut last_was_sep = false;
    for ch in source.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_sep = false;
        } else if matches!(ch, '_' | '-' | '.') || ch.is_whitespace() {
            if !last_was_sep {
                out.push('_');
                last_was_sep = true;
            }
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn sanitize_version(value: Option<String>, fallback: &str) -> String {
    let raw = value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    let mut cleaned = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        .collect::<String>();
    if cleaned.is_empty() {
        cleaned = "1.0.0".to_string();
    }
    if !cleaned.chars().any(|c| c == '.') {
        cleaned.push_str(".0.0");
    }
    cleaned.chars().take(32).collect()
}

fn safe_platforms(platforms: Option<Vec<String>>) -> Vec<String> {
    let mut values = platforms
        .unwrap_or_else(|| {
            vec![
                "windows".to_string(),
                "mac".to_string(),
                "linux".to_string(),
            ]
        })
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| matches!(value.as_str(), "windows" | "mac" | "linux"))
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.is_empty() {
        values = vec![
            "windows".to_string(),
            "mac".to_string(),
            "linux".to_string(),
        ];
    }
    values
}

fn is_zip_symlink(file: &zip::read::ZipFile<'_>) -> bool {
    file.unix_mode()
        .map(|mode| (mode & 0o170000) == 0o120000)
        .unwrap_or(false)
}

fn is_executable_like(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let Some(ext) = lower.rsplit('.').next() else {
        return false;
    };
    matches!(
        ext,
        "exe"
            | "dll"
            | "dylib"
            | "so"
            | "sh"
            | "bash"
            | "zsh"
            | "command"
            | "bat"
            | "cmd"
            | "ps1"
            | "vbs"
            | "js"
            | "jar"
            | "py"
            | "rb"
            | "pl"
            | "msi"
            | "scr"
    ) || lower.ends_with(".app/contents/macos")
}

fn hash_file(path: &std::path::Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_custom_mod_cancelled()?;
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn read_zip_file_to_bytes(
    file: &mut zip::read::ZipFile<'_>,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    if file.size() > max_bytes {
        return Err(format!("File exceeds {} KB.", max_bytes / 1024));
    }
    let mut bytes = Vec::with_capacity(file.size().min(max_bytes) as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!("File exceeds {} KB.", max_bytes / 1024));
    }
    Ok(bytes)
}

fn read_zip_file_to_string(
    file: &mut zip::read::ZipFile<'_>,
    max_bytes: u64,
) -> Result<String, String> {
    let bytes = read_zip_file_to_bytes(file, max_bytes)?;
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8.".to_string())
}

fn zip_file_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn readme_candidate_score(path: &str) -> Option<usize> {
    let basename = zip_file_basename(path).to_ascii_lowercase();
    let is_readme = basename == "readme"
        || basename == "readme.md"
        || basename == "readme.txt"
        || basename == "readme.markdown"
        || basename == "readme.rst"
        || basename.starts_with("readme.");
    if !is_readme {
        return None;
    }
    let depth = path.matches('/').count();
    Some(depth)
}

fn icon_mime_for_path(path: &str) -> Option<&'static str> {
    let basename = zip_file_basename(path).to_ascii_lowercase();
    match basename.as_str() {
        "icon.png" => Some("image/png"),
        "icon.jpg" | "icon.jpeg" => Some("image/jpeg"),
        "icon.webp" => Some("image/webp"),
        _ => None,
    }
}

fn parse_custom_mod_manifest<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> (Option<serde_json::Value>, Option<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut manifest_index: Option<usize> = None;

    for i in 0..archive.len() {
        if check_custom_mod_cancelled().is_err() {
            return (None, None, warnings);
        }
        let Ok(file) = archive.by_index(i) else {
            continue;
        };
        let Some(name) = normalize_zip_entry_name(file.name()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if lower == "manifest.json" || lower.ends_with("/manifest.json") {
            manifest_index = Some(i);
            break;
        }
    }

    let Some(index) = manifest_index else {
        warnings.push(
            "No Thunderstore-style manifest.json was found; defaults will be used.".to_string(),
        );
        return (None, None, warnings);
    };

    let mut file = match archive.by_index(index) {
        Ok(file) => file,
        Err(_) => return (None, None, warnings),
    };
    if file.size() > CUSTOM_MOD_MAX_MANIFEST_BYTES {
        warnings.push("manifest.json is too large and was ignored.".to_string());
        return (None, None, warnings);
    }
    let content = match read_zip_file_to_string(&mut file, CUSTOM_MOD_MAX_MANIFEST_BYTES) {
        Ok(content) => content,
        Err(_) => {
            warnings.push("manifest.json could not be read as UTF-8 and was ignored.".to_string());
            return (None, None, warnings);
        }
    };
    let manifest_sha256 = Some(hash_bytes(content.as_bytes()));
    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(value) => (Some(value), manifest_sha256, warnings),
        Err(_) => {
            warnings.push("manifest.json is invalid JSON and was ignored.".to_string());
            (None, manifest_sha256, warnings)
        }
    }
}

fn inspect_custom_mod_archive<R: std::io::Read + std::io::Seek>(
    mut archive: zip::ZipArchive<R>,
    file_name: String,
    file_size: u64,
    sha256: String,
) -> Result<CustomModInspection, String> {
    if archive.len() == 0 {
        return Err("The custom mod archive is empty.".to_string());
    }
    if archive.len() > CUSTOM_MOD_MAX_ENTRIES {
        return Err(format!(
            "The custom mod archive contains too many files ({} > {}).",
            archive.len(),
            CUSTOM_MOD_MAX_ENTRIES
        ));
    }

    let (manifest, manifest_sha256, mut warnings) = parse_custom_mod_manifest(&mut archive);
    let mut executable_files = Vec::new();
    let mut normalized_paths = HashSet::new();
    let mut total_files = 0usize;
    let mut total_uncompressed = 0u64;
    let mut has_payload_file = false;
    let mut readme_content: Option<(usize, String)> = None;
    let mut icon_data_url: Option<String> = None;
    let mut fingerprint_parts = Vec::new();

    for i in 0..archive.len() {
        check_custom_mod_cancelled()?;
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw_name = file.name().to_string();
        if is_zip_symlink(&file) {
            return Err(format!("Blocked symlink entry in archive: {}", raw_name));
        }

        let normalized_path = normalize_zip_entry_name(&raw_name)
            .ok_or_else(|| format!("Blocked unsafe archive path: {}", raw_name))?;
        if normalized_path.chars().count() > CUSTOM_MOD_MAX_PATH_CHARS {
            return Err(format!(
                "Blocked overlong archive path: {}",
                normalized_path
            ));
        }
        let path_key = normalized_path.to_ascii_lowercase();
        if !normalized_paths.insert(path_key) {
            return Err(format!(
                "Blocked duplicate archive path: {}",
                normalized_path
            ));
        }

        if zip_entry_is_dir(&raw_name) {
            continue;
        }

        fingerprint_parts.push(format!(
            "{}:{}:{}",
            normalized_path,
            file.size(),
            file.crc32()
        ));
        total_files += 1;
        has_payload_file = true;
        total_uncompressed = total_uncompressed
            .checked_add(file.size())
            .ok_or_else(|| "Archive size accounting overflowed.".to_string())?;
        if total_uncompressed > CUSTOM_MOD_MAX_UNCOMPRESSED_BYTES {
            return Err(format!(
                "Blocked archive: unpacked size exceeds {} MB.",
                CUSTOM_MOD_MAX_UNCOMPRESSED_BYTES / 1024 / 1024
            ));
        }
        if file.size() > CUSTOM_MOD_MAX_SINGLE_FILE_BYTES {
            return Err(format!(
                "Blocked file over {} MB: {}",
                CUSTOM_MOD_MAX_SINGLE_FILE_BYTES / 1024 / 1024,
                normalized_path
            ));
        }

        let compressed = file.compressed_size();
        if compressed == 0 && file.size() > 0 {
            return Err(format!(
                "Blocked suspicious zero-compressed-size entry: {}",
                normalized_path
            ));
        }
        if compressed > 0 && file.size() > 10 * 1024 * 1024 && file.size() / compressed.max(1) > 250
        {
            return Err(format!(
                "Blocked suspicious compression ratio for {}.",
                normalized_path
            ));
        }

        if normalized_path.starts_with("__MACOSX/") {
            warnings.push(
                "Archive contains macOS metadata folders; they will be ignored by the installer."
                    .to_string(),
            );
        }
        if is_executable_like(&normalized_path) {
            executable_files.push(normalized_path.clone());
        }

        if let Some(score) = readme_candidate_score(&normalized_path) {
            let should_read = readme_content
                .as_ref()
                .map(|(current_score, _)| score < *current_score)
                .unwrap_or(true);
            if should_read {
                match read_zip_file_to_string(&mut file, CUSTOM_MOD_MAX_README_BYTES) {
                    Ok(content) => {
                        readme_content = Some((score, content));
                    }
                    Err(_) => {
                        warnings.push(format!("README could not be read: {}", normalized_path));
                    }
                }
            }
        } else if icon_data_url.is_none() {
            if let Some(mime) = icon_mime_for_path(&normalized_path) {
                match read_zip_file_to_bytes(&mut file, CUSTOM_MOD_MAX_ICON_BYTES) {
                    Ok(bytes) => {
                        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                        icon_data_url = Some(format!("data:{};base64,{}", mime, encoded));
                    }
                    Err(_) => {
                        warnings.push(format!("Icon could not be read: {}", normalized_path));
                    }
                }
            }
        }
    }

    if !has_payload_file {
        return Err("The custom mod archive does not contain any files.".to_string());
    }

    executable_files.sort();
    executable_files.dedup();
    warnings.sort();
    warnings.dedup();

    if !executable_files.is_empty() {
        warnings.push(format!(
            "Detected {} executable or loadable file(s). Custom mods can run code in-game.",
            executable_files.len()
        ));
    }

    let manifest_name = manifest
        .as_ref()
        .and_then(|value| value["name"].as_str())
        .map(|value| value.to_string());
    let manifest_version = manifest
        .as_ref()
        .and_then(|value| value["version_number"].as_str())
        .map(|value| value.to_string());
    let manifest_description = manifest
        .as_ref()
        .and_then(|value| value["description"].as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(2000).collect::<String>());

    let stem = std::path::Path::new(&file_name)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "CustomMod".to_string());
    let suggested_name = sanitize_mod_slug(manifest_name, &stem);
    let suggested_version = sanitize_version(manifest_version, "1.0.0");
    fingerprint_parts.sort();
    let content_fingerprint = hash_bytes(fingerprint_parts.join("\n").as_bytes());
    let risk_level = if executable_files.iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        lower.ends_with(".exe")
            || lower.ends_with(".sh")
            || lower.ends_with(".command")
            || lower.ends_with(".bat")
            || lower.ends_with(".cmd")
            || lower.ends_with(".ps1")
            || lower.ends_with(".vbs")
            || lower.ends_with(".js")
    }) {
        "high"
    } else if !executable_files.is_empty() || !warnings.is_empty() {
        "medium"
    } else {
        "low"
    }
    .to_string();

    Ok(CustomModInspection {
        file_name,
        file_size,
        sha256,
        manifest_sha256,
        content_fingerprint,
        suggested_name,
        suggested_description: manifest_description,
        suggested_author: "Local".to_string(),
        suggested_version,
        readme: readme_content.map(|(_, content)| content),
        icon_data_url,
        platforms: vec![
            "windows".to_string(),
            "mac".to_string(),
            "linux".to_string(),
        ],
        security_report: CustomModSecurityReport {
            risk_level,
            warnings,
            executable_files,
            total_files,
            total_uncompressed_bytes: total_uncompressed,
        },
    })
}

fn inspect_custom_mod_file_with_name(
    path: &std::path::Path,
    display_name: Option<String>,
) -> Result<CustomModInspection, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Please choose a .zip/.r2z file, not a folder.".to_string());
    }
    if metadata.len() > CUSTOM_MOD_MAX_ARCHIVE_BYTES {
        return Err(format!(
            "Custom mod archive is too large ({} MB max).",
            CUSTOM_MOD_MAX_ARCHIVE_BYTES / 1024 / 1024
        ));
    }
    let file_name = display_name.unwrap_or_else(|| {
        path.file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "custom-mod.zip".to_string())
    });
    let sha256 = hash_file(path)?;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;
    inspect_custom_mod_archive(archive, file_name, metadata.len(), sha256)
}

fn inspect_custom_mod_file(path: &std::path::Path) -> Result<CustomModInspection, String> {
    inspect_custom_mod_file_with_name(path, None)
}

fn zip_entry_name_from_relative_path(path: &std::path::Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(value) = component else {
            return Err("Blocked unsafe folder entry path.".to_string());
        };
        let part = value.to_string_lossy();
        if part.is_empty() || part == "." || part == ".." {
            return Err("Blocked unsafe folder entry path.".to_string());
        }
        parts.push(part.to_string());
    }

    if parts.is_empty() {
        return Err("Blocked empty folder entry path.".to_string());
    }

    Ok(parts.join("/"))
}

fn package_custom_mod_folder_to_zip(
    source_dir: &std::path::Path,
    target_zip: &std::path::Path,
) -> Result<(), String> {
    check_custom_mod_cancelled()?;
    let metadata = fs::metadata(source_dir).map_err(|e| e.to_string())?;
    if !metadata.is_dir() {
        return Err("Please choose a custom mod folder.".to_string());
    }

    if let Some(parent) = target_zip.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = fs::File::create(target_zip).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut total_files = 0usize;
    let mut total_bytes = 0u64;

    for entry in walkdir::WalkDir::new(source_dir).follow_links(false) {
        check_custom_mod_cancelled()?;
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "Blocked symlink inside custom mod folder: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let relative = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|e| e.to_string())?;
        let entry_name = zip_entry_name_from_relative_path(relative)?;
        if entry_name.chars().count() > CUSTOM_MOD_MAX_PATH_CHARS {
            return Err(format!(
                "Blocked overlong folder entry path: {}",
                entry_name
            ));
        }
        if entry_name == ".DS_Store" || entry_name.ends_with("/.DS_Store") {
            continue;
        }

        let file_metadata = entry.metadata().map_err(|e| e.to_string())?;
        total_files += 1;
        if total_files > CUSTOM_MOD_MAX_ENTRIES {
            return Err(format!(
                "The custom mod folder contains too many files ({} > {}).",
                total_files, CUSTOM_MOD_MAX_ENTRIES
            ));
        }
        if file_metadata.len() > CUSTOM_MOD_MAX_SINGLE_FILE_BYTES {
            return Err(format!(
                "Blocked file over {} MB: {}",
                CUSTOM_MOD_MAX_SINGLE_FILE_BYTES / 1024 / 1024,
                entry_name
            ));
        }
        total_bytes = total_bytes
            .checked_add(file_metadata.len())
            .ok_or_else(|| "Folder size accounting overflowed.".to_string())?;
        if total_bytes > CUSTOM_MOD_MAX_UNCOMPRESSED_BYTES {
            return Err(format!(
                "Blocked folder: unpacked size exceeds {} MB.",
                CUSTOM_MOD_MAX_UNCOMPRESSED_BYTES / 1024 / 1024
            ));
        }

        zip.start_file(entry_name, options)
            .map_err(|e| e.to_string())?;
        let mut input = fs::File::open(entry.path()).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            check_custom_mod_cancelled()?;
            let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            zip.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        }
    }

    if total_files == 0 {
        return Err("The selected custom mod folder does not contain any files.".to_string());
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn temp_custom_mod_zip_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "r2modmac-custom-mod-{}-{}.zip",
        now_millis(),
        label
    ))
}

fn inspect_custom_mod_path(path: &std::path::Path) -> Result<CustomModInspection, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.is_file() {
        return inspect_custom_mod_file(path);
    }
    if metadata.is_dir() {
        let folder_name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "custom-mod-folder".to_string());
        let temp_path = temp_custom_mod_zip_path("inspect");
        if let Err(err) = package_custom_mod_folder_to_zip(path, &temp_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(err);
        }
        let result = inspect_custom_mod_file_with_name(&temp_path, Some(folder_name));
        let _ = fs::remove_file(&temp_path);
        return result;
    }

    Err("Please choose a custom mod folder or .zip/.r2z archive.".to_string())
}

fn local_mod_dir(
    app: &AppHandle,
    profile_id: &str,
    local_id: &str,
) -> Result<std::path::PathBuf, String> {
    Ok(crate::utils::paths::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id)
        .join("local_mods")
        .join(local_id))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn make_local_mod_id(sha256: &str) -> String {
    let prefix = sha256.chars().take(16).collect::<String>();
    format!("local-{}-{}", now_millis(), prefix)
}

fn build_local_mod_response(
    local_id: String,
    inspection: CustomModInspection,
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    enabled: bool,
    platforms: Option<Vec<String>>,
    source_path: Option<String>,
    pending_sync: bool,
) -> (StoredLocalModMetadata, serde_json::Value) {
    let display_name = sanitize_mod_slug(name, &inspection.suggested_name);
    let author = sanitize_mod_slug(author, &inspection.suggested_author);
    let version_number = sanitize_version(version, &inspection.suggested_version);
    let full_name = format!("{}-{}-{}", author, display_name, version_number);
    let description = inspection.suggested_description.clone();
    let platforms = safe_platforms(platforms);
    let metadata = StoredLocalModMetadata {
        local_id: local_id.clone(),
        full_name: full_name.clone(),
        display_name: display_name.clone(),
        author: author.clone(),
        version_number: version_number.clone(),
        description: description.clone(),
        readme: inspection.readme.clone(),
        icon_data_url: inspection.icon_data_url.clone(),
        file_name: inspection.file_name.clone(),
        file_size: inspection.file_size,
        sha256: inspection.sha256.clone(),
        manifest_sha256: inspection.manifest_sha256.clone(),
        content_fingerprint: inspection.content_fingerprint.clone(),
        source_path: source_path.clone(),
        platforms: platforms.clone(),
        imported_at: now_millis(),
        security_report: inspection.security_report.clone(),
    };

    let mod_value = serde_json::json!({
        "uuid4": local_id,
        "fullName": full_name,
        "versionNumber": version_number,
        "enabled": enabled,
        "source": "local",
        "localId": metadata.local_id,
        "displayName": display_name,
        "author": author,
        "description": description,
        "readme": metadata.readme,
        "iconUrl": metadata.icon_data_url,
        "fileName": metadata.file_name,
        "fileSize": metadata.file_size,
        "sha256": metadata.sha256,
        "manifestSha256": metadata.manifest_sha256,
        "contentFingerprint": metadata.content_fingerprint,
        "sourcePath": metadata.source_path,
        "platforms": platforms,
        "securityReport": metadata.security_report,
        "pending_sync": pending_sync
    });

    (metadata, mod_value)
}

fn write_local_mod_metadata(
    dir: &std::path::Path,
    metadata: &StoredLocalModMetadata,
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(metadata).map_err(|e| e.to_string())?;
    fs::write(dir.join("metadata.json"), content).map_err(|e| e.to_string())
}

fn read_local_mod_metadata(dir: &std::path::Path) -> Option<StoredLocalModMetadata> {
    fs::read_to_string(dir.join("metadata.json"))
        .ok()
        .and_then(|content| serde_json::from_str::<StoredLocalModMetadata>(&content).ok())
}

fn prepare_custom_mod_source_payload(
    source_path: &std::path::Path,
    temp_label: &str,
) -> Result<(std::path::PathBuf, bool, CustomModInspection), String> {
    let source_metadata = fs::metadata(source_path).map_err(|e| e.to_string())?;
    if source_metadata.is_dir() {
        let temp_payload_path = temp_custom_mod_zip_path(temp_label);
        if let Err(err) = package_custom_mod_folder_to_zip(source_path, &temp_payload_path) {
            let _ = fs::remove_file(&temp_payload_path);
            return Err(err);
        }
        let display_name = source_path
            .file_name()
            .map(|value| value.to_string_lossy().to_string());
        let inspection =
            match inspect_custom_mod_file_with_name(&temp_payload_path, display_name.clone()) {
                Ok(inspection) => inspection,
                Err(err) => {
                    let _ = fs::remove_file(&temp_payload_path);
                    return Err(err);
                }
            };
        Ok((temp_payload_path, true, inspection))
    } else if source_metadata.is_file() {
        let inspection = inspect_custom_mod_file(source_path)?;
        Ok((source_path.to_path_buf(), false, inspection))
    } else {
        Err("Please choose a custom mod folder or .zip/.r2z archive.".to_string())
    }
}

fn copy_payload_with_hash_limit<R: Read>(
    mut reader: R,
    target_path: &std::path::Path,
) -> Result<(u64, String), String> {
    check_custom_mod_cancelled()?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut out = fs::File::create(target_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        check_custom_mod_cancelled()?;
        let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "Payload size accounting overflowed.".to_string())?;
        if total > CUSTOM_MOD_MAX_ARCHIVE_BYTES {
            return Err(format!(
                "Custom mod payload is larger than {} MB.",
                CUSTOM_MOD_MAX_ARCHIVE_BYTES / 1024 / 1024
            ));
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
    }
    Ok((total, format!("{:x}", hasher.finalize())))
}

fn copy_file_with_cancel(
    source_path: &std::path::Path,
    target_path: &std::path::Path,
) -> Result<u64, String> {
    check_custom_mod_cancelled()?;
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut input = fs::File::open(source_path).map_err(|e| e.to_string())?;
    let mut output = fs::File::create(target_path).map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        check_custom_mod_cancelled()?;
        let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "Payload size accounting overflowed.".to_string())?;
        if total > CUSTOM_MOD_MAX_ARCHIVE_BYTES {
            return Err(format!(
                "Custom mod payload is larger than {} MB.",
                CUSTOM_MOD_MAX_ARCHIVE_BYTES / 1024 / 1024
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
    }
    Ok(total)
}

#[cfg(test)]
mod custom_mod_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_fixture_zip(entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "r2modmac-custom-mod-test-{}-{}.zip",
            now_millis(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let file = fs::File::create(&path).expect("create fixture zip");
        let mut zip = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (name, data) in entries {
            zip.start_file(*name, options).expect("start fixture entry");
            zip.write_all(data).expect("write fixture entry");
        }
        zip.finish().expect("finish fixture zip");
        path
    }

    fn write_fixture_folder(entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-custom-mod-folder-test-{}-{}",
            now_millis(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create fixture folder");

        for (name, data) in entries {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create fixture parent");
            }
            fs::write(path, data).expect("write fixture file");
        }

        dir
    }

    #[test]
    fn inspect_custom_mod_accepts_valid_archive() {
        let manifest = br#"{"name":"SafeMod","version_number":"1.2.3","dependencies":[]}"#;
        let path = write_fixture_zip(&[
            ("manifest.json", manifest),
            ("BepInEx/plugins/SafeMod/SafeMod.dll", b"dll bytes"),
        ]);

        let inspection = inspect_custom_mod_file(&path).expect("valid custom mod");
        assert_eq!(inspection.suggested_name, "SafeMod");
        assert_eq!(inspection.suggested_version, "1.2.3");
        assert_eq!(inspection.security_report.total_files, 2);
        assert_eq!(inspection.security_report.risk_level, "medium");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn inspect_custom_mod_reads_readme_icon_and_description() {
        let manifest = br#"{"name":"RichMod","version_number":"4.5.6","description":"Manifest description","dependencies":[]}"#;
        let path = write_fixture_zip(&[
            ("manifest.json", manifest),
            ("README.weird", b"# Rich Mod\n\nCustom docs"),
            ("icon.png", b"not-really-a-png-but-small"),
            ("plugins/RichMod.dll", b"dll bytes"),
        ]);

        let inspection = inspect_custom_mod_file(&path).expect("rich custom mod");
        assert_eq!(inspection.suggested_name, "RichMod");
        assert_eq!(inspection.suggested_version, "4.5.6");
        assert_eq!(
            inspection.suggested_description.as_deref(),
            Some("Manifest description")
        );
        assert_eq!(
            inspection.readme.as_deref(),
            Some("# Rich Mod\n\nCustom docs")
        );
        assert!(inspection
            .icon_data_url
            .as_deref()
            .unwrap_or_default()
            .starts_with("data:image/png;base64,"));
        assert!(inspection.manifest_sha256.is_some());
        assert!(!inspection.content_fingerprint.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn inspect_custom_mod_accepts_mod_folder() {
        let manifest = br#"{"name":"FolderMod","version_number":"2.3.4","dependencies":[]}"#;
        let path = write_fixture_folder(&[
            ("manifest.json", manifest),
            ("README.md", b"folder mod"),
            ("plugins/FolderMod.dll", b"dll bytes"),
        ]);

        let inspection = inspect_custom_mod_path(&path).expect("valid custom mod folder");
        assert_eq!(
            inspection.file_name,
            path.file_name().unwrap().to_string_lossy().to_string()
        );
        assert_eq!(inspection.suggested_name, "FolderMod");
        assert_eq!(inspection.suggested_version, "2.3.4");
        assert_eq!(inspection.security_report.total_files, 3);

        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn inspect_custom_mod_rejects_zip_slip_paths() {
        let path = write_fixture_zip(&[
            (
                "manifest.json",
                br#"{"name":"Bad","version_number":"1.0.0"}"#,
            ),
            ("../../outside.txt", b"bad"),
        ]);

        let err = inspect_custom_mod_file(&path).expect_err("zip slip should fail");
        assert!(err.contains("unsafe archive path"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn inspect_custom_mod_rejects_case_insensitive_duplicates() {
        let path = write_fixture_zip(&[
            (
                "manifest.json",
                br#"{"name":"Dup","version_number":"1.0.0"}"#,
            ),
            ("plugins/Mod/File.dll", b"one"),
            ("plugins/mod/file.dll", b"two"),
        ]);

        let err = inspect_custom_mod_file(&path).expect_err("duplicate paths should fail");
        assert!(err.contains("duplicate archive path"));

        let _ = fs::remove_file(path);
    }
}

fn is_bepinex_shell_script(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".sh") && lower.contains("bepinex")
}

fn is_balatro_lovely_mod(mod_name: &str) -> bool {
    extract_mod_key(mod_name) == "thunderstore-lovely"
}

fn is_balatro_steamodded_mod(mod_name: &str) -> bool {
    extract_mod_key(mod_name) == "steamopollys-steamodded"
}

fn balatro_target_folder_name(mod_name: &str) -> String {
    if is_balatro_steamodded_mod(mod_name) {
        "smods".to_string()
    } else {
        mod_name.to_string()
    }
}

fn set_script_executable(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .map_err(|e| format!("Failed to inspect script permissions: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)
            .map_err(|e| format!("Failed to set script executable bit: {}", e))?;
    }

    Ok(())
}

fn normalize_ini_bool(value: Option<String>, default: bool) -> &'static str {
    match value
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("true" | "1" | "yes" | "on") => "true",
        Some("false" | "0" | "no" | "off") => "false",
        _ if default => "true",
        _ => "false",
    }
}

fn extract_ini_value(content: &str, keys: &[&str]) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with(';')
            || trimmed.starts_with('[')
        {
            return None;
        }

        let (key, value) = trimmed.split_once('=')?;
        if keys
            .iter()
            .any(|needle| key.trim().eq_ignore_ascii_case(needle))
        {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

pub(crate) fn normalize_macos_doorstop_config_file(path: &std::path::Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let dll_search_path_override = extract_ini_value(
        &content,
        &["dllSearchPathOverride", "dll_search_path_override"],
    )
    .unwrap_or_default();
    let ignore_disable_switch = normalize_ini_bool(
        extract_ini_value(&content, &["ignoreDisableSwitch", "ignore_disable_switch"]),
        false,
    );
    let redirect_output_log = "true";

    let normalized = format!(
        "[UnityDoorstop]\n\
# Specifies whether assembly executing is enabled\n\
enabled=true\n\
# Specifies the path (absolute, or relative to the game's exe) to the DLL/EXE that should be executed by Doorstop\n\
targetAssembly=BepInEx\\core\\BepInEx.Preloader.dll\n\
# Specifies whether Unity's output log should be redirected to <current folder>\\output_log.txt\n\
redirectOutputLog={redirect_output_log}\n\
# If enabled, DOORSTOP_DISABLE env var value is ignored\n\
# USE THIS ONLY WHEN ASKED TO OR YOU KNOW WHAT THIS MEANS\n\
ignoreDisableSwitch={ignore_disable_switch}\n\
# Overrides default Mono DLL search path\n\
# Sometimes it is needed to instruct Mono to seek its assemblies from a different path\n\
# (e.g. mscorlib is stripped in original game)\n\
dllSearchPathOverride={dll_search_path_override}\n"
    );

    if content.replace("\r\n", "\n") != normalized {
        fs::write(path, normalized).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn detect_lovely_runtime_in_zip<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> (bool, bool) {
    let mut has_macos_runtime = false;
    let mut has_windows_runtime = false;

    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let Some(name) = normalize_zip_entry_name(file.name()) else {
                continue;
            };
            let name = name.to_lowercase();
            if name.ends_with("run_lovely_macos.sh") || name.ends_with("liblovely.dylib") {
                has_macos_runtime = true;
            }
            if name.ends_with("version.dll") {
                has_windows_runtime = true;
            }
        }
    }

    (has_macos_runtime, has_windows_runtime)
}

fn extract_zip_directory_to_target<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_dir: &std::path::Path,
) -> Result<(), String> {
    if target_dir.exists() {
        let _ = fs::remove_dir_all(target_dir);
    }
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match normalize_zip_entry_path(file.name()) {
            Some(path) => target_dir.join(path),
            None => continue,
        };

        if zip_entry_is_dir(file.name()) {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn normalize_regular_mod_entry(
    relative_path: &std::path::Path,
    mod_name: &str,
) -> Option<std::path::PathBuf> {
    fn is_regular_mod_anchor(component: &str) -> bool {
        matches!(
            component.to_lowercase().as_str(),
            "bepinex"
                | "plugins"
                | "patchers"
                | "core"
                | "config"
                | "monomod"
                | "doorstop_libs"
                | "doorstop_config.ini"
                | "libdoorstop.dylib"
                | "run_bepinex.sh"
                | "winhttp.dll"
        )
    }

    let mut normalized = relative_path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if normalized.is_empty() {
        return None;
    }

    if let Some(anchor_index) = normalized
        .iter()
        .position(|component| is_regular_mod_anchor(component))
    {
        if anchor_index > 0 {
            normalized = normalized.split_off(anchor_index);
        }
    }

    let first = normalized[0].to_lowercase();
    let remainder = normalized.iter().skip(1).cloned().collect::<Vec<_>>();

    match first.as_str() {
        "bepinex" => {
            let mut root = std::path::PathBuf::from("BepInEx");
            for part in &remainder {
                root.push(part);
            }
            return Some(root);
        }
        "plugins" | "patchers" | "core" | "config" | "monomod" => {
            let mut root = std::path::PathBuf::from("BepInEx");
            root.push(&first);
            for part in &remainder {
                root.push(part);
            }
            return Some(root);
        }
        "doorstop_libs" => {
            let mut root = std::path::PathBuf::from("doorstop_libs");
            for part in &remainder {
                root.push(part);
            }
            return Some(root);
        }
        "doorstop_config.ini" | "libdoorstop.dylib" | "run_bepinex.sh" => {
            return Some(std::path::PathBuf::from(&normalized[0]));
        }
        _ => {
            let mut fallback = std::path::PathBuf::from("BepInEx");
            fallback.push("plugins");
            fallback.push(mod_name);
            for part in &normalized {
                fallback.push(part);
            }
            return Some(fallback);
        }
    }
}

fn get_profile_game_identifier(app: &AppHandle, profile_id: &str) -> Option<String> {
    let profiles_path = crate::utils::paths::app_data_dir(app).ok()?.join("profiles.json");
    let profiles_data = fs::read_to_string(profiles_path).ok()?;
    let profiles: Vec<serde_json::Value> = serde_json::from_str(&profiles_data).ok()?;
    profiles
        .iter()
        .find(|profile| profile["id"].as_str() == Some(profile_id))
        .and_then(|profile| profile["gameIdentifier"].as_str())
        .map(|s| s.to_string())
}

fn profile_is_vanilla(app: &AppHandle, profile_id: &str) -> bool {
    let profiles_path = match crate::utils::paths::app_data_dir(app) {
        Ok(path) => path.join("profiles.json"),
        Err(_) => return false,
    };

    let Ok(profiles_data) = fs::read_to_string(profiles_path) else {
        return false;
    };

    let Ok(profiles) = serde_json::from_str::<Vec<serde_json::Value>>(&profiles_data) else {
        return false;
    };

    profiles
        .iter()
        .find(|profile| profile["id"].as_str() == Some(profile_id))
        .and_then(|profile| profile["is_vanilla"].as_bool())
        .unwrap_or(false)
}

fn remap_disabled_macos_runtime_path(
    relative_path: &std::path::Path,
    use_disabled_runtime: bool,
) -> std::path::PathBuf {
    if !use_disabled_runtime {
        return relative_path.to_path_buf();
    }

    let mut components = relative_path.components();
    let Some(std::path::Component::Normal(first)) = components.next() else {
        return relative_path.to_path_buf();
    };

    let first = first.to_string_lossy().to_string();
    let mut mapped = match first.as_str() {
        "BepInEx" => std::path::PathBuf::from("BepInEx_DISABLED"),
        "doorstop_libs" => std::path::PathBuf::from("doorstop_libs_DISABLED"),
        "doorstop_config.ini" => std::path::PathBuf::from("doorstop_config.ini_DISABLED"),
        "libdoorstop.dylib" => std::path::PathBuf::from("libdoorstop.dylib_DISABLED"),
        _ => std::path::PathBuf::from(first),
    };

    for component in components {
        mapped.push(component.as_os_str());
    }

    mapped
}

fn runtime_bepinex_dir_name(use_disabled_runtime: bool) -> &'static str {
    if use_disabled_runtime {
        "BepInEx_DISABLED"
    } else {
        "BepInEx"
    }
}

fn runtime_plugins_dir(
    game_dir: &std::path::Path,
    use_disabled_runtime: bool,
) -> std::path::PathBuf {
    game_dir
        .join(runtime_bepinex_dir_name(use_disabled_runtime))
        .join("plugins")
}

fn is_probably_risk_of_rain_2_game_dir(game_dir: &std::path::Path) -> bool {
    let folder_name = game_dir
        .file_name()
        .map(|name| normalize_alnum(&name.to_string_lossy()))
        .unwrap_or_default();

    if folder_name == "riskofrain2" || folder_name == "ror2" {
        return true;
    }

    game_dir.join("Risk of Rain 2.exe").exists()
        || game_dir.join("RoR2.exe").exists()
        || game_dir.join("Risk of Rain 2_Data").is_dir()
}

fn should_install_ror2_crossover_newtonsoft_compat(
    game_dir: &std::path::Path,
    target_is_macos: bool,
) -> bool {
    cfg!(target_os = "macos") && !target_is_macos && is_probably_risk_of_rain_2_game_dir(game_dir)
}

fn extract_newtonsoft_json_dll_from_nupkg(nupkg_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(nupkg_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
        format!(
            "Downloaded Newtonsoft.Json package is not a valid nupkg: {}",
            e
        )
    })?;
    let mut dll = archive
        .by_name(NEWTONSOFT_JSON_NETSTANDARD20_ENTRY)
        .map_err(|_| {
            format!(
                "Newtonsoft.Json {} package did not contain {}",
                NEWTONSOFT_JSON_VERSION, NEWTONSOFT_JSON_NETSTANDARD20_ENTRY
            )
        })?;
    let mut bytes = Vec::new();
    std::io::copy(&mut dll, &mut bytes)
        .map_err(|e| format!("Failed to extract Newtonsoft.Json.dll: {}", e))?;
    Ok(bytes)
}

fn newtonsoft_json_nuget_url() -> String {
    format!(
        "https://api.nuget.org/v3-flatcontainer/newtonsoft.json/{0}/newtonsoft.json.{0}.nupkg",
        NEWTONSOFT_JSON_VERSION
    )
}

async fn download_newtonsoft_json_dll() -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to build NuGet client: {}", e))?;
    let url = newtonsoft_json_nuget_url();
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download Newtonsoft.Json: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Newtonsoft.Json download request failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read Newtonsoft.Json package: {}", e))?;

    extract_newtonsoft_json_dll_from_nupkg(bytes.as_ref())
}

async fn prepare_ror2_crossover_newtonsoft_compat(
    game_dir: &std::path::Path,
    mod_name: &str,
    target_is_macos: bool,
) -> Result<Vec<RuntimeCompatAsset>, String> {
    if !should_install_ror2_crossover_newtonsoft_compat(game_dir, target_is_macos) {
        return Ok(Vec::new());
    }

    let relative_path = std::path::PathBuf::from(ROR2_CROSSOVER_NEWTONSOFT_TARGET);
    if game_dir.join(&relative_path).exists() {
        return Ok(Vec::new());
    }

    eprintln!(
        "[install_mod] Risk of Rain 2 compatibility: installing Newtonsoft.Json {} for CrossOver/Wine runtime",
        NEWTONSOFT_JSON_VERSION
    );

    let compat_required = extract_mod_key(mod_name) == "levteam-macoshealthbarsfix";
    let bytes = match download_newtonsoft_json_dll().await {
        Ok(bytes) => bytes,
        Err(error) if compat_required => return Err(error),
        Err(error) => {
            eprintln!(
                "[install_mod] Risk of Rain 2 compatibility: could not install Newtonsoft.Json helper: {}",
                error
            );
            return Ok(Vec::new());
        }
    };
    Ok(vec![RuntimeCompatAsset {
        relative_path,
        bytes,
        label: "Newtonsoft.Json",
    }])
}

fn write_runtime_compat_assets(
    target_root: &std::path::Path,
    assets: &[RuntimeCompatAsset],
    use_disabled_runtime: bool,
) -> Result<(), String> {
    for asset in assets {
        let relative_path =
            remap_disabled_macos_runtime_path(&asset.relative_path, use_disabled_runtime);
        let outpath = target_root.join(relative_path);
        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&outpath, &asset.bytes).map_err(|e| {
            format!(
                "Failed to write {} compatibility asset to {}: {}",
                asset.label,
                outpath.display(),
                e
            )
        })?;
    }

    Ok(())
}

fn extract_regular_mod_to_root<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_root: &std::path::Path,
    mod_name: &str,
    use_disabled_runtime: bool,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let enclosed = match normalize_zip_entry_path(file.name()) {
            Some(path) => path,
            None => continue,
        };
        let Some(relative_target) = normalize_regular_mod_entry(&enclosed, mod_name) else {
            continue;
        };
        let outpath = target_root.join(remap_disabled_macos_runtime_path(
            &relative_target,
            use_disabled_runtime,
        ));

        if zip_entry_is_dir(file.name()) {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            continue;
        }

        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn collect_regular_mod_files<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    mod_name: &str,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut files = Vec::new();

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        if zip_entry_is_dir(file.name()) {
            continue;
        }

        let enclosed = match normalize_zip_entry_path(file.name()) {
            Some(path) => path,
            None => continue,
        };
        let Some(relative_target) = normalize_regular_mod_entry(&enclosed, mod_name) else {
            continue;
        };
        files.push(relative_target);
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn extract_lovely_zip_to_game_root<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    game_dir: &std::path::Path,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let file_name = normalize_zip_entry_path(file.name())
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .or_else(|| {
                std::path::Path::new(file.name())
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .unwrap_or_default();
        let lower = file_name.to_lowercase();
        if lower != "run_lovely_macos.sh" && lower != "liblovely.dylib" {
            continue;
        }

        let outpath = game_dir.join(&file_name);
        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        if lower == "run_lovely_macos.sh" {
            set_script_executable(&outpath)?;
        }
    }

    Ok(())
}

fn extract_lovely_tarball_to_game_root(
    bytes: &[u8],
    game_dir: &std::path::Path,
) -> Result<(), String> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let file_name = entry
            .path()
            .map_err(|e| e.to_string())?
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let lower = file_name.to_lowercase();

        if lower != "run_lovely_macos.sh" && lower != "liblovely.dylib" {
            continue;
        }

        let outpath = game_dir.join(&file_name);
        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        if lower == "run_lovely_macos.sh" {
            set_script_executable(&outpath)?;
        }
    }

    Ok(())
}

fn lovely_asset_name_for_current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "lovely-aarch64-apple-darwin.tar.gz",
        _ => "lovely-x86_64-apple-darwin.tar.gz",
    }
}

async fn download_official_lovely_runtime(version: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to build Lovely client: {}", e))?;
    let desired_asset = lovely_asset_name_for_current_arch();
    let exact_tag = format!("v{}", version);

    let release_urls = [
        format!(
            "https://api.github.com/repos/ethangreen-dev/lovely-injector/releases/tags/{}",
            exact_tag
        ),
        "https://api.github.com/repos/ethangreen-dev/lovely-injector/releases/latest".to_string(),
    ];

    for release_url in release_urls {
        let response = match client.get(&release_url).send().await {
            Ok(response) => response,
            Err(_) => continue,
        };
        if !response.status().is_success() {
            continue;
        }

        let release = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse Lovely release metadata: {}", e))?;

        if let Some(download_url) = release["assets"].as_array().and_then(|assets| {
            assets.iter().find_map(|asset| {
                let name = asset["name"].as_str()?;
                if name.eq_ignore_ascii_case(desired_asset) {
                    asset["browser_download_url"]
                        .as_str()
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        }) {
            eprintln!(
                "[install_mod] Falling back to official Lovely runtime: {}",
                download_url
            );
            let bytes = client
                .get(&download_url)
                .send()
                .await
                .map_err(|e| format!("Failed to download Lovely runtime: {}", e))?
                .error_for_status()
                .map_err(|e| format!("Official Lovely runtime request failed: {}", e))?
                .bytes()
                .await
                .map_err(|e| format!("Failed to read Lovely runtime: {}", e))?;
            return Ok(bytes.to_vec());
        }
    }

    Err(format!(
        "Could not resolve the official macOS Lovely runtime for version {}",
        version
    ))
}

pub(crate) fn extract_version_number_from_full_name(full_name: &str) -> Option<String> {
    let tail = full_name.rsplit('-').next()?;
    if tail.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Some(tail.to_string())
    } else {
        None
    }
}

#[command]
pub async fn inspect_custom_mod(path: String) -> Result<serde_json::Value, String> {
    guard_custom_mod_rate_limit("inspect_custom_mod")?;
    let path = std::path::PathBuf::from(path);
    let inspection = inspect_custom_mod_path(&path)?;
    Ok(serde_json::to_value(inspection).map_err(|e| e.to_string())?)
}

#[command]
pub async fn cancel_custom_mod_import() -> Result<bool, String> {
    if !CUSTOM_MOD_IMPORT_ACTIVE.load(Ordering::SeqCst) {
        CUSTOM_MOD_IMPORT_CANCELLED.store(false, Ordering::SeqCst);
        return Ok(false);
    }
    CUSTOM_MOD_IMPORT_CANCELLED.store(true, Ordering::SeqCst);
    Ok(true)
}

#[command]
pub async fn import_custom_mod(
    app: AppHandle,
    profile_id: String,
    path: String,
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    platforms: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    guard_custom_mod_rate_limit("import_custom_mod")?;
    let _cancel_guard = begin_custom_mod_import();
    let source_path = std::path::PathBuf::from(path);
    let (source_payload_path, remove_source_payload, inspection) =
        prepare_custom_mod_source_payload(&source_path, "folder-import")?;
    if let Err(err) = check_custom_mod_cancelled() {
        if remove_source_payload {
            let _ = fs::remove_file(&source_payload_path);
        }
        return Err(err);
    }
    let local_id = make_local_mod_id(&inspection.sha256);
    let dir = local_mod_dir(&app, &profile_id, &local_id)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let payload_path = dir.join("payload.zip");
    if let Err(err) = copy_file_with_cancel(&source_payload_path, &payload_path) {
        let _ = fs::remove_dir_all(&dir);
        if remove_source_payload {
            let _ = fs::remove_file(&source_payload_path);
        }
        return Err(err);
    }
    if remove_source_payload {
        let _ = fs::remove_file(&source_payload_path);
    }
    if let Err(err) = check_custom_mod_cancelled() {
        let _ = fs::remove_dir_all(&dir);
        return Err(err);
    }
    let copied_hash = match hash_file(&payload_path) {
        Ok(hash) => hash,
        Err(err) => {
            let _ = fs::remove_dir_all(&dir);
            return Err(err);
        }
    };
    if copied_hash != inspection.sha256 {
        let _ = fs::remove_dir_all(&dir);
        return Err("Copied custom mod payload failed hash verification.".to_string());
    }

    let (metadata, mod_value) = build_local_mod_response(
        local_id,
        inspection.clone(),
        name,
        author,
        version,
        true,
        platforms,
        Some(source_path.to_string_lossy().to_string()),
        true,
    );
    write_local_mod_metadata(&dir, &metadata)?;

    Ok(serde_json::json!({
        "mod": mod_value,
        "inspection": inspection
    }))
}

#[command]
pub async fn refresh_local_mod_metadata(
    app: AppHandle,
    profile_id: String,
    local_id: String,
    source_path: Option<String>,
    enabled: Option<bool>,
) -> Result<serde_json::Value, String> {
    let dir = local_mod_dir(&app, &profile_id, &local_id)?;
    let payload_path = dir.join("payload.zip");
    let previous = read_local_mod_metadata(&dir);
    let resolved_source_path = source_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| {
            previous
                .as_ref()
                .and_then(|metadata| metadata.source_path.clone())
        });

    let mut staged_from_source = false;
    let inspection = if let Some(path) = resolved_source_path.as_deref() {
        let source = std::path::PathBuf::from(path);
        if source.exists() {
            let (prepared_payload, remove_prepared_payload, inspection) =
                prepare_custom_mod_source_payload(&source, "folder-refresh")?;
            let source_changed = previous
                .as_ref()
                .map(|metadata| metadata.sha256 != inspection.sha256)
                .unwrap_or(true);
            if source_changed {
                if let Err(err) = copy_file_with_cancel(&prepared_payload, &payload_path) {
                    if remove_prepared_payload {
                        let _ = fs::remove_file(prepared_payload);
                    }
                    return Err(err);
                }
                staged_from_source = true;
            }
            if remove_prepared_payload {
                let _ = fs::remove_file(prepared_payload);
            }
            inspection
        } else {
            inspect_custom_mod_file(&payload_path)?
        }
    } else {
        inspect_custom_mod_file(&payload_path)?
    };

    let existing_author = previous.as_ref().map(|metadata| metadata.author.clone());
    let existing_platforms = previous.as_ref().map(|metadata| metadata.platforms.clone());
    let (metadata, mut mod_value) = build_local_mod_response(
        local_id,
        inspection.clone(),
        None,
        existing_author,
        None,
        enabled.unwrap_or(true),
        existing_platforms,
        resolved_source_path,
        false,
    );

    let changed = previous
        .as_ref()
        .map(|old| {
            staged_from_source
                || old.full_name != metadata.full_name
                || old.version_number != metadata.version_number
                || old.description != metadata.description
                || old.readme != metadata.readme
                || old.icon_data_url != metadata.icon_data_url
                || old.sha256 != metadata.sha256
                || old.manifest_sha256 != metadata.manifest_sha256
                || old.content_fingerprint != metadata.content_fingerprint
                || old.source_path != metadata.source_path
        })
        .unwrap_or(true);
    mod_value["pending_sync"] = serde_json::json!(changed);
    write_local_mod_metadata(&dir, &metadata)?;

    Ok(serde_json::json!({
        "changed": changed,
        "mod": mod_value,
        "inspection": inspection
    }))
}

#[command]
pub async fn import_embedded_custom_mod(
    app: AppHandle,
    profile_id: String,
    archive_path: String,
    payload_path: String,
    name: Option<String>,
    author: Option<String>,
    version: Option<String>,
    enabled: Option<bool>,
    platforms: Option<Vec<String>>,
    expected_sha256: Option<String>,
) -> Result<serde_json::Value, String> {
    guard_custom_mod_rate_limit("import_embedded_custom_mod")?;
    let archive_file = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|e| e.to_string())?;
    let mut payload = archive
        .by_name(&payload_path)
        .map_err(|_| format!("Embedded local mod payload not found: {}", payload_path))?;
    if payload.size() > CUSTOM_MOD_MAX_ARCHIVE_BYTES {
        return Err(format!(
            "Embedded custom mod payload exceeds {} MB.",
            CUSTOM_MOD_MAX_ARCHIVE_BYTES / 1024 / 1024
        ));
    }

    let payload_name = std::path::Path::new(&payload_path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "payload.zip".to_string());
    let seed_hash = hash_bytes(format!("{}:{}", archive_path, payload_path).as_bytes());
    let local_id = make_local_mod_id(&seed_hash);
    let dir = local_mod_dir(&app, &profile_id, &local_id)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let staged_payload_path = dir.join("payload.zip");
    let (_copied_size, copied_hash) =
        copy_payload_with_hash_limit(&mut payload, &staged_payload_path)?;
    if let Some(expected) = expected_sha256
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !copied_hash.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_dir_all(&dir);
            return Err(
                "Embedded custom mod payload hash does not match export metadata.".to_string(),
            );
        }
    }

    let inspection = inspect_custom_mod_file(&staged_payload_path)?;
    if inspection.sha256 != copied_hash {
        let _ = fs::remove_dir_all(&dir);
        return Err("Embedded custom mod payload failed post-copy verification.".to_string());
    }
    let mut inspection = inspection;
    inspection.file_name = payload_name;

    let (metadata, mod_value) = build_local_mod_response(
        local_id,
        inspection.clone(),
        name,
        author,
        version,
        enabled.unwrap_or(true),
        platforms,
        None,
        false,
    );
    write_local_mod_metadata(&dir, &metadata)?;

    Ok(serde_json::json!({
        "mod": mod_value,
        "inspection": inspection
    }))
}

#[command]
pub async fn install_local_mod(
    app: AppHandle,
    profile_id: String,
    local_id: String,
    mod_name: String,
    game_path: String,
    use_profile_cache: Option<bool>,
) -> Result<serde_json::Value, String> {
    guard_custom_mod_rate_limit("install_local_mod")?;
    let dir = local_mod_dir(&app, &profile_id, &local_id)?;
    let payload_path = dir.join("payload.zip");
    let inspection = inspect_custom_mod_file(&payload_path)?;

    if let Ok(content) = fs::read_to_string(dir.join("metadata.json")) {
        if let Ok(metadata) = serde_json::from_str::<StoredLocalModMetadata>(&content) {
            if metadata.sha256 != inspection.sha256 {
                return Err(
                    "Local custom mod payload hash no longer matches its metadata.".to_string(),
                );
            }
        }
    }

    let bytes = fs::read(&payload_path).map_err(|e| e.to_string())?;
    install_mod_bytes(
        app,
        profile_id,
        mod_name,
        game_path,
        use_profile_cache,
        bytes,
    )
    .await
}

#[command]
pub async fn delete_local_mod_payload(
    app: AppHandle,
    profile_id: String,
    local_id: String,
) -> Result<bool, String> {
    let dir = local_mod_dir(&app, &profile_id, &local_id)?;
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn official_bepinex_version_candidates(thunderstore_version: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let parts: Vec<&str> = thunderstore_version.split('.').collect();

    if parts.len() == 3
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && parts[2].chars().all(|c| c.is_ascii_digit())
        && parts[2].len() == 4
    {
        if let (Ok(major), Ok(minor), Ok(patch_major), Ok(patch_minor)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2][0..2].parse::<u32>(),
            parts[2][2..4].parse::<u32>(),
        ) {
            if major == 5 && (minor, patch_major, patch_minor) < (4, 23, 5) {
                candidates.push("5.4.23.5".to_string());
            }
            candidates.push(format!(
                "{}.{}.{}.{}",
                major, minor, patch_major, patch_minor
            ));
            candidates.push(format!("{}.{}.{}", major, minor, patch_major));
            candidates.push(format!("{}.{}.{}.0", major, minor, patch_major));
        }
    }

    if candidates.is_empty() {
        if thunderstore_version.starts_with("5.") && thunderstore_version != "5.4.23.5" {
            candidates.push("5.4.23.5".to_string());
        }
        candidates.push(thunderstore_version.to_string());
    }

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped.contains(&candidate) {
            deduped.push(candidate);
        }
    }
    deduped
}

fn direct_bepinex_asset_candidates(official_version: &str) -> Vec<String> {
    let mut assets = vec![
        format!("BepInEx_macos_universal_{}.zip", official_version),
        format!("BepInEx_macos_x64_{}.zip", official_version),
        format!("BepInEx_unix_{}.zip", official_version),
    ];
    assets.dedup();
    assets
}

fn select_macos_bepinex_asset_url(release: &serde_json::Value) -> Option<String> {
    let assets = release["assets"].as_array()?;

    let collect_assets = || {
        assets
            .iter()
            .filter_map(|asset| {
                let name = asset["name"].as_str()?;
                let url = asset["browser_download_url"].as_str()?;
                Some((name.to_lowercase(), url.to_string()))
            })
            .collect::<Vec<_>>()
    };

    let asset_entries = collect_assets();
    asset_entries
        .iter()
        .find(|(name, _)| name.contains("macos_universal") && name.ends_with(".zip"))
        .or_else(|| {
            asset_entries
                .iter()
                .find(|(name, _)| name.contains("macos_x64") && name.ends_with(".zip"))
        })
        .or_else(|| {
            asset_entries
                .iter()
                .find(|(name, _)| name.contains("unix") && name.ends_with(".zip"))
        })
        .map(|(_, url)| url.clone())
}

async fn download_bepinex_release_asset(
    client: &reqwest::Client,
    api_url: &str,
    context: &str,
) -> Result<Option<Vec<u8>>, String> {
    let response = match client.get(api_url).send().await {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };
    if !response.status().is_success() {
        return Ok(None);
    }

    let release = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse {} release metadata: {}", context, e))?;
    let Some(download_url) = select_macos_bepinex_asset_url(&release) else {
        return Ok(None);
    };

    eprintln!(
        "[install_mod] Falling back to official macOS BepInEx runtime: {}",
        download_url
    );
    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download official BepInEx runtime: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Official BepInEx runtime request failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read official BepInEx runtime: {}", e))?;

    Ok(Some(bytes.to_vec()))
}

async fn download_official_macos_bepinex_pack(
    thunderstore_version: &str,
) -> Result<Vec<u8>, String> {
    let version_candidates = official_bepinex_version_candidates(thunderstore_version);
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to build GitHub client: {}", e))?;

    for official_version in &version_candidates {
        let tag = format!("v{}", official_version);
        let api_url = format!(
            "https://api.github.com/repos/BepInEx/BepInEx/releases/tags/{}",
            tag
        );

        if let Some(bytes) =
            download_bepinex_release_asset(&client, &api_url, &format!("BepInEx {}", tag)).await?
        {
            return Ok(bytes);
        }

        for asset_name in direct_bepinex_asset_candidates(official_version) {
            let direct_url = format!(
                "https://github.com/BepInEx/BepInEx/releases/download/{}/{}",
                tag, asset_name
            );
            if let Ok(response) = client.get(&direct_url).send().await {
                if response.status().is_success() {
                    eprintln!(
                        "[install_mod] Falling back to direct official macOS BepInEx asset: {}",
                        direct_url
                    );
                    let bytes = response
                        .bytes()
                        .await
                        .map_err(|e| format!("Failed to read official BepInEx asset: {}", e))?;
                    return Ok(bytes.to_vec());
                }
            }
        }
    }

    let releases_url = "https://api.github.com/repos/BepInEx/BepInEx/releases?per_page=20";
    if let Ok(response) = client.get(releases_url).send().await {
        if response.status().is_success() {
            let releases = response
                .json::<Vec<serde_json::Value>>()
                .await
                .map_err(|e| format!("Failed to parse BepInEx releases list: {}", e))?;

            for release in releases {
                let Some(tag_name) = release["tag_name"].as_str() else {
                    continue;
                };
                if !tag_name.starts_with("v5.") {
                    continue;
                }

                if let Some(download_url) = select_macos_bepinex_asset_url(&release) {
                    eprintln!(
                        "[install_mod] Falling back to latest compatible BepInEx 5 macOS runtime: {} ({})",
                        tag_name,
                        download_url
                    );
                    let bytes = client
                        .get(&download_url)
                        .send()
                        .await
                        .map_err(|e| format!("Failed to download fallback BepInEx runtime: {}", e))?
                        .error_for_status()
                        .map_err(|e| format!("Fallback BepInEx runtime request failed: {}", e))?
                        .bytes()
                        .await
                        .map_err(|e| format!("Failed to read fallback BepInEx runtime: {}", e))?;
                    return Ok(bytes.to_vec());
                }
            }
        }
    }

    Err(format!(
        "Could not resolve an official macOS BepInEx runtime for Thunderstore version {}",
        thunderstore_version
    ))
}

async fn download_official_macos_bepinex6_pack(
    thunderstore_version: &str,
    runtime_kind: &str,
) -> Result<Vec<u8>, String> {
    let build_number = thunderstore_version.split('.').nth(2).ok_or_else(|| {
        format!(
            "Could not parse BepInEx 6 build number from {}",
            thunderstore_version
        )
    })?;
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to build GitHub client: {}", e))?;

    let project_page = client
        .get("https://builds.bepinex.dev/projects/bepinex_be")
        .send()
        .await
        .map_err(|e| format!("Failed to query BepInEx build index: {}", e))?
        .error_for_status()
        .map_err(|e| format!("BepInEx build index request failed: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read BepInEx build index: {}", e))?;

    let build_path = format!("/projects/bepinex_be/{}/", build_number);
    let candidates: &[&str] = if runtime_kind == "il2cpp" {
        &["bepinex-unity.il2cpp-macos-x64", "bepinex_unityil2cpp_x64"]
    } else {
        &["bepinex-unity.mono-macos-x64", "bepinex_unitymono_unix"]
    };

    let href_re = regex::Regex::new(r#"href="([^"]+\.zip)""#)
        .map_err(|e| format!("Invalid BepInEx build page regex: {}", e))?;
    let mut hrefs: Vec<(String, String)> = href_re
        .captures_iter(&project_page)
        .filter_map(|captures| captures.get(1).map(|m| m.as_str().to_string()))
        .filter(|href| href.contains(&build_path))
        .map(|href| {
            let lower = href.to_lowercase();
            (href, lower)
        })
        .collect();

    hrefs.sort_by_key(|(_, lower)| {
        if runtime_kind == "il2cpp" {
            if lower.contains("macos-x64") {
                0
            } else {
                1
            }
        } else if lower.contains("macos-x64") {
            0
        } else if lower.contains("unitymono_unix") || lower.contains("mono-unix") {
            1
        } else {
            2
        }
    });

    for needle in candidates {
        for (href, lower) in &hrefs {
            if !lower.contains(needle) {
                continue;
            }

            let download_url = if href.starts_with("http") {
                href.clone()
            } else {
                format!("https://builds.bepinex.dev{}", href)
            };

            eprintln!(
                "[install_mod] Falling back to official macOS BepInEx 6 runtime: {}",
                download_url
            );

            let bytes = client
                .get(&download_url)
                .send()
                .await
                .map_err(|e| format!("Failed to download official BepInEx 6 runtime: {}", e))?
                .error_for_status()
                .map_err(|e| format!("Official BepInEx 6 runtime request failed: {}", e))?
                .bytes()
                .await
                .map_err(|e| format!("Failed to read official BepInEx 6 runtime: {}", e))?;

            let cursor = std::io::Cursor::new(bytes.as_ref());
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
                format!("Downloaded official BepInEx 6 runtime is not a zip: {}", e)
            })?;
            let (is_bepinex_pack, _) = detect_bepinex_structure(&mut archive);
            let (has_macos_loader, _) = detect_bepinex_pack_platform(&mut archive);

            if is_bepinex_pack && has_macos_loader {
                return Ok(bytes.to_vec());
            }
        }
    }

    Err(format!(
        "Could not resolve a valid BepInEx 6 {} macOS build for {}",
        runtime_kind, thunderstore_version
    ))
}

pub(crate) async fn download_official_macos_bepinex_runtime(
    thunderstore_version: &str,
    runtime_kind: &str,
) -> Result<Vec<u8>, String> {
    if thunderstore_version.starts_with("6.") {
        download_official_macos_bepinex6_pack(thunderstore_version, runtime_kind).await
    } else {
        download_official_macos_bepinex_pack(thunderstore_version).await
    }
}

pub(crate) fn extract_bepinex_pack_to_root<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_root: &std::path::Path,
    target_is_macos: bool,
    use_disabled_runtime: bool,
) -> Result<(), String> {
    let (is_bepinex_pack, bepinex_prefix) = detect_bepinex_structure(archive);
    if !is_bepinex_pack {
        return Err("Archive does not look like a BepInEx runtime".to_string());
    }

    let prefix = bepinex_prefix.unwrap_or_default();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = normalize_zip_entry_name(file.name()) else {
            continue;
        };
        let relative_path = if !prefix.is_empty() && name.starts_with(&prefix) {
            &name[prefix.len()..]
        } else {
            &name
        };

        if relative_path.is_empty() {
            continue;
        }

        let Some(normalized_relative_path) =
            normalize_bepinex_pack_entry(relative_path, target_is_macos)
        else {
            continue;
        };
        let outpath = target_root.join(remap_disabled_macos_runtime_path(
            &normalized_relative_path,
            target_is_macos && use_disabled_runtime,
        ));

        if zip_entry_is_dir(file.name()) {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
            if target_is_macos
                && normalized_relative_path == std::path::PathBuf::from("doorstop_config.ini")
            {
                normalize_macos_doorstop_config_file(&outpath)?;
            }
            if normalized_relative_path
                .file_name()
                .map(|name| is_bepinex_shell_script(&name.to_string_lossy()))
                .unwrap_or(false)
            {
                set_script_executable(&outpath)?;
            }
        }
    }

    Ok(())
}

fn normalize_macos_bepinex_runtime_overlay_entry(
    relative_path: &str,
) -> Option<std::path::PathBuf> {
    let normalized = normalize_bepinex_pack_entry(relative_path, true)?;
    let lower = normalized
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();

    if lower == "run_bepinex.sh"
        || lower == "libdoorstop.dylib"
        || lower == "doorstop_config.ini"
        || lower == "doorstop_libs"
        || lower.starts_with("doorstop_libs/")
    {
        Some(normalized)
    } else {
        None
    }
}

fn extract_macos_bepinex_runtime_overlay_to_root<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_root: &std::path::Path,
    use_disabled_runtime: bool,
) -> Result<(), String> {
    let (is_bepinex_pack, bepinex_prefix) = detect_bepinex_structure(archive);
    if !is_bepinex_pack {
        return Err("Archive does not look like a BepInEx runtime".to_string());
    }

    let prefix = bepinex_prefix.unwrap_or_default();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = normalize_zip_entry_name(file.name()) else {
            continue;
        };
        let relative_path = if !prefix.is_empty() && name.starts_with(&prefix) {
            &name[prefix.len()..]
        } else {
            &name
        };

        if relative_path.is_empty() {
            continue;
        }

        let Some(normalized_relative_path) =
            normalize_macos_bepinex_runtime_overlay_entry(relative_path)
        else {
            continue;
        };
        let outpath = target_root.join(remap_disabled_macos_runtime_path(
            &normalized_relative_path,
            use_disabled_runtime,
        ));

        if zip_entry_is_dir(file.name()) {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
            continue;
        }

        if normalized_relative_path == std::path::PathBuf::from("doorstop_config.ini")
            && outpath.exists()
        {
            normalize_macos_doorstop_config_file(&outpath)?;
            continue;
        }

        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        if normalized_relative_path == std::path::PathBuf::from("doorstop_config.ini") {
            normalize_macos_doorstop_config_file(&outpath)?;
        }
        if normalized_relative_path
            .file_name()
            .map(|name| is_bepinex_shell_script(&name.to_string_lossy()))
            .unwrap_or(false)
        {
            set_script_executable(&outpath)?;
        }
    }

    Ok(())
}

fn collect_bepinex_pack_files<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target_is_macos: bool,
) -> Result<Vec<std::path::PathBuf>, String> {
    let (is_bepinex_pack, bepinex_prefix) = detect_bepinex_structure(archive);
    if !is_bepinex_pack {
        return Err("Archive does not look like a BepInEx runtime".to_string());
    }

    let prefix = bepinex_prefix.unwrap_or_default();
    let mut files = Vec::new();

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = normalize_zip_entry_name(file.name()) else {
            continue;
        };
        if zip_entry_is_dir(file.name()) {
            continue;
        }

        let relative_path = if !prefix.is_empty() && name.starts_with(&prefix) {
            &name[prefix.len()..]
        } else {
            &name
        };

        if relative_path.is_empty() {
            continue;
        }

        let Some(normalized_relative_path) =
            normalize_bepinex_pack_entry(relative_path, target_is_macos)
        else {
            continue;
        };
        files.push(normalized_relative_path);
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_macos_bepinex_runtime_overlay_files<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Vec<std::path::PathBuf>, String> {
    let (is_bepinex_pack, bepinex_prefix) = detect_bepinex_structure(archive);
    if !is_bepinex_pack {
        return Err("Archive does not look like a BepInEx runtime".to_string());
    }

    let prefix = bepinex_prefix.unwrap_or_default();
    let mut files = Vec::new();

    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = normalize_zip_entry_name(file.name()) else {
            continue;
        };
        if zip_entry_is_dir(file.name()) {
            continue;
        }

        let relative_path = if !prefix.is_empty() && name.starts_with(&prefix) {
            &name[prefix.len()..]
        } else {
            &name
        };

        if relative_path.is_empty() {
            continue;
        }

        let Some(normalized_relative_path) =
            normalize_macos_bepinex_runtime_overlay_entry(relative_path)
        else {
            continue;
        };
        files.push(normalized_relative_path);
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn normalize_bepinex_pack_entry(
    relative_path: &str,
    target_is_macos: bool,
) -> Option<std::path::PathBuf> {
    let trimmed = relative_path.trim_start_matches("./").trim_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    let file_name = path.file_name()?.to_string_lossy().to_string();
    let lower_trimmed = trimmed.to_lowercase();
    let is_root_level = path.components().count() == 1;

    if lower_trimmed == "bepinex" || lower_trimmed.starts_with("bepinex/") {
        return Some(std::path::PathBuf::from(trimmed));
    }

    if lower_trimmed == "doorstop_libs" || lower_trimmed.starts_with("doorstop_libs/") {
        return Some(std::path::PathBuf::from(trimmed));
    }

    if lower_trimmed == "winhttp.dll" && target_is_macos {
        return None;
    }

    if matches!(
        lower_trimmed.as_str(),
        "doorstop_config.ini" | "libdoorstop.dylib" | "winhttp.dll"
    ) {
        return Some(std::path::PathBuf::from(file_name));
    }

    if is_root_level && is_bepinex_shell_script(&file_name) {
        return Some(std::path::PathBuf::from("run_bepinex.sh"));
    }

    if is_root_level && lower_trimmed.ends_with(".dylib") {
        return Some(std::path::PathBuf::from(file_name));
    }

    None
}

fn extract_mod_key(input: &str) -> String {
    let parts: Vec<&str> = input.split('-').collect();
    if parts.len() >= 2 {
        format!("{}-{}", parts[0].to_lowercase(), parts[1].to_lowercase())
    } else {
        input.to_lowercase()
    }
}

fn normalize_alnum(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect()
}

fn folder_matches_mod(folder_name: &str, mod_query: &str, mod_key: &str) -> bool {
    let folder_lower = folder_name.to_lowercase();
    if folder_lower.contains(mod_query) || mod_query.contains(&folder_lower) {
        return true;
    }

    let folder_norm = normalize_alnum(folder_name);
    let query_norm = normalize_alnum(mod_query);
    if !folder_norm.is_empty()
        && !query_norm.is_empty()
        && (folder_norm.contains(&query_norm) || query_norm.contains(&folder_norm))
    {
        return true;
    }

    let folder_tokens = tokenize(folder_name);
    let query_tokens = tokenize(mod_query);
    let overlap = folder_tokens
        .iter()
        .filter(|token| query_tokens.iter().any(|q| q == *token))
        .count();
    if overlap >= 2 {
        return true;
    }

    extract_mod_key(folder_name) == mod_key
}

fn find_mod_entry_recursive(
    base: &std::path::Path,
    mod_name: &str,
    depth: usize,
) -> Option<std::path::PathBuf> {
    if depth == 0 || !base.exists() {
        return None;
    }

    let mod_query = mod_name.to_lowercase();
    let mod_key = extract_mod_key(&mod_query);

    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            if folder_matches_mod(&name, &mod_query, &mod_key) {
                return Some(path);
            }

            let file_type = entry.file_type().ok();
            let is_dir_like = file_type
                .as_ref()
                .map(|t| t.is_dir() || t.is_symlink())
                .unwrap_or(false);
            if is_dir_like {
                if let Some(found) = find_mod_entry_recursive(&path, mod_name, depth - 1) {
                    return Some(found);
                }
            }
        }
    }

    None
}

fn find_mod_folder_in(base: &std::path::Path, mod_name: &str) -> Option<String> {
    if !base.exists() {
        return None;
    }

    let mod_query = mod_name.to_lowercase();
    let mod_key = extract_mod_key(&mod_query);
    let mut fallback: Option<String> = None;

    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.filter_map(|e| e.ok()) {
            let folder_name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().ok();
            let is_container = file_type
                .as_ref()
                .map(|t| t.is_dir() || t.is_symlink())
                .unwrap_or(false);
            if !is_container {
                continue;
            }

            let folder_key = extract_mod_key(&folder_name);
            if folder_key == mod_key {
                return Some(folder_name);
            }

            if fallback.is_none() && folder_matches_mod(&folder_name, &mod_query, &mod_key) {
                fallback = Some(folder_name);
            }
        }
    }

    fallback
}

fn is_macos_game_dir(path: &std::path::Path) -> bool {
    if path
        .file_name()
        .map(|name| name.to_string_lossy().ends_with(".app"))
        .unwrap_or(false)
    {
        return true;
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".app"))
        })
        .unwrap_or(false)
}

fn find_macos_app_bundle(path: &std::path::Path) -> Option<std::path::PathBuf> {
    if path
        .file_name()
        .map(|name| name.to_string_lossy().ends_with(".app"))
        .unwrap_or(false)
    {
        return Some(path.to_path_buf());
    }

    fs::read_dir(path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|entry_path| {
            entry_path
                .file_name()
                .map(|name| name.to_string_lossy().ends_with(".app"))
                .unwrap_or(false)
        })
}

pub(crate) fn detect_unity_runtime_kind(game_dir: &std::path::Path) -> &'static str {
    let Some(app_bundle) = find_macos_app_bundle(game_dir) else {
        return "mono";
    };

    let data_dir = app_bundle.join("Contents").join("Resources").join("Data");
    if data_dir.join("Managed").is_dir() {
        "mono"
    } else {
        "il2cpp"
    }
}

fn detect_bepinex_pack_platform<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> (bool, bool) {
    let mut has_macos_loader = false;
    let mut has_windows_loader = false;

    for i in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(i) {
            let Some(name) = normalize_zip_entry_name(file.name()) else {
                continue;
            };
            let name = name.to_lowercase();
            let file_name = std::path::Path::new(&name)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if name.ends_with(".dylib") {
                has_macos_loader = true;
            }

            if is_bepinex_shell_script(&file_name) {
                let mut text = String::new();
                let _ = std::io::Read::read_to_string(&mut file, &mut text);
                let lower = text.to_lowercase();
                if lower.contains("dyld_insert_libraries")
                    && lower.contains("dylib")
                    && (lower.contains("doorstop_enable") || lower.contains("doorstop_enabled"))
                {
                    has_macos_loader = true;
                }
            }

            if name.ends_with("winhttp.dll") {
                has_windows_loader = true;
            }
        }
    }

    (has_macos_loader, has_windows_loader)
}

#[command]
pub async fn install_mod(
    app: AppHandle,
    profile_id: String,
    download_url: String,
    mod_name: String,
    game_path: String,
    use_profile_cache: Option<bool>,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Download failed with status {}", status));
    }

    let total_bytes = response.content_length();
    // Use the streaming body directly via chunk() instead of bytes_stream() so we
    // don't need the `stream` feature (which pulls in h2 streaming machinery).
    let mut body = response;
    let mut bytes: Vec<u8> = Vec::new();
    let mut downloaded: u64 = 0;
    let download_started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Ok(Some(next_chunk)) = body.chunk().await {
        let chunk = next_chunk;
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);

        let now = std::time::Instant::now();
        if now.duration_since(last_emit).as_millis() >= 120 {
            let elapsed = download_started.elapsed().as_secs_f64().max(0.001);
            let speed_bps = downloaded as f64 / elapsed;
            let progress_percent = total_bytes
                .map(|total| {
                    ((downloaded as f64 / total as f64) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8
                })
                .unwrap_or(0);

            let _ = app.emit(
                "mod-download-progress",
                serde_json::json!({
                    "mod_name": mod_name.as_str(),
                    "downloaded_bytes": downloaded,
                    "total_bytes": total_bytes,
                    "speed_bps": speed_bps,
                    "progress_percent": progress_percent,
                    "done": false
                }),
            );
            last_emit = now;
        }
    }

    let elapsed = download_started.elapsed().as_secs_f64().max(0.001);
    let final_speed_bps = downloaded as f64 / elapsed;
    let _ = app.emit(
        "mod-download-progress",
        serde_json::json!({
            "mod_name": mod_name.as_str(),
            "downloaded_bytes": downloaded,
            "total_bytes": total_bytes,
            "speed_bps": final_speed_bps,
            "progress_percent": 100,
            "done": true
        }),
    );

    install_mod_bytes(
        app,
        profile_id,
        mod_name,
        game_path,
        use_profile_cache,
        bytes,
    )
    .await
}

async fn install_mod_bytes(
    app: AppHandle,
    profile_id: String,
    mod_name: String,
    game_path: String,
    use_profile_cache: Option<bool>,
    bytes: Vec<u8>,
) -> Result<serde_json::Value, String> {
    // Install DIRECTLY to game folder
    let game_dir = std::path::Path::new(&game_path);
    let target_is_macos = is_macos_game_dir(game_dir);
    let game_identifier = get_profile_game_identifier(&app, &profile_id).unwrap_or_default();
    let target_is_balatro = crate::models::shared::is_balatro_identifier(&game_identifier) || is_balatro_game_path(game_dir);
    let target_is_outerwilds = crate::models::shared::is_outerwilds_identifier(&game_identifier)
        || crate::models::shared::is_outerwilds_game_path(game_dir);
    let install_into_disabled_runtime =
        target_is_macos && !target_is_balatro && !target_is_outerwilds && profile_is_vanilla(&app, &profile_id);

    eprintln!(
        "[install_mod] Installing {} directly to game: {:?}",
        mod_name, game_dir
    );

    let mut runtime_bytes = bytes;

    if target_is_outerwilds {
        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

        // Detect if this is the OWML loader itself (contains OWML.Launcher.exe at root or in a subdirectory)
        let is_owml_loader = (0..archive.len()).any(|i| {
            archive.by_index_raw(i).ok()
                .and_then(|f| normalize_zip_entry_name(f.name()))
                .map(|name| {
                    let lower = name.to_lowercase();
                    lower.ends_with("owml.launcher.exe")
                })
                .unwrap_or(false)
        });

        let owml_root = game_dir.join("OWML");

        if is_owml_loader {
            // Extract OWML loader to <game_path>/OWML/
            fs::create_dir_all(&owml_root).map_err(|e| e.to_string())?;
            let cursor = std::io::Cursor::new(&runtime_bytes);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            extract_zip_directory_to_target(&mut archive, &owml_root)?;

            if use_profile_cache.unwrap_or(false) {
                let profile_dir = crate::utils::paths::app_data_dir(&app)
                    .map_err(|e| e.to_string())?
                    .join("profiles")
                    .join(&profile_id);
                let cache_owml = profile_dir.join("OWML");
                fs::create_dir_all(&cache_owml).map_err(|e| e.to_string())?;
                let cursor = std::io::Cursor::new(&runtime_bytes);
                let mut cache_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
                let _ = extract_zip_directory_to_target(&mut cache_archive, &cache_owml);
            }

            eprintln!("[install_mod] Installed OWML loader to {:?}", owml_root);
            return Ok(serde_json::json!({ "success": true }));
        }

        // Regular OWML mod: read manifest.json from zip to get uniqueName, dependencies and nesting prefix
        let (unique_name, dependencies, manifest_prefix) = {
            let cursor = std::io::Cursor::new(&runtime_bytes);
            let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            let mut found_un: Option<String> = None;
            let mut found_deps: Vec<String> = Vec::new();
            let mut prefix_path: Option<String> = None;
            for i in 0..archive.len() {
                if let Ok(mut file) = archive.by_index(i) {
                    let name = file.name().to_string();
                    let normalized = normalize_zip_entry_name(&name);
                    if let Some(ref n) = normalized {
                        // Find manifest.json at any nesting level
                        let segments: Vec<&str> = n.split('/').collect();
                        if segments.last().map(|s| s.eq_ignore_ascii_case("manifest.json")).unwrap_or(false) {
                            let mut content = String::new();
                            use std::io::Read;
                            if file.read_to_string(&mut content).is_ok() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                                    if let Some(un) = v["uniqueName"].as_str() {
                                        found_un = Some(un.to_string());
                                    }
                                    if let Some(arr) = v["dependencies"].as_array() {
                                        for dep in arr {
                                            if let Some(dep_str) = dep.as_str() {
                                                found_deps.push(dep_str.to_string());
                                            }
                                        }
                                    }
                                    // Derive the prefix path of manifest.json
                                    let mut prefix = segments[0..segments.len()-1].join("/");
                                    if !prefix.is_empty() {
                                        prefix.push('/');
                                    }
                                    prefix_path = Some(prefix);
                                    
                                    if found_un.is_some() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (found_un, found_deps, prefix_path.unwrap_or_default())
        };

        let mod_folder_name = unique_name.unwrap_or_else(|| {
            // Fallback: derive folder name from mod_name (Author-ModName-Version -> ModName)
            let parts: Vec<&str> = mod_name.split('-').collect();
            if parts.len() >= 2 { parts[1].to_string() } else { mod_name.clone() }
        });

        let mod_dir = owml_root.join("Mods").join(&mod_folder_name);
        if mod_dir.exists() {
            let _ = fs::remove_dir_all(&mod_dir);
        }
        fs::create_dir_all(&mod_dir).map_err(|e| e.to_string())?;

        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name().to_string();
            let normalized = normalize_zip_entry_name(&name).unwrap_or(name);
            
            if normalized.starts_with(&manifest_prefix) {
                let relative_path_str = &normalized[manifest_prefix.len()..];
                if relative_path_str.is_empty() {
                    continue;
                }
                
                let outpath = mod_dir.join(relative_path_str);
                
                if zip_entry_is_dir(file.name()) {
                    fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
                } else {
                    if let Some(parent) = outpath.parent() {
                        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
                    std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                }
            }
        }

        if use_profile_cache.unwrap_or(false) {
            let profile_dir = crate::utils::paths::app_data_dir(&app)
                .map_err(|e| e.to_string())?
                .join("profiles")
                .join(&profile_id);
            let profile_mod_dir = profile_dir.join("OWML").join("Mods").join(&mod_folder_name);
            if profile_mod_dir.exists() {
                let _ = fs::remove_dir_all(&profile_mod_dir);
            }
            fs::create_dir_all(&profile_mod_dir).map_err(|e| e.to_string())?;
            
            let cursor = std::io::Cursor::new(&runtime_bytes);
            let mut cache_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            for i in 0..cache_archive.len() {
                let mut file = cache_archive.by_index(i).map_err(|e| e.to_string())?;
                let name = file.name().to_string();
                let normalized = normalize_zip_entry_name(&name).unwrap_or(name);
                
                if normalized.starts_with(&manifest_prefix) {
                    let relative_path_str = &normalized[manifest_prefix.len()..];
                    if relative_path_str.is_empty() {
                        continue;
                    }
                    
                    let outpath = profile_mod_dir.join(relative_path_str);
                    
                    if zip_entry_is_dir(file.name()) {
                        fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
                    } else {
                        if let Some(parent) = outpath.parent() {
                            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
                        std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                    }
                }
            }
        }

        eprintln!("[install_mod] Installed OWML mod '{}' to {:?}", mod_folder_name, mod_dir);
        return Ok(serde_json::json!({
            "success": true,
            "uniqueName": mod_folder_name,
            "dependencies": dependencies
        }));
    }

    if target_is_balatro {
        if is_balatro_lovely_mod(&mod_name) {
            let version_number = extract_version_number_from_full_name(&mod_name)
                .ok_or_else(|| format!("Could not parse Lovely version from {}", mod_name))?;
            let cursor = std::io::Cursor::new(&runtime_bytes);
            let mut archive_for_detect = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            let (has_macos_runtime, _has_windows_runtime) =
                detect_lovely_runtime_in_zip(&mut archive_for_detect);

            if has_macos_runtime {
                let cursor = std::io::Cursor::new(&runtime_bytes);
                let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
                extract_lovely_zip_to_game_root(&mut archive, game_dir)?;
            } else {
                runtime_bytes = download_official_lovely_runtime(&version_number).await?;
                extract_lovely_tarball_to_game_root(&runtime_bytes, game_dir)?;
            }

            dequarantine_recursive(game_dir);

            if use_profile_cache.unwrap_or(false) {
                let profile_dir = crate::utils::paths::app_data_dir(&app)
                    .map_err(|e| e.to_string())?
                    .join("profiles")
                    .join(&profile_id);
                let cache_dir = profile_dir.join("BalatroRoot");
                fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

                for file_name in ["run_lovely_macos.sh", "liblovely.dylib"] {
                    let src = game_dir.join(file_name);
                    if src.exists() {
                        let dst = cache_dir.join(file_name);
                        fs::copy(&src, &dst).map_err(|e| e.to_string())?;
                        if file_name.ends_with(".sh") {
                            set_script_executable(&dst)?;
                        }
                    }
                }
            }

            eprintln!("[install_mod] Successfully installed Lovely runtime for Balatro");
            return Ok(serde_json::json!({ "success": true }));
        }

        let mods_root = get_balatro_mods_dir().ok_or_else(|| {
            "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string()
        })?;
        fs::create_dir_all(&mods_root).map_err(|e| e.to_string())?;

        let target_folder = balatro_target_folder_name(&mod_name);
        let mod_dir = mods_root.join(&target_folder);
        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
        extract_zip_directory_to_target(&mut archive, &mod_dir)?;

        if use_profile_cache.unwrap_or(false) {
            let profile_dir = crate::utils::paths::app_data_dir(&app)
                .map_err(|e| e.to_string())?
                .join("profiles")
                .join(&profile_id);
            let profile_mod_dir = profile_dir
                .join("Balatro")
                .join("Mods")
                .join(&target_folder);
            let cursor = std::io::Cursor::new(&runtime_bytes);
            let mut cache_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            extract_zip_directory_to_target(&mut cache_archive, &profile_mod_dir)?;
        }

        return Ok(serde_json::json!({ "success": true }));
    }

    // Smart detection: Check if this is BepInEx framework (not just "BepInExPack/" prefix)
    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut archive_for_detect = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    let (mut is_bepinex_pack, _) = detect_bepinex_structure(&mut archive_for_detect);
    let (mut has_macos_loader, mut has_windows_loader) =
        detect_bepinex_pack_platform(&mut archive_for_detect);
    let mut macos_runtime_overlay_bytes: Option<Vec<u8>> = None;
    let runtime_compat_assets =
        prepare_ror2_crossover_newtonsoft_compat(game_dir, &mod_name, target_is_macos).await?;

    if is_bepinex_pack && target_is_macos {
        let version_number = extract_version_number_from_full_name(&mod_name);
        let should_overlay_official_runtime = !has_macos_loader;

        if should_overlay_official_runtime {
            let version_number = version_number
                .ok_or_else(|| format!("Could not parse BepInEx version from {}", mod_name))?;
            let runtime_kind = detect_unity_runtime_kind(game_dir);
            let fallback_runtime_bytes =
                download_official_macos_bepinex_runtime(&version_number, runtime_kind).await?;

            let cursor = std::io::Cursor::new(&fallback_runtime_bytes);
            let mut fallback_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            let (fallback_is_bepinex_pack, _) = detect_bepinex_structure(&mut fallback_archive);
            let (fallback_has_macos_loader, fallback_has_windows_loader) =
                detect_bepinex_pack_platform(&mut fallback_archive);

            if !fallback_is_bepinex_pack || !fallback_has_macos_loader {
                return Err("Downloaded official macOS BepInEx runtime looks invalid".to_string());
            }

            if has_macos_loader {
                eprintln!(
                    "[install_mod] Overlaying official macOS BepInEx loader over {}",
                    mod_name
                );
            }

            is_bepinex_pack = fallback_is_bepinex_pack;
            has_macos_loader = fallback_has_macos_loader;
            has_windows_loader = fallback_has_windows_loader;
            macos_runtime_overlay_bytes = Some(fallback_runtime_bytes);
        }
    }

    // Install to game folder
    let managed_files = {
        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut manifest_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

        let mut files = if is_bepinex_pack {
            collect_bepinex_pack_files(&mut manifest_archive, target_is_macos)?
        } else {
            collect_regular_mod_files(&mut manifest_archive, &mod_name)?
        };

        if let Some(overlay_bytes) = macos_runtime_overlay_bytes.as_ref() {
            let cursor = std::io::Cursor::new(overlay_bytes);
            let mut overlay_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            files.extend(collect_macos_bepinex_runtime_overlay_files(
                &mut overlay_archive,
            )?);
        }
        files.extend(
            runtime_compat_assets
                .iter()
                .map(|asset| asset.relative_path.clone()),
        );

        files.sort();
        files.dedup();
        files
            .into_iter()
            .map(|file| remap_disabled_macos_runtime_path(&file, install_into_disabled_runtime))
            .collect::<Vec<_>>()
    };
    let backed_up_files = if managed_files.is_empty() {
        Vec::new()
    } else {
        backup_existing_mod_files(
            &app,
            &profile_id,
            GAME_MANIFEST_SCOPE,
            &mod_name,
            game_dir,
            &managed_files,
        )?
    };

    let cursor = std::io::Cursor::new(&runtime_bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    if is_bepinex_pack {
        if !target_is_macos && has_macos_loader && !has_windows_loader {
            return Err("Detected a macOS-only BepInEx pack. Please use a Windows/CrossOver-compatible pack for this profile.".to_string());
        }

        eprintln!("[install_mod] Detected BepInExPack - installing to game root");
        extract_bepinex_pack_to_root(
            &mut archive,
            game_dir,
            target_is_macos,
            install_into_disabled_runtime,
        )?;

        if let Some(overlay_bytes) = macos_runtime_overlay_bytes.as_ref() {
            let cursor = std::io::Cursor::new(overlay_bytes);
            let mut overlay_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            extract_macos_bepinex_runtime_overlay_to_root(
                &mut overlay_archive,
                game_dir,
                install_into_disabled_runtime,
            )?;
        }

        if target_is_macos && !install_into_disabled_runtime {
            migrate_root_plugins_into_bepinex(game_dir)?;
            dequarantine_recursive(game_dir);
        }
    } else {
        extract_regular_mod_to_root(
            &mut archive,
            game_dir,
            &mod_name,
            install_into_disabled_runtime,
        )?;
        if target_is_macos && !install_into_disabled_runtime {
            migrate_root_plugins_into_bepinex(game_dir)?;
        }
    }

    if !runtime_compat_assets.is_empty() {
        write_runtime_compat_assets(
            game_dir,
            &runtime_compat_assets,
            install_into_disabled_runtime,
        )?;
    }

    if !managed_files.is_empty() {
        save_owned_mod_manifest(
            &app,
            &profile_id,
            GAME_MANIFEST_SCOPE,
            &mod_name,
            game_dir,
            &managed_files,
            &backed_up_files,
        )?;
    }

    // LEGACY MODE: Also save to profile cache folder
    if use_profile_cache.unwrap_or(false) {
        let profile_dir = crate::utils::paths::app_data_dir(&app)
            .map_err(|e| e.to_string())?
            .join("profiles")
            .join(&profile_id);
        eprintln!(
            "[install_mod] LEGACY: Also caching to profile: {:?}",
            profile_dir
        );

        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

        if is_bepinex_pack {
            // Cache BepInExPack to profile root
            extract_bepinex_pack_to_root(&mut archive, &profile_dir, target_is_macos, false)?;

            if let Some(overlay_bytes) = macos_runtime_overlay_bytes.as_ref() {
                let cursor = std::io::Cursor::new(overlay_bytes);
                let mut overlay_archive =
                    zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
                extract_macos_bepinex_runtime_overlay_to_root(
                    &mut overlay_archive,
                    &profile_dir,
                    false,
                )?;
            }
        } else {
            eprintln!(
                "[install_mod] Updating profile cache root for {:?}",
                profile_dir
            );
            extract_regular_mod_to_root(&mut archive, &profile_dir, &mod_name, false)?;
            if target_is_macos {
                migrate_root_plugins_into_bepinex(&profile_dir)?;
            }
        }

        if !runtime_compat_assets.is_empty() {
            write_runtime_compat_assets(&profile_dir, &runtime_compat_assets, false)?;
        }

        if target_is_macos {
            dequarantine_recursive(game_dir);
        }
    }

    eprintln!(
        "[install_mod] Successfully installed {} to game folder",
        mod_name
    );
    Ok(serde_json::json!({ "success": true }))
}

#[command]
pub async fn remove_mod(
    app: AppHandle,
    profile_id: String,
    mod_name: String,
) -> Result<bool, String> {
    let profile_dir = crate::utils::paths::app_data_dir(&app)
        .unwrap()
        .join("profiles")
        .join(&profile_id);
    let plugins_dir = profile_dir.join("BepInEx").join("plugins");

    // mod_name is usually "Namespace-Name-Version" or "Namespace-Name"
    // We need to find the folder.
    // Logic similar to open_mod_folder

    if plugins_dir.exists() {
        for entry in walkdir::WalkDir::new(&plugins_dir)
            .min_depth(1)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Simple check: if folder name contains mod_name (case insensitive)
                    // Better: check if folder name STARTS with mod_name (Namespace-Name)
                    // But mod_name passed from frontend is usually "Namespace-Name-Version"
                    // We should probably pass the "clean name" or handle it.

                    // Let's try to match loosely for now, or require exact match if possible.
                    // Frontend passes `mod.uuid4` to store, but `removeMod` in store has `modId`.
                    // Wait, store `removeMod` takes `modId` (uuid4).
                    // But to delete file, I need the name.
                    // The store has the profile, so it knows the name.

                    // I will update the store to pass the name.

                    if name.to_lowercase().contains(&mod_name.to_lowercase()) {
                        fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

#[command]
pub async fn open_mod_folder(
    app: AppHandle,
    _profile_id: String,
    mod_name: String,
    game_identifier: String,
    platform: Option<String>,
) -> Result<(), String> {
    let game_path = get_game_path(app.clone(), game_identifier.clone(), platform)
        .await?
        .ok_or_else(|| "GAME_PATH_NOT_CONFIGURED".to_string())?;
    let game_root = std::path::Path::new(&game_path);
    let mod_key = extract_mod_key(&mod_name);

    if crate::models::shared::is_outerwilds_identifier(&game_identifier)
        || crate::models::shared::is_outerwilds_game_path(game_root)
    {
        let owml_root = game_root.join("OWML");
        if !owml_root.exists() {
            return Err("MODS_NOT_APPLIED".to_string());
        }

        if mod_key.contains("owml") || mod_key.contains("outerwildsmodmanager") {
            open::that(&owml_root).map_err(|e| format!("Failed to open OWML folder: {}", e))?;
            return Ok(());
        }

        let mods_root = owml_root.join("Mods");
        if !mods_root.exists() {
            return Err("MODS_NOT_APPLIED".to_string());
        }

        let mut target_dir = None;
        if let Ok(entries) = fs::read_dir(&mods_root) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    let normalized_folder = folder_name.replace('.', "-").to_lowercase();
                    let normalized_mod = mod_key.replace('.', "-").to_lowercase();
                    
                    if normalized_folder == normalized_mod || normalized_folder.contains(&normalized_mod) || normalized_mod.contains(&normalized_folder) {
                        target_dir = Some(entry.path());
                        break;
                    }

                    let manifest_path = entry.path().join("manifest.json");
                    if manifest_path.exists() {
                        if let Ok(data) = fs::read_to_string(&manifest_path) {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                                if let Some(un) = v["uniqueName"].as_str() {
                                    let un_normalized = un.replace('.', "-").to_lowercase();
                                    if un_normalized == normalized_mod {
                                        target_dir = Some(entry.path());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(target) = target_dir {
            if target.exists() {
                open::that(&target).map_err(|e| format!("Failed to open OWML mod folder: {}", e))?;
                return Ok(());
            }
        }

        return Err("MOD_NOT_INSTALLED".to_string());
    }

    if is_balatro_game_path(game_root) {
        let mods_root = get_balatro_mods_dir().ok_or_else(|| "MODS_NOT_APPLIED".to_string())?;

        if is_balatro_lovely_mod(&mod_name) {
            let lovely_files_present = [
                game_root.join("run_lovely_macos.sh"),
                game_root.join("liblovely.dylib"),
            ]
            .iter()
            .all(|path| path.exists());

            if lovely_files_present {
                open::that(game_root)
                    .map_err(|e| format!("Failed to open Balatro root folder: {}", e))?;
                return Ok(());
            }

            return Err("MODS_NOT_APPLIED".to_string());
        }

        let target = if is_balatro_steamodded_mod(&mod_name) {
            mods_root.join("smods")
        } else if let Some(entry_name) = find_mod_folder_in(&mods_root, &mod_name) {
            mods_root.join(entry_name)
        } else if let Some(found_path) = find_mod_entry_recursive(&mods_root, &mod_name, 3) {
            if found_path.is_file() {
                found_path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or(found_path)
            } else {
                found_path
            }
        } else {
            return Err("MOD_NOT_INSTALLED".to_string());
        };

        if target.exists() {
            open::that(&target).map_err(|e| format!("Failed to open Balatro mod folder: {}", e))?;
            return Ok(());
        }

        return Err("MOD_NOT_INSTALLED".to_string());
    }

    // BepInExPack is installed to game root (BepInEx/, doorstop files), not under plugins/.
    if mod_key.contains("bepinexpack") {
        let use_disabled_runtime = profile_is_vanilla(&app, &_profile_id)
            && is_macos_game_dir(game_root)
            && !is_balatro_game_path(game_root);
        let bepinex_root = game_root.join(runtime_bepinex_dir_name(use_disabled_runtime));
        if bepinex_root.exists() {
            open::that(&bepinex_root)
                .map_err(|e| format!("Failed to open BepInEx folder: {}", e))?;
            return Ok(());
        }

        let has_root_injection_files = if use_disabled_runtime {
            [
                game_root.join("run_bepinex.sh"),
                game_root.join("doorstop_libs_DISABLED"),
                game_root.join("doorstop_config.ini_DISABLED"),
                game_root.join("libdoorstop.dylib_DISABLED"),
            ]
            .into_iter()
            .any(|p| p.exists())
        } else {
            [
                game_root.join("run_bepinex.sh"),
                game_root.join("doorstop_libs"),
                game_root.join("doorstop_config.ini"),
                game_root.join("libdoorstop.dylib"),
                game_root.join("winhttp.dll"),
            ]
            .into_iter()
            .any(|p| p.exists())
        };

        if has_root_injection_files {
            open::that(game_root).map_err(|e| format!("Failed to open game root folder: {}", e))?;
            return Ok(());
        }

        return Err("MODS_NOT_APPLIED".to_string());
    }

    let use_disabled_runtime = profile_is_vanilla(&app, &_profile_id)
        && is_macos_game_dir(game_root)
        && !is_balatro_game_path(game_root);
    let bepinex_root = game_root.join(runtime_bepinex_dir_name(use_disabled_runtime));
    if !bepinex_root.exists() {
        return Err("MODS_NOT_APPLIED".to_string());
    }

    // First check common mod locations.
    let common_dirs = [
        bepinex_root.join("plugins"),
        bepinex_root.join("patchers"),
        bepinex_root.join("core"),
    ];

    for dir in common_dirs {
        if let Some(entry_name) = find_mod_folder_in(&dir, &mod_name) {
            let target = dir.join(entry_name);
            open::that(&target).map_err(|e| format!("Failed to open mod folder: {}", e))?;
            return Ok(());
        }
    }

    // Recursive fallback for packages that place files in non-standard nested paths.
    if let Some(found_path) = find_mod_entry_recursive(&bepinex_root, &mod_name, 4) {
        let target = if found_path.is_file() {
            found_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(found_path)
        } else {
            found_path
        };
        open::that(&target).map_err(|e| format!("Failed to open mod folder: {}", e))?;
        return Ok(());
    }

    // MonoMod-based packs may resolve under patchers without a clear folder match.
    if mod_name.to_lowercase().contains("monomod") {
        let patchers_dir = bepinex_root.join("patchers");
        if patchers_dir.exists() {
            open::that(&patchers_dir)
                .map_err(|e| format!("Failed to open patchers folder: {}", e))?;
            return Ok(());
        }
    }

    Err("MOD_NOT_INSTALLED".to_string())
}

#[command]
pub async fn toggle_mod(
    app: AppHandle,
    profile_id: String,
    mod_name: String,
    enabled: bool,
    game_identifier: Option<String>,
    platform: Option<String>,
) -> Result<(), String> {
    eprintln!(
        "[toggle_mod] Toggle mod: {} enabled: {} in profile: {}",
        mod_name, enabled, profile_id
    );

    let use_disabled_runtime = profile_is_vanilla(&app, &profile_id)
        && platform
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("mac"))
            .unwrap_or(false);

    // Get game path for sync (optional - toggle still works without it)
    let game_plugins = if let Some(ref game_id) = game_identifier {
        if let Ok(Some(game_path_str)) =
            get_game_path(app.clone(), game_id.clone(), platform.clone()).await
        {
            Some(runtime_plugins_dir(
                std::path::Path::new(&game_path_str),
                use_disabled_runtime,
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Get profile cache path (may or may not exist depending on legacy mode)
    let profile_dir = crate::utils::paths::app_data_dir(&app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(&profile_id);
    let profile_plugins_dir = profile_dir.join("BepInEx").join("plugins");

    // Find mod in profile cache OR game folder
    // Try profile cache first
    let mut found_folder_name = find_mod_folder_in(&profile_plugins_dir, &mod_name);

    // If not found in profile cache, try game folder
    if found_folder_name.is_none() {
        if let Some(ref game_plugins_path) = game_plugins {
            found_folder_name = find_mod_folder_in(game_plugins_path, &mod_name);
        }
    }

    // If we have a game folder, sync the mod state
    if let Some(ref game_plugins_path) = game_plugins {
        if let Some(ref folder_name) = found_folder_name {
            let game_mod_path = game_plugins_path.join(folder_name);
            let profile_mod_path = profile_plugins_dir.join(folder_name);

            if enabled {
                // Need to add mod to game - copy from profile cache if available
                if profile_mod_path.exists() && !game_mod_path.exists() {
                    eprintln!(
                        "[toggle_mod] Enabling mod - copying from cache to game: {}",
                        folder_name
                    );
                    copy_dir_recursive(&profile_mod_path, &game_mod_path)
                        .map_err(|e| format!("Failed to sync mod to game: {}", e))?;
                }
            } else {
                // Remove mod from game folder (keep in cache)
                if game_mod_path.exists() || game_mod_path.is_symlink() {
                    eprintln!(
                        "[toggle_mod] Disabling mod - removing from game: {}",
                        folder_name
                    );
                    if game_mod_path.is_symlink() || game_mod_path.is_file() {
                        fs::remove_file(&game_mod_path).map_err(|e| {
                            format!("Failed to remove mod symlink/file from game: {}", e)
                        })?;
                    } else {
                        fs::remove_dir_all(&game_mod_path).map_err(|e| {
                            format!("Failed to remove mod directory from game: {}", e)
                        })?;
                    }
                }
            }
        }
    }

    // Always succeed - the enabled state is tracked in profiles.json, not file system
    eprintln!("[toggle_mod] Toggle complete for mod: {}", mod_name);
    Ok(())
}

#[command]
pub async fn copy_mod_from_cache(
    app: AppHandle,
    profile_id: String,
    mod_name: String,
    game_path: String,
) -> Result<serde_json::Value, String> {
    let profile_dir = crate::utils::paths::app_data_dir(&app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(&profile_id);
    let game_dir = std::path::Path::new(&game_path);
    let mod_name_lower = mod_name.to_lowercase();
    let use_disabled_runtime = profile_is_vanilla(&app, &profile_id)
        && is_macos_game_dir(game_dir)
        && !is_balatro_game_path(game_dir);

    if is_balatro_game_path(game_dir) {
        if is_balatro_lovely_mod(&mod_name) {
            let cache_dir = profile_dir.join("BalatroRoot");
            if cache_dir.exists() {
                for file_name in ["run_lovely_macos.sh", "liblovely.dylib"] {
                    let src = cache_dir.join(file_name);
                    if src.exists() {
                        let dst = game_dir.join(file_name);
                        fs::copy(&src, &dst).map_err(|e| e.to_string())?;
                        if file_name.ends_with(".sh") {
                            set_script_executable(&dst)?;
                        }
                    }
                }
                dequarantine_recursive(game_dir);
                return Ok(serde_json::json!({ "success": true, "copied": true }));
            }
        } else {
            let profile_mods_dir = profile_dir.join("Balatro").join("Mods");
            let game_mods_dir = get_balatro_mods_dir().ok_or_else(|| {
                "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string()
            })?;
            fs::create_dir_all(&game_mods_dir).map_err(|e| e.to_string())?;

            let target_folder = balatro_target_folder_name(&mod_name);
            let src_path = profile_mods_dir.join(&target_folder);
            let dst_path = game_mods_dir.join(&target_folder);
            if src_path.exists() {
                if dst_path.exists() {
                    let _ = fs::remove_dir_all(&dst_path);
                }
                copy_dir_recursive(&src_path, &dst_path).map_err(|e| e.to_string())?;
                return Ok(serde_json::json!({ "success": true, "copied": true }));
            }
        }

        eprintln!(
            "[copy_mod_from_cache] Balatro mod {} not found in profile cache",
            mod_name
        );
        return Ok(serde_json::json!({ "success": false, "copied": false }));
    }

    let game_identifier = get_profile_game_identifier(&app, &profile_id).unwrap_or_default();
    let target_is_outerwilds = crate::models::shared::is_outerwilds_identifier(&game_identifier)
        || crate::models::shared::is_outerwilds_game_path(game_dir);

    if target_is_outerwilds {
        let profile_mods_dir = profile_dir.join("OWML").join("Mods");
        let game_mods_dir = game_dir.join("OWML").join("Mods");
        fs::create_dir_all(&game_mods_dir).map_err(|e| e.to_string())?;

        let read_owml_unique_name = |mod_dir: &std::path::Path| -> Option<String> {
            let manifest_path = mod_dir.join("manifest.json");
            if let Ok(data) = fs::read_to_string(&manifest_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(unique_name) = v["uniqueName"].as_str() {
                        return Some(unique_name.replace('.', "-").to_lowercase());
                    }
                }
            }
            None
        };

        let mut found_folder = None;
        if let Ok(entries) = fs::read_dir(&profile_mods_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    let folder_name = entry.file_name().to_string_lossy().to_string();
                    let mod_key = read_owml_unique_name(&entry.path())
                        .unwrap_or_else(|| extract_mod_key(&folder_name));
                    let query_key = extract_mod_key(&mod_name);
                    if mod_key == query_key {
                        found_folder = Some(folder_name);
                        break;
                    }
                }
            }
        }

        if let Some(folder) = found_folder {
            let src_path = profile_mods_dir.join(&folder);
            let dst_path = game_mods_dir.join(&folder);
            if src_path.exists() {
                if dst_path.exists() {
                    let _ = fs::remove_dir_all(&dst_path);
                }
                copy_dir_recursive(&src_path, &dst_path).map_err(|e| e.to_string())?;
                return Ok(serde_json::json!({ "success": true, "copied": true }));
            }
        }

        eprintln!(
            "[copy_mod_from_cache] Outer Wilds mod {} not found in profile cache",
            mod_name
        );
        return Ok(serde_json::json!({ "success": false, "copied": false }));
    }

    if is_macos_game_dir(game_dir) && mod_name_lower.contains("bepinexpack") {
        let profile_has_runtime = profile_dir.join("BepInEx").join("core").is_dir()
            && profile_dir.join("doorstop_libs").is_dir()
            && profile_dir.join("run_bepinex.sh").exists();

        if profile_has_runtime {
            let root_dirs = [
                ("BepInEx", runtime_bepinex_dir_name(use_disabled_runtime)),
                (
                    "doorstop_libs",
                    if use_disabled_runtime {
                        "doorstop_libs_DISABLED"
                    } else {
                        "doorstop_libs"
                    },
                ),
            ];
            for (source_name, dest_name) in root_dirs {
                let src = profile_dir.join(source_name);
                let dst = game_dir.join(dest_name);
                if src.exists() {
                    if dst.exists() {
                        let _ = fs::remove_dir_all(&dst);
                    }
                    copy_dir_recursive(&src, &dst).map_err(|e| e.to_string())?;
                }
            }

            let root_files = [
                (
                    "doorstop_config.ini",
                    if use_disabled_runtime {
                        "doorstop_config.ini_DISABLED"
                    } else {
                        "doorstop_config.ini"
                    },
                ),
                (
                    "libdoorstop.dylib",
                    if use_disabled_runtime {
                        "libdoorstop.dylib_DISABLED"
                    } else {
                        "libdoorstop.dylib"
                    },
                ),
                ("run_bepinex.sh", "run_bepinex.sh"),
            ];
            for (source_name, dest_name) in root_files {
                let src = profile_dir.join(source_name);
                let dst = game_dir.join(dest_name);
                if src.exists() {
                    if dst.exists() {
                        let _ = fs::remove_file(&dst);
                    }
                    fs::copy(&src, &dst).map_err(|e| e.to_string())?;
                    if source_name.ends_with(".sh") {
                        set_script_executable(&dst)?;
                    }
                }
            }

            dequarantine_recursive(game_dir);
            if !use_disabled_runtime {
                migrate_root_plugins_into_bepinex(game_dir)?;
            }
            return Ok(serde_json::json!({ "success": true, "copied": true }));
        }

        let version_number = extract_version_number_from_full_name(&mod_name)
            .ok_or_else(|| format!("Could not parse BepInEx version from {}", mod_name))?;
        let runtime_kind = detect_unity_runtime_kind(game_dir);
        let runtime_bytes =
            download_official_macos_bepinex_runtime(&version_number, runtime_kind).await?;

        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut game_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
        extract_bepinex_pack_to_root(&mut game_archive, game_dir, true, use_disabled_runtime)?;

        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut profile_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
        extract_bepinex_pack_to_root(&mut profile_archive, &profile_dir, true, false)?;

        dequarantine_recursive(game_dir);
        if !use_disabled_runtime {
            migrate_root_plugins_into_bepinex(game_dir)?;
        }
        return Ok(serde_json::json!({ "success": true, "copied": true }));
    }

    let profile_plugins_dir = profile_dir.join("BepInEx").join("plugins");
    let game_plugins_dir = runtime_plugins_dir(game_dir, use_disabled_runtime);

    if let Ok(entries) = fs::read_dir(&profile_plugins_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let folder_name = entry.file_name().to_string_lossy().to_string();

            if folder_name.to_lowercase().contains(&mod_name_lower)
                || mod_name_lower.contains(&folder_name.to_lowercase())
            {
                let src_path = entry.path();
                let dst_path = game_plugins_dir.join(&folder_name);

                if src_path.is_dir() {
                    eprintln!(
                        "[copy_mod_from_cache] Copying {} from cache to game",
                        folder_name
                    );
                    fs::create_dir_all(&game_plugins_dir).map_err(|e| e.to_string())?;
                    if dst_path.exists() {
                        let _ = fs::remove_dir_all(&dst_path);
                    }
                    copy_dir_recursive(&src_path, &dst_path).map_err(|e| e.to_string())?;
                    if is_macos_game_dir(game_dir) && !use_disabled_runtime {
                        migrate_root_plugins_into_bepinex(game_dir)?;
                    }
                    return Ok(serde_json::json!({ "success": true, "copied": true }));
                }
            }
        }
    }

    // Not found in cache
    eprintln!(
        "[copy_mod_from_cache] Mod {} not found in profile cache",
        mod_name
    );
    Ok(serde_json::json!({ "success": false, "copied": false }))
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ChunkCache {
    pub url: String,
    pub packages: Vec<crate::models::shared::Package>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct GamePackagesCache {
    pub chunks: Vec<ChunkCache>,
}

fn load_packages_from_disk(app: &AppHandle, game_id: &str) -> Option<GamePackagesCache> {
    use std::io::Read;
    let cache_dir = match crate::utils::paths::app_cache_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("[load_packages_from_disk] Failed to resolve cache dir: {}", e);
            return None;
        }
    };
    let cache_file = cache_dir.join(format!("{}_packages_v2.json.gz", game_id));
    if !cache_file.exists() {
        eprintln!("[load_packages_from_disk] Cache file does not exist: {:?}", cache_file);
        return None;
    }
    let file = match std::fs::File::open(&cache_file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[load_packages_from_disk] Failed to open cache file: {}", e);
            return None;
        }
    };
    let mut gz = flate2::read::GzDecoder::new(file);
    let mut data = Vec::new();
    if let Err(e) = gz.read_to_end(&mut data) {
        eprintln!("[load_packages_from_disk] Failed to decompress cache file: {}", e);
        return None;
    }
    match serde_json::from_slice(&data) {
        Ok(cache) => Some(cache),
        Err(e) => {
            eprintln!("[load_packages_from_disk] Failed to deserialize cache: {}", e);
            return None;
        }
    }
}

fn save_packages_to_disk(app: &AppHandle, game_id: &str, cache: &GamePackagesCache) -> Result<(), String> {
    use std::io::Write;
    let cache_dir = crate::utils::paths::app_cache_dir(app)
        .map_err(|e| format!("Failed to get cache dir: {}", e))?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {}", e))?;
    let cache_file = cache_dir.join(format!("{}_packages_v2.json.gz", game_id));
    let file = std::fs::File::create(cache_file).map_err(|e| format!("Failed to create cache file: {}", e))?;
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let serialized = serde_json::to_vec(cache).map_err(|e| format!("Failed to serialize cache: {}", e))?;
    encoder.write_all(&serialized).map_err(|e| format!("Failed to compress cache: {}", e))?;
    encoder.finish().map_err(|e| format!("Failed to finalize gzip: {}", e))?;
    Ok(())
}

fn decode_gzip_or_plain(bytes: &[u8], label: &str) -> Result<String, String> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut gz = flate2::read::GzDecoder::new(bytes);
        let mut out = String::new();
        std::io::Read::read_to_string(&mut gz, &mut out)
            .map_err(|e| format!("Failed to decompress {}: {}", label, e))?;
        Ok(out)
    } else {
        String::from_utf8(bytes.to_vec())
            .map_err(|e| format!("Failed to decode {} as utf-8: {}", label, e))
    }
}

fn parse_index_chunk_urls(index_json: &str) -> Result<Vec<String>, String> {
    let parsed: serde_json::Value = serde_json::from_str(index_json)
        .map_err(|e| format!("Failed to parse index JSON: {}", e))?;

    let extract_urls = |arr: &[serde_json::Value]| -> Vec<String> {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    };

    if let Some(arr) = parsed.as_array() {
        let urls = extract_urls(arr);
        if !urls.is_empty() {
            return Ok(urls);
        }
    }

    if let Some(arr) = parsed.get("chunks").and_then(|v| v.as_array()) {
        let urls = extract_urls(arr);
        if !urls.is_empty() {
            return Ok(urls);
        }
    }

    Err("Index JSON has no valid chunk URLs".to_string())
}

async fn load_chunk(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<crate::models::shared::Package>, String> {
    use std::time::Duration;
    let mut last_error: Option<String> = None;
    for attempt in 1..=3 {
        let resp = match client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                last_error = Some(format!("Attempt {} network error: {}", attempt, e));
                tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                continue;
            }
        };

        if !resp.status().is_success() {
            last_error = Some(format!(
                "Attempt {} failed with status {}",
                attempt,
                resp.status()
            ));
            tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
            continue;
        }

        let bytes = match resp.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                last_error = Some(format!("Attempt {} failed reading body: {}", attempt, e));
                tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                continue;
            }
        };

        let json_str = if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut gz = flate2::read::GzDecoder::new(&bytes[..]);
            let mut out = String::new();
            match std::io::Read::read_to_string(&mut gz, &mut out) {
                Ok(_) => out,
                Err(e) => {
                    last_error = Some(format!(
                        "Attempt {} failed decompressing gzip: {}",
                        attempt, e
                    ));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            }
        } else {
            match String::from_utf8(bytes.to_vec()) {
                Ok(out) => out,
                Err(e) => {
                    last_error =
                        Some(format!("Attempt {} invalid utf-8 chunk: {}", attempt, e));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            }
        };

        let mut packages: Vec<crate::models::shared::Package> =
            match serde_json::from_str(&json_str) {
                Ok(packages) => packages,
                Err(e) => {
                    last_error = Some(format!(
                        "Attempt {} failed parsing JSON chunk: {}",
                        attempt, e
                    ));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            };

        drop(json_str);

        // Truncate versions to 1 (latest version) to keep memory usage extremely low
        for pkg in &mut packages {
            if pkg.versions.len() > 1 {
                pkg.versions.truncate(1);
            }
            if let Some(ver) = pkg.versions.first_mut() {
                if ver.dependencies.len() > 20 {
                    ver.dependencies.truncate(20);
                }
                if ver.description.len() > 200 {
                    let truncated = ver
                        .description
                        .char_indices()
                        .take_while(|(i, _)| *i < 200)
                        .last()
                        .map(|(i, c)| i + c.len_utf8())
                        .unwrap_or(200);
                    ver.description.truncate(truncated);
                }
            }
        }

        // Filter out Manager packages
        packages.retain(|pkg| {
            !pkg.full_name.contains("ebkr-r2modman")
                && !pkg.full_name.contains("Tslat-ThunderstoreModManager")
                && !pkg.full_name.contains("Kesomannen-GaleModManager")
        });

        // Release excess capacity
        packages.shrink_to_fit();
        for pkg in &mut packages {
            pkg.versions.shrink_to_fit();
            if let Some(ver) = pkg.versions.first_mut() {
                ver.dependencies.shrink_to_fit();
            }
        }

        return Ok(packages);
    }

    Err(last_error.unwrap_or_else(|| "Failed to load chunk".to_string()))
}

#[command]
pub async fn fetch_packages(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    game_id: String,
) -> Result<usize, String> {
    use std::time::SystemTime;

    let start_time = SystemTime::now();
    let client = thunderstore_client();

    // --- Outer Wilds: use ow-mod-db instead of Thunderstore ---
    if crate::models::shared::is_outerwilds_identifier(&game_id) {
        // Check memory cache first
        {
            let packages_lock = state.packages.read().await;
            if let Some(packages) = packages_lock.get(&game_id) {
                if !packages.is_empty() {
                    eprintln!("[fetch_packages/ow] Serving {} packages from memory (instant)", packages.len());
                    return Ok(packages.len());
                }
            }
        }

        // Try to load from disk cache first
        if let Some(cache) = load_packages_from_disk(&app, &game_id) {
            let mut all_packages = Vec::new();
            for chunk in &cache.chunks {
                all_packages.extend(chunk.packages.clone());
            }
            let count = all_packages.len();
            eprintln!(
                "[fetch_packages/ow] Loaded {} packages from disk cache for {} (instant)",
                count, game_id
            );

            // Put them into memory state immediately so user sees them
            {
                let mut packages_lock = state.packages.write().await;
                packages_lock.insert(game_id.clone(), all_packages);
            }
            state.touch_packages_cache(&game_id).await;

            // Spawn background task to check/fetch network db and update sizes/disk cache
            let packages_arc = state.packages.clone();
            let game_id_clone = game_id.clone();
            let app_handle = app.clone();
            tokio::spawn(async move {
                let client = thunderstore_client();
                const OW_DB_URL: &str = "https://ow-mods.github.io/ow-mod-db/database.json";
                const OW_THUMBNAIL_BASE: &str = "https://ow-mods.github.io/ow-mod-db/thumbnails/";

                // Build size cache from existing packages in memory
                let mut size_cache = std::collections::HashMap::new();
                {
                    let lock = packages_arc.read().await;
                    if let Some(existing_pkgs) = lock.get(&game_id_clone) {
                        for p in existing_pkgs {
                            for v in &p.versions {
                                if v.file_size > 0 {
                                    size_cache.insert(v.download_url.clone(), v.file_size);
                                }
                            }
                        }
                    }
                }

                let resp = match client.get(OW_DB_URL).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[fetch_packages/ow] Background fetch failed: {}", e);
                        return;
                    }
                };
                if !resp.status().is_success() {
                    eprintln!("[fetch_packages/ow] Background fetch status failed: {}", resp.status());
                    return;
                }
                let db: serde_json::Value = match resp.json().await {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("[fetch_packages/ow] Background JSON parse failed: {}", e);
                        return;
                    }
                };

                let releases = db["releases"].as_array().cloned().unwrap_or_default();
                let mut packages: Vec<crate::models::shared::Package> = Vec::with_capacity(releases.len());

                for entry in &releases {
                    let unique_name = entry["uniqueName"].as_str().unwrap_or("");
                    if unique_name.is_empty() { continue; }

                    let (owner, mod_name) = if let Some(dot) = unique_name.find('.') {
                        (&unique_name[..dot], &unique_name[dot + 1..])
                    } else {
                        (unique_name, unique_name)
                    };

                    let name = entry["name"].as_str().unwrap_or(mod_name).to_string();
                    let description = entry["description"].as_str().unwrap_or("").to_string();
                    let version = entry["version"].as_str().unwrap_or("0.0.0")
                        .trim_start_matches('v').to_string();
                    let download_url = entry["downloadUrl"].as_str().unwrap_or("").to_string();
                    let download_count = entry["downloadCount"].as_i64().unwrap_or(0);
                    let repo = entry["repo"].as_str().unwrap_or("").to_string();
                    let latest_release_date = entry["latestReleaseDate"].as_str().unwrap_or("").to_string();
                    let first_release_date = entry["firstReleaseDate"].as_str().unwrap_or("").to_string();
                    let is_required = entry["required"].as_bool().unwrap_or(false);

                    let icon_url = entry["thumbnail"]["main"].as_str()
                        .map(|thumb| format!("{}{}", OW_THUMBNAIL_BASE, thumb))
                        .unwrap_or_default();

                    let categories: Vec<String> = entry["tags"].as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();

                    let full_name = format!("{}-{}", owner, mod_name);
                    let package_url = if repo.is_empty() {
                        format!("https://outerwildsmods.com/mods/{}/", entry["slug"].as_str().unwrap_or(""))
                    } else {
                        repo.clone()
                    };

                    let cached_size = size_cache.get(&download_url).copied().unwrap_or(0);

                    let version_struct = crate::models::shared::PackageVersion {
                        name: mod_name.to_string(),
                        description: description.clone(),
                        icon: icon_url.clone(),
                        version_number: version,
                        dependencies: vec![],
                        download_url,
                        downloads: download_count,
                        website_url: repo.clone(),
                        file_size: cached_size,
                        uuid4: unique_name.to_string(),
                        full_name: full_name.clone(),
                        date_created: latest_release_date.clone(),
                        is_active: true,
                    };

                    let mut versions_list = vec![version_struct];

                    // Check for prerelease and add it to version list
                    if let Some(prerelease) = entry.get("prerelease") {
                        if let Some(pre_version) = prerelease["version"].as_str() {
                            let pre_version_clean = pre_version.trim_start_matches('v').to_string();
                            let pre_download_url = prerelease["downloadUrl"].as_str().unwrap_or("").to_string();
                            let pre_date = prerelease["date"].as_str().unwrap_or("").to_string();
                            
                            if !pre_version_clean.is_empty() && !pre_download_url.is_empty() {
                                let cached_pre_size = size_cache.get(&pre_download_url).copied().unwrap_or(0);
                                versions_list.push(crate::models::shared::PackageVersion {
                                    name: mod_name.to_string(),
                                    description: format!("(Prerelease) {}", description.clone()),
                                    icon: icon_url.clone(),
                                    version_number: pre_version_clean,
                                    dependencies: vec![],
                                    download_url: pre_download_url,
                                    downloads: download_count,
                                    website_url: repo.clone(),
                                    file_size: cached_pre_size,
                                    uuid4: format!("{}-prerelease", unique_name),
                                    full_name: full_name.clone(),
                                    date_created: pre_date,
                                    is_active: true,
                                });
                            }
                        }
                    }

                    packages.push(crate::models::shared::Package {
                        name: name.clone(),
                        full_name,
                        owner: owner.to_string(),
                        package_url,
                        date_created: first_release_date,
                        date_updated: latest_release_date,
                        uuid4: unique_name.to_string(),
                        rating_score: download_count as i64,
                        is_pinned: is_required,
                        is_deprecated: false,
                        has_nsfw_content: false,
                        categories,
                        versions: versions_list,
                    });
                }

                // Resolve file sizes for background-fetched packages in parallel ONLY if we don't have them
                let urls: Vec<(usize, usize, String)> = packages.iter().enumerate()
                    .flat_map(|(pkg_idx, p)| {
                        p.versions.iter().enumerate().filter_map(move |(ver_idx, v)| {
                            if v.download_url.is_empty() || v.file_size > 0 {
                                None
                            } else {
                                Some((pkg_idx, ver_idx, v.download_url.clone()))
                            }
                        })
                    })
                    .collect();

                if !urls.is_empty() {
                    let head_client = thunderstore_client();
                    let parallelism = 8usize;
                    let mut stream = futures_util::stream::iter(urls)
                        .map(|(pkg_idx, ver_idx, url)| {
                            let c = head_client.clone();
                            async move {
                                let size = c.head(&url).send().await.ok()
                                    .and_then(|r| r.headers().get("content-length")
                                        .and_then(|v| v.to_str().ok())
                                        .and_then(|s| s.parse::<u64>().ok()))
                                    .unwrap_or(0);
                                (pkg_idx, ver_idx, size)
                            }
                        })
                        .buffer_unordered(parallelism);

                    while let Some((pkg_idx, ver_idx, size)) = stream.next().await {
                        if size == 0 { continue; }
                        if let Some(pkg) = packages.get_mut(pkg_idx) {
                            if let Some(ver) = pkg.versions.get_mut(ver_idx) {
                                ver.file_size = size;
                            }
                        }
                    }
                }

                // Put them into memory state and save to disk
                {
                    let mut packages_lock = packages_arc.write().await;
                    packages_lock.insert(game_id_clone.clone(), packages.clone());
                }

                let first_chunk = ChunkCache {
                    url: "ow-mod-db".to_string(),
                    packages: packages,
                };
                let cache = GamePackagesCache {
                    chunks: vec![first_chunk],
                };
                let _ = save_packages_to_disk(&app_handle, &game_id_clone, &cache);

                let total = cache.chunks[0].packages.len();
                let _ = app_handle.emit(
                    "packages-loaded",
                    serde_json::json!({ "game_id": game_id_clone, "total_count": total }),
                );
            });

            return Ok(count);
        }

        const OW_DB_URL: &str = "https://ow-mods.github.io/ow-mod-db/database.json";
        const OW_THUMBNAIL_BASE: &str = "https://ow-mods.github.io/ow-mod-db/thumbnails/";

        eprintln!("[fetch_packages/ow] Fetching ow-mod-db from {}", OW_DB_URL);
        let resp = client.get(OW_DB_URL).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("[fetch_packages/ow] ow-mod-db fetch failed: {}", resp.status()));
        }
        let db: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

        let releases = db["releases"].as_array().cloned().unwrap_or_default();
        let mut packages: Vec<crate::models::shared::Package> = Vec::with_capacity(releases.len());

        for entry in &releases {
            let unique_name = entry["uniqueName"].as_str().unwrap_or("");
            if unique_name.is_empty() { continue; }

            // Split "Namespace.ModName" → owner="Namespace", name="ModName"
            let (owner, mod_name) = if let Some(dot) = unique_name.find('.') {
                (&unique_name[..dot], &unique_name[dot + 1..])
            } else {
                (unique_name, unique_name)
            };

            let name = entry["name"].as_str().unwrap_or(mod_name).to_string();
            let description = entry["description"].as_str().unwrap_or("").to_string();
            let version = entry["version"].as_str().unwrap_or("0.0.0")
                .trim_start_matches('v').to_string();
            let download_url = entry["downloadUrl"].as_str().unwrap_or("").to_string();
            let download_count = entry["downloadCount"].as_i64().unwrap_or(0);
            let repo = entry["repo"].as_str().unwrap_or("").to_string();
            let latest_release_date = entry["latestReleaseDate"].as_str().unwrap_or("").to_string();
            let first_release_date = entry["firstReleaseDate"].as_str().unwrap_or("").to_string();
            let is_required = entry["required"].as_bool().unwrap_or(false);

            let icon_url = entry["thumbnail"]["main"].as_str()
                .map(|thumb| format!("{}{}", OW_THUMBNAIL_BASE, thumb))
                .unwrap_or_default();

            let categories: Vec<String> = entry["tags"].as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            let full_name = format!("{}-{}", owner, mod_name);
            let package_url = if repo.is_empty() {
                format!("https://outerwildsmods.com/mods/{}/", entry["slug"].as_str().unwrap_or(""))
            } else {
                repo.clone()
            };

            let version_struct = crate::models::shared::PackageVersion {
                name: mod_name.to_string(),
                description: description.clone(),
                icon: icon_url.clone(),
                version_number: version,
                dependencies: vec![],
                download_url,
                downloads: download_count,
                website_url: repo.clone(),
                file_size: 0,
                uuid4: unique_name.to_string(),
                full_name: full_name.clone(),
                date_created: latest_release_date.clone(),
                is_active: true,
            };

            let mut versions_list = vec![version_struct];

            // Check for prerelease and add it to version list
            if let Some(prerelease) = entry.get("prerelease") {
                if let Some(pre_version) = prerelease["version"].as_str() {
                    let pre_version_clean = pre_version.trim_start_matches('v').to_string();
                    let pre_download_url = prerelease["downloadUrl"].as_str().unwrap_or("").to_string();
                    let pre_date = prerelease["date"].as_str().unwrap_or("").to_string();
                    
                    if !pre_version_clean.is_empty() && !pre_download_url.is_empty() {
                        versions_list.push(crate::models::shared::PackageVersion {
                            name: mod_name.to_string(),
                            description: format!("(Prerelease) {}", description.clone()),
                            icon: icon_url,
                            version_number: pre_version_clean,
                            dependencies: vec![],
                            download_url: pre_download_url,
                            downloads: download_count,
                            website_url: repo.clone(),
                            file_size: 0,
                            uuid4: format!("{}-prerelease", unique_name),
                            full_name: full_name.clone(),
                            date_created: pre_date,
                            is_active: true,
                        });
                    }
                }
            }

            packages.push(crate::models::shared::Package {
                name: name.clone(),
                full_name,
                owner: owner.to_string(),
                package_url,
                date_created: first_release_date,
                date_updated: latest_release_date,
                uuid4: unique_name.to_string(),
                rating_score: download_count as i64,
                is_pinned: is_required,
                is_deprecated: false,
                has_nsfw_content: false,
                categories,
                versions: versions_list,
            });
        }

        let count = packages.len();
        eprintln!("[fetch_packages/ow] Loaded {} mods from ow-mod-db", count);

        {
            let mut packages_lock = state.packages.write().await;
            packages_lock.insert(game_id.clone(), packages);
        }

        let _ = app.emit(
            "packages-loaded",
            serde_json::json!({ "game_id": game_id, "total_count": count }),
        );

        if let Ok(elapsed) = start_time.elapsed() {
            eprintln!("[fetch_packages/ow] Loaded in {:.2?}", elapsed);
        }

        // Background: resolve file sizes via HEAD requests in parallel
        {
            let packages_arc = state.packages.clone();
            let game_id_clone = game_id.clone();
            let app_handle = app.clone();

            tokio::spawn(async move {
                // Collect (pkg_idx, ver_idx, download_url) for mods that have a URL and don't have a size yet
                let urls: Vec<(usize, usize, String)> = {
                    let lock = packages_arc.read().await;
                    if let Some(pkgs) = lock.get(&game_id_clone) {
                        pkgs.iter().enumerate()
                            .flat_map(|(pkg_idx, p)| {
                                p.versions.iter().enumerate().filter_map(move |(ver_idx, v)| {
                                    if v.download_url.is_empty() || v.file_size > 0 {
                                        None
                                    } else {
                                        Some((pkg_idx, ver_idx, v.download_url.clone()))
                                    }
                                })
                            })
                            .collect()
                    } else {
                        return;
                    }
                };

                if !urls.is_empty() {
                    let head_client = thunderstore_client();
                    let parallelism = 8usize;
                    let mut stream = futures_util::stream::iter(urls)
                        .map(|(pkg_idx, ver_idx, url)| {
                            let c = head_client.clone();
                            async move {
                                let size = c.head(&url).send().await.ok()
                                    .and_then(|r| r.headers().get("content-length")
                                        .and_then(|v| v.to_str().ok())
                                        .and_then(|s| s.parse::<u64>().ok()))
                                    .unwrap_or(0);
                                (pkg_idx, ver_idx, size)
                            }
                        })
                        .buffer_unordered(parallelism);

                    while let Some((pkg_idx, ver_idx, size)) = stream.next().await {
                        if size == 0 { continue; }
                        let mut lock = packages_arc.write().await;
                        if let Some(pkgs) = lock.get_mut(&game_id_clone) {
                            if let Some(pkg) = pkgs.get_mut(pkg_idx) {
                                if let Some(ver) = pkg.versions.get_mut(ver_idx) {
                                    ver.file_size = size;
                                }
                            }
                        }
                    }
                }

                // Save chunk cache to disk to preserve the parsed database structure
                let (total, cache_to_save) = {
                    let lock = packages_arc.read().await;
                    let total = lock.get(&game_id_clone).map(|p| p.len()).unwrap_or(0);
                    let pkgs = lock.get(&game_id_clone).cloned().unwrap_or_default();
                    let first_chunk = ChunkCache {
                        url: "ow-mod-db".to_string(),
                        packages: pkgs,
                    };
                    let cache = GamePackagesCache {
                        chunks: vec![first_chunk],
                    };
                    (total, cache)
                };
                let _ = save_packages_to_disk(&app_handle, &game_id_clone, &cache_to_save);
                let _ = app_handle.emit(
                    "packages-loaded",
                    serde_json::json!({ "game_id": game_id_clone, "total_count": total }),
                );
            });
        }

        return Ok(count);
    }

    // 0. Check if we already have packages in memory (instant return)
    {
        let packages_lock = state.packages.read().await;
        if let Some(packages) = packages_lock.get(&game_id) {
            if !packages.is_empty() {
                eprintln!(
                    "[fetch_packages] Serving {} packages from memory (instant)",
                    packages.len()
                );
                return Ok(packages.len());
            }
        }
    }

    // 1. Try to load from chunk-level disk cache first
    if let Some(cache) = load_packages_from_disk(&app, &game_id) {
        let mut all_packages = Vec::new();
        for chunk in &cache.chunks {
            all_packages.extend(chunk.packages.clone());
        }
        let count = all_packages.len();
        eprintln!(
            "[fetch_packages] Loaded {} packages from chunk disk cache for {} (instant)",
            count, game_id
        );

        // Put them into memory state immediately so user sees them
        {
            let mut packages_lock = state.packages.write().await;
            packages_lock.insert(game_id.clone(), all_packages);
        }
        state.touch_packages_cache(&game_id).await;

        // Spawn background task to check index and update in background if needed
        let packages_arc = state.packages.clone();
        let game_id_clone = game_id.clone();
        let app_handle = app.clone();
        tokio::spawn(async move {
            let client = thunderstore_client();
            let index_url = format!(
                "https://thunderstore.io/c/{}/api/v1/package-listing-index/",
                game_id_clone
            );
            
            let resp = match client.get(&index_url).send().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[fetch_packages] Background index check failed: {}", e);
                    return;
                }
            };
            if !resp.status().is_success() {
                eprintln!("[fetch_packages] Background index check status failed: {}", resp.status());
                return;
            }
            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[fetch_packages] Background index body read failed: {}", e);
                    return;
                }
            };
            let index_json = match decode_gzip_or_plain(&bytes, "index") {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("[fetch_packages] Background index decode failed: {}", e);
                    return;
                }
            };
            let online_chunk_urls = match parse_index_chunk_urls(&index_json) {
                Ok(urls) => urls,
                Err(e) => {
                    eprintln!("[fetch_packages] Background index parse failed: {}", e);
                    return;
                }
            };

            // Compare online chunk URLs with cached chunk URLs
            let cached_urls: Vec<String> = cache.chunks.iter().map(|c| c.url.clone()).collect();
            if cached_urls == online_chunk_urls {
                eprintln!("[fetch_packages] Chunk disk cache is fully up-to-date for {}. No updates needed.", game_id_clone);
                return;
            }

            // Chunks differ! Keep chunks that are still online, and download only the new ones!
            eprintln!("[fetch_packages] Chunk URLs differ from cache. Updating changed chunks in background for {}...", game_id_clone);
            
            let mut kept_chunks: Vec<ChunkCache> = cache.chunks.into_iter()
                .filter(|c| online_chunk_urls.contains(&c.url))
                .collect();

            let urls_to_download: Vec<String> = online_chunk_urls.iter()
                .filter(|url| !cached_urls.contains(url))
                .cloned()
                .collect();

            if !urls_to_download.is_empty() {
                let parallelism = 3usize;
                let mut stream = futures_util::stream::iter(urls_to_download)
                    .map(|url| {
                        let client = thunderstore_client();
                        async move {
                            match load_chunk(&client, &url).await {
                                Ok(packages) => Ok(ChunkCache { url, packages }),
                                Err(e) => Err(e),
                            }
                        }
                    })
                    .buffer_unordered(parallelism);

                while let Some(result) = stream.next().await {
                    match result {
                        Ok(new_chunk) => {
                            kept_chunks.push(new_chunk);
                        }
                        Err(e) => {
                            eprintln!("[fetch_packages] Background update chunk load error: {}", e);
                            return;
                        }
                    }
                }
            }

            // Merge everything
            let mut final_packages = Vec::new();
            for chunk in &kept_chunks {
                final_packages.extend(chunk.packages.clone());
            }
            final_packages.shrink_to_fit();
            let count = final_packages.len();
            eprintln!("[fetch_packages] Background update complete. Loaded {} packages for {}", count, game_id_clone);

            // Update in-memory state
            {
                let mut packages_lock = packages_arc.write().await;
                packages_lock.insert(game_id_clone.clone(), final_packages);
            }

            // Save new cache to disk
            let new_cache = GamePackagesCache {
                chunks: kept_chunks,
            };
            if let Err(e) = save_packages_to_disk(&app_handle, &game_id_clone, &new_cache) {
                eprintln!("[fetch_packages] Failed to save updated chunk cache: {}", e);
            }

            // Emit event so frontend knows packages are updated
            let _ = app_handle.emit(
                "packages-loaded",
                serde_json::json!({
                    "game_id": game_id_clone,
                    "total_count": count
                }),
            );
        });

        return Ok(count);
    }

    // 2. Cache miss: Fetch the index (list of chunk URLs)
    let index_url = format!(
        "https://thunderstore.io/c/{}/api/v1/package-listing-index/",
        game_id
    );
    eprintln!("[fetch_packages] Fetching index from: {}", index_url);

    let resp = client
        .get(&index_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch index: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Index request failed with status {}",
            resp.status()
        ));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let index_json = decode_gzip_or_plain(&bytes, "index")?;
    let chunk_urls: Vec<String> = parse_index_chunk_urls(&index_json)?;
    let total_chunks = chunk_urls.len();
    eprintln!("[fetch_packages] Found {} chunks", total_chunks);
    if total_chunks == 0 {
        return Ok(0);
    }

    // 3. Load FIRST successful chunk immediately for instant UI
    let mut first_loaded_index: Option<usize> = None;
    let mut first_chunk: Option<ChunkCache> = None;
    let first_attempts = std::cmp::min(5, chunk_urls.len());
    for idx in 0..first_attempts {
        let first_url = &chunk_urls[idx];
        match load_chunk(&client, first_url).await {
            Ok(first_packages) => {
                let count = first_packages.len();
                eprintln!(
                    "[fetch_packages] First chunk loaded (index {}): {} packages",
                    idx, count
                );

                // Update state immediately so UI can show something
                let mut packages_lock = state.packages.write().await;
                packages_lock.insert(game_id.clone(), first_packages.clone());
                first_loaded_index = Some(idx);
                first_chunk = Some(ChunkCache {
                    url: first_url.clone(),
                    packages: first_packages,
                });
                drop(packages_lock);
                state.touch_packages_cache(&game_id).await;
                break;
            }
            Err(e) => {
                eprintln!(
                    "[fetch_packages] Failed first chunk attempt idx {}: {}",
                    idx, e
                );
            }
        }
    }

    if first_loaded_index.is_none() {
        return Err(
            "Failed to load initial package chunks (network timeout or CDN issue)".to_string(),
        );
    }

    // 4. Load remaining chunks in parallel (streaming to state)
    let remaining_urls: Vec<String> = chunk_urls
        .clone()
        .into_iter()
        .enumerate()
        .filter_map(|(idx, url)| {
            if Some(idx) == first_loaded_index {
                None
            } else {
                Some(url)
            }
        })
        .collect();

    if !remaining_urls.is_empty() {
        let packages_arc = state.packages.clone();
        let game_id_clone = game_id.clone();
        let app_handle = app.clone();
        let first_chunk = first_chunk.unwrap();

        tokio::spawn(async move {
            let parallelism = 3usize;
            let mut stream = futures_util::stream::iter(remaining_urls)
                .map(|url| {
                    let client = thunderstore_client();
                    async move {
                        match load_chunk(&client, &url).await {
                            Ok(packages) => Ok(ChunkCache { url, packages }),
                            Err(e) => Err(e),
                        }
                    }
                })
                .buffer_unordered(parallelism);

            let mut chunks = vec![first_chunk];
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        let mut packages_lock = packages_arc.write().await;
                        if let Some(existing) = packages_lock.get_mut(&game_id_clone) {
                            existing.extend(chunk.packages.clone());
                        }
                        chunks.push(chunk);
                    }
                    Err(e) => eprintln!("[fetch_packages] Chunk error: {}", e),
                }
            }

            // Shrink final in-memory vector
            {
                let mut packages_lock = packages_arc.write().await;
                if let Some(existing) = packages_lock.get_mut(&game_id_clone) {
                    existing.shrink_to_fit();
                }
            }

            // Save cache
            let final_count = {
                let packages_lock = packages_arc.read().await;
                packages_lock.get(&game_id_clone).map(|p| p.len()).unwrap_or(0)
            };

            eprintln!(
                "[fetch_packages] Background loading complete. Total: {} packages",
                final_count
            );

            let cache = GamePackagesCache { chunks };
            if let Err(e) = save_packages_to_disk(&app_handle, &game_id_clone, &cache) {
                eprintln!("[fetch_packages] Failed to save chunk cache to disk: {}", e);
            }

            let _ = app_handle.emit(
                "packages-loaded",
                serde_json::json!({
                    "game_id": game_id_clone,
                    "total_count": final_count
                }),
            );
        });
    } else {
        // Only 1 chunk total
        let cache = GamePackagesCache {
            chunks: vec![first_chunk.unwrap()],
        };
        let _ = save_packages_to_disk(&app, &game_id, &cache);
    }

    let packages_lock = state.packages.read().await;
    let count = packages_lock.get(&game_id).map(|p| p.len()).unwrap_or(0);

    if let Ok(elapsed) = start_time.elapsed() {
        eprintln!(
            "[fetch_packages] Initial load in {:.2?} ({} packages ready, {} chunks loading in background)",
            elapsed,
            count,
            total_chunks - 1
        );
    }

    Ok(count)
}

#[command]
pub async fn get_available_categories(
    state: tauri::State<'_, AppState>,
    game_id: String,
) -> Result<Vec<String>, String> {
    let packages_lock = state.packages.read().await;

    if let Some(packages) = packages_lock.get(&game_id) {
        let mut categories: std::collections::HashSet<String> = std::collections::HashSet::new();

        for p in packages.iter() {
            for cat in &p.categories {
                categories.insert(cat.clone());
            }
        }

        let mut result: Vec<String> = categories.into_iter().collect();
        result.sort();
        Ok(result)
    } else {
        Ok(vec![])
    }
}

#[derive(Serialize)]
pub struct PackageListResponse {
    pub items: Vec<crate::models::shared::Package>,
    pub total: usize,
}

#[command]
pub async fn get_packages(
    state: tauri::State<'_, AppState>,
    game_id: String,
    page: usize,
    page_size: usize,
    search: String,
    sort: Option<String>,
    nsfw: Option<bool>,
    deprecated: Option<bool>,
    sort_direction: Option<String>,
    categories: Option<Vec<String>>,
    mods: Option<bool>,
    modpacks: Option<bool>,
) -> Result<PackageListResponse, String> {
    let packages_lock = state.packages.read().await;

    if let Some(packages) = packages_lock.get(&game_id) {
        // Initial filtering
        let mut filtered: Vec<&crate::models::shared::Package> = packages
            .iter()
            .filter(|p| {
                // 1. Search Filter
                if !search.is_empty() {
                    let search_lower = search.to_lowercase();
                    let name = p.name.to_lowercase();
                    let full_name = p.full_name.to_lowercase();
                    if !name.contains(&search_lower) && !full_name.contains(&search_lower) {
                        return false;
                    }
                }

                // 2. NSFW Filter
                let nsfw_tag_active = nsfw.unwrap_or(false);
                let is_nsfw = p.has_nsfw_content;
                if nsfw_tag_active {
                    if !is_nsfw {
                        return false;
                    }
                } else {
                    if is_nsfw {
                        return false;
                    }
                }

                // 3. Deprecated Filter
                let deprecated_tag_active = deprecated.unwrap_or(false);
                let is_deprecated = p.is_deprecated;
                if deprecated_tag_active {
                    if !is_deprecated {
                        return false;
                    }
                } else {
                    if is_deprecated {
                        return false;
                    }
                }

                // 4. Mods/Modpacks Filter
                let mods_active = mods.unwrap_or(false);
                let modpacks_active = modpacks.unwrap_or(false);

                let is_modpack = p.categories.iter().any(|c| c.to_lowercase() == "modpacks");

                if mods_active != modpacks_active {
                    if mods_active && is_modpack {
                        return false;
                    }
                    if modpacks_active && !is_modpack {
                        return false;
                    }
                }

                // 5. Category/Tag Filter
                if let Some(ref filter_cats) = categories {
                    if !filter_cats.is_empty() {
                        let pkg_name = p.name.to_lowercase();
                        let pkg_full_name = p.full_name.to_lowercase();

                        let has_match = filter_cats.iter().any(|fc| {
                            let fc_lower = fc.to_lowercase();
                            p.categories
                                .iter()
                                .any(|c| c.to_lowercase().contains(&fc_lower))
                                || pkg_name.contains(&fc_lower)
                                || pkg_full_name.contains(&fc_lower)
                        });

                        if !has_match {
                            return false;
                        }
                    }
                }

                true
            })
            .collect();

        // Sorting
        if let Some(sort_by) = sort {
            let direction = sort_direction.unwrap_or("desc".to_string());
            let is_asc = direction == "asc";

            match sort_by.as_str() {
                "downloads" => filtered.sort_by(|a, b| {
                    let da = a.versions.first().map(|ver| ver.downloads).unwrap_or(0);
                    let db = b.versions.first().map(|ver| ver.downloads).unwrap_or(0);
                    if is_asc {
                        da.cmp(&db)
                    } else {
                        db.cmp(&da)
                    }
                }),
                "rating" => filtered.sort_by(|a, b| {
                    let ra = a.rating_score;
                    let rb = b.rating_score;
                    if is_asc {
                        ra.cmp(&rb)
                    } else {
                        rb.cmp(&ra)
                    }
                }),
                "updated" => filtered.sort_by(|a, b| {
                    let da = &a.date_updated;
                    let db = &b.date_updated;
                    if is_asc {
                        da.cmp(db)
                    } else {
                        db.cmp(da)
                    }
                }),
                "alphabetical" => filtered.sort_by(|a, b| {
                    let na = a.name.to_lowercase();
                    let nb = b.name.to_lowercase();
                    if is_asc {
                        na.cmp(&nb)
                    } else {
                        nb.cmp(&na)
                    }
                }),
                _ => {}
            }
        }

        let start = page * page_size;
        let total = filtered.len();
        if start >= total {
            return Ok(PackageListResponse {
                items: Vec::new(),
                total,
            });
        }

        let end = std::cmp::min(start + page_size, total);
        let slice: Vec<crate::models::shared::Package> =
            filtered[start..end].iter().map(|&v| (*v).clone()).collect();

        Ok(PackageListResponse {
            items: slice,
            total,
        })
    } else {
        Ok(PackageListResponse {
            items: Vec::new(),
            total: 0,
        })
    }
}

#[command]
pub async fn lookup_packages_by_names(
    state: tauri::State<'_, AppState>,
    game_id: String,
    names: Vec<String>,
) -> Result<serde_json::Value, String> {
    use std::sync::OnceLock;
    // Compile the version-strip regex once for the process lifetime.
    static VERSION_REGEX: OnceLock<regex::Regex> = OnceLock::new();
    let re = VERSION_REGEX.get_or_init(|| {
        regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").expect("invalid version regex")
    });

    let packages_lock = state.packages.read().await;

    if let Some(packages) = packages_lock.get(&game_id) {
        // Build a HashMap for O(1) lookups instead of O(n) linear search per name.
        let index: std::collections::HashMap<&str, &crate::models::shared::Package> =
            packages.iter().map(|p| (p.full_name.as_str(), p)).collect();

        let mut found = Vec::with_capacity(names.len());
        let mut unknown = Vec::new();

        for name in names {
            // Strip version if present: "Author-Mod-1.0.0" -> "Author-Mod"
            let clean_name = if let Some(caps) = re.captures(&name) {
                caps.get(1)
                    .map_or(name.as_str(), |m| m.as_str())
                    .to_string()
            } else {
                name.clone()
            };

            if let Some(pkg) = index.get(clean_name.as_str()) {
                found.push((*pkg).clone());
            } else {
                unknown.push(name);
            }
        }

        Ok(serde_json::json!({
            "found": found,
            "unknown": unknown
        }))
    } else {
        Err("Game packages not loaded".to_string())
    }
}

#[command]
pub async fn fetch_package_by_name(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    name: String,
    game_id: Option<String>,
) -> Result<Option<crate::models::shared::Package>, String> {
    // If this is Outer Wilds and the cache is empty, fetch the package list first
    if game_id.as_deref() == Some("outerwilds") {
        let is_empty = {
            let packages_guard = state.packages.read().await;
            packages_guard.get("outerwilds").map(|p| p.is_empty()).unwrap_or(true)
        };
        if is_empty {
            eprintln!("[fetch_package_by_name] Outer Wilds packages not loaded in cache. Loading now...");
            use tauri::Manager;
            let app_state = app.state::<AppState>();
            let _ = fetch_packages(app.clone(), app_state, "outerwilds".to_string()).await;
        }
    }

    // name might be "Namespace-Name" or "Namespace-Name-Version"

    // 1. Strip version if present (Regex: ^(.*)-(\d+\.\d+\.\d+)$)
    let re = regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").unwrap();
    let (clean_name, version_str) = if let Some(caps) = re.captures(&name) {
        (
            caps.get(1).map_or(name.clone(), |m| m.as_str().to_string()),
            Some(caps.get(2).unwrap().as_str().to_string()),
        )
    } else {
        (name.clone(), None)
    };

    eprintln!(
        "[fetch_package_by_name] Resolving package {} (version: {:?})...",
        clean_name, version_str
    );

    // 2. Split Namespace and Name (dots replaced with hyphens for Outer Wilds compatibility)
    let clean_name_normalized = clean_name.replace('.', "-");
    let parts: Vec<&str> = clean_name_normalized.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Ok(None);
    }
    let namespace = parts[0];
    let package_name = parts[1];

    // 3. Try memory search first
    {
        let packages_guard = state.packages.read().await;
        let mut found_pkg: Option<crate::models::shared::Package> = None;
        if let Some(ref gid) = game_id {
            if let Some(game_packages) = packages_guard.get(gid) {
                for pkg in game_packages {
                    if pkg.full_name.eq_ignore_ascii_case(&clean_name_normalized)
                        || pkg.name.eq_ignore_ascii_case(&clean_name_normalized)
                        || pkg.full_name.eq_ignore_ascii_case(&clean_name)
                        || pkg.name.eq_ignore_ascii_case(&clean_name)
                    {
                        found_pkg = Some(pkg.clone());
                        break;
                    }
                }
            }
        } else {
            for game_packages in packages_guard.values() {
                for pkg in game_packages {
                    if pkg.full_name.eq_ignore_ascii_case(&clean_name_normalized)
                        || pkg.name.eq_ignore_ascii_case(&clean_name_normalized)
                        || pkg.full_name.eq_ignore_ascii_case(&clean_name)
                        || pkg.name.eq_ignore_ascii_case(&clean_name)
                    {
                        found_pkg = Some(pkg.clone());
                        break;
                    }
                }
            }
        }

        if let Some(pkg) = found_pkg {
            if let Some(ref v) = version_str {
                if pkg.versions.iter().any(|ver| ver.version_number == *v) {
                    eprintln!("[fetch_package_by_name] Found package version in cache");
                    return Ok(Some(pkg));
                }
            } else {
                eprintln!("[fetch_package_by_name] Found package in cache");
                return Ok(Some(pkg));
            }
        }
    }

    // 4. Fallback to network
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let url = if let Some(ref v) = version_str {
        format!(
            "https://thunderstore.io/api/experimental/package/{}/{}/{}/",
            namespace, package_name, v
        )
    } else {
        format!(
            "https://thunderstore.io/api/experimental/package/{}/{}/",
            namespace, package_name
        )
    };

    eprintln!(
        "[fetch_package_by_name] Cache miss. Fetching from: {}",
        url
    );

    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;

    if response.status() == 404 {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(format!("Failed to fetch package: {}", response.status()));
    }

    let val: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

    let pkg = if version_str.is_some() {
        // Response is a single version object
        let ver_name = val["name"].as_str().unwrap_or(package_name).to_string();
        let ver_desc = val["description"].as_str().unwrap_or("").to_string();
        let ver_icon = val["icon"].as_str().unwrap_or("").to_string();
        let ver_num = val["version_number"].as_str().unwrap_or("").to_string();
        let ver_deps = val["dependencies"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        let ver_download = val["download_url"].as_str().unwrap_or("").to_string();
        let ver_downloads = val["downloads"].as_i64().unwrap_or(0);
        let ver_website = val["website_url"].as_str().unwrap_or("").to_string();
        let ver_full = val["full_name"].as_str().unwrap_or("").to_string();
        let ver_uuid = val["uuid4"].as_str().unwrap_or("").to_string();

        let version_struct = crate::models::shared::PackageVersion {
            name: ver_name,
            description: ver_desc,
            icon: ver_icon,
            version_number: ver_num,
            dependencies: ver_deps,
            download_url: ver_download,
            downloads: ver_downloads,
            website_url: ver_website,
            file_size: val["file_size"].as_u64().unwrap_or(0),
            uuid4: ver_uuid,
            full_name: ver_full,
            date_created: val["date_created"].as_str().unwrap_or("").to_string(),
            is_active: val["is_active"].as_bool().unwrap_or(true),
        };

        crate::models::shared::Package {
            name: package_name.to_string(),
            full_name: clean_name.to_string(),
            owner: namespace.to_string(),
            package_url: format!("https://thunderstore.io/package/{}/{}/", namespace, package_name),
            date_created: "".to_string(),
            date_updated: "".to_string(),
            uuid4: "".to_string(),
            rating_score: 0,
            is_pinned: false,
            is_deprecated: false,
            has_nsfw_content: false,
            categories: vec![],
            versions: vec![version_struct],
        }
    } else {
        // Response is the top-level package metadata with a "latest" version object
        let latest_val = &val["latest"];
        let ver_name = latest_val["name"].as_str().unwrap_or(package_name).to_string();
        let ver_desc = latest_val["description"].as_str().unwrap_or("").to_string();
        let ver_icon = latest_val["icon"].as_str().unwrap_or("").to_string();
        let ver_num = latest_val["version_number"].as_str().unwrap_or("").to_string();
        let ver_deps = latest_val["dependencies"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        let ver_download = latest_val["download_url"].as_str().unwrap_or("").to_string();
        let ver_downloads = latest_val["downloads"].as_i64().unwrap_or(0);
        let ver_website = latest_val["website_url"].as_str().unwrap_or("").to_string();
        let ver_full = latest_val["full_name"].as_str().unwrap_or("").to_string();
        let ver_uuid = latest_val["uuid4"].as_str().unwrap_or("").to_string();

        let version_struct = crate::models::shared::PackageVersion {
            name: ver_name,
            description: ver_desc,
            icon: ver_icon,
            version_number: ver_num,
            dependencies: ver_deps,
            download_url: ver_download,
            downloads: ver_downloads,
            website_url: ver_website,
            file_size: latest_val["file_size"].as_u64().unwrap_or(0),
            uuid4: ver_uuid,
            full_name: ver_full,
            date_created: latest_val["date_created"].as_str().unwrap_or("").to_string(),
            is_active: latest_val["is_active"].as_bool().unwrap_or(true),
        };

        let pkg_name = val["name"].as_str().unwrap_or(package_name).to_string();
        let pkg_full = val["full_name"].as_str().unwrap_or(&clean_name).to_string();
        let pkg_owner = val["owner"].as_str().unwrap_or(namespace).to_string();
        let pkg_url = val["package_url"].as_str().unwrap_or("").to_string();
        let pkg_uuid = val["uuid4"].as_str().unwrap_or("").to_string();
        let pkg_date_created = val["date_created"].as_str().unwrap_or("").to_string();
        let pkg_date_updated = val["date_updated"].as_str().unwrap_or("").to_string();
        let pkg_rating = val["rating_score"].as_i64().unwrap_or(0);
        let pkg_pinned = val["is_pinned"].as_bool().unwrap_or(false);
        let pkg_deprecated = val["is_deprecated"].as_bool().unwrap_or(false);

        let mut has_nsfw = false;
        let mut categories = vec![];
        if let Some(listings) = val["community_listings"].as_array() {
            if let Some(first_listing) = listings.first() {
                has_nsfw = first_listing["has_nsfw_content"].as_bool().unwrap_or(false);
                categories = first_listing["categories"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect();
            }
        }

        crate::models::shared::Package {
            name: pkg_name,
            full_name: pkg_full,
            owner: pkg_owner,
            package_url: pkg_url,
            date_created: pkg_date_created,
            date_updated: pkg_date_updated,
            uuid4: pkg_uuid,
            rating_score: pkg_rating,
            is_pinned: pkg_pinned,
            is_deprecated: pkg_deprecated,
            has_nsfw_content: has_nsfw,
            categories,
            versions: vec![version_struct],
        }
    };

    Ok(Some(pkg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("r2modmac-{}-{}", label, unique));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_test_nupkg(dll_bytes: &[u8]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut archive = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        archive
            .start_file(NEWTONSOFT_JSON_NETSTANDARD20_ENTRY, options)
            .unwrap();
        archive.write_all(dll_bytes).unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn risk_of_rain_2_detection_accepts_folder_name() {
        let path = std::path::Path::new("/tmp/steamapps/common/Risk of Rain 2");
        assert!(is_probably_risk_of_rain_2_game_dir(path));
    }

    #[test]
    fn risk_of_rain_2_detection_accepts_game_files() {
        let dir = temp_dir("ror2-detection");
        fs::write(dir.join("Risk of Rain 2.exe"), b"fake").unwrap();
        assert!(is_probably_risk_of_rain_2_game_dir(&dir));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn extracts_newtonsoft_dll_from_netstandard20_nupkg_entry() {
        let nupkg = make_test_nupkg(b"newtonsoft dll");
        let dll = extract_newtonsoft_json_dll_from_nupkg(&nupkg).unwrap();
        assert_eq!(dll, b"newtonsoft dll");
    }

    #[test]
    fn newtonsoft_download_url_interpolates_version() {
        let url = newtonsoft_json_nuget_url();
        assert_eq!(
            url,
            "https://api.nuget.org/v3-flatcontainer/newtonsoft.json/12.0.3/newtonsoft.json.12.0.3.nupkg"
        );
        assert!(!url.contains("{0}"));
    }

    #[test]
    fn writes_runtime_compat_asset_to_bepinex_core() {
        let dir = temp_dir("compat-asset");
        let asset = RuntimeCompatAsset {
            relative_path: std::path::PathBuf::from(ROR2_CROSSOVER_NEWTONSOFT_TARGET),
            bytes: b"newtonsoft dll".to_vec(),
            label: "Newtonsoft.Json",
        };

        write_runtime_compat_assets(&dir, &[asset], false).unwrap();
        let written = fs::read(dir.join(ROR2_CROSSOVER_NEWTONSOFT_TARGET)).unwrap();
        assert_eq!(written, b"newtonsoft dll");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_is_macos_game_dir() {
        let app_path = std::path::Path::new("/Applications/Balatro.app");
        assert!(is_macos_game_dir(app_path));

        let temp = temp_dir("is_macos_game_dir");
        let app_inside = temp.join("SomeGame.app");
        fs::create_dir_all(&app_inside).unwrap();
        assert!(is_macos_game_dir(&temp));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_packages_caching_serialization() {
        let pkg = crate::models::shared::Package {
            name: "TestMod".to_string(),
            full_name: "Test-TestMod".to_string(),
            owner: "".to_string(),
            package_url: "".to_string(),
            date_created: "".to_string(),
            date_updated: "2026-06-19".to_string(),
            uuid4: "some-uuid".to_string(),
            rating_score: 10,
            is_pinned: false,
            is_deprecated: false,
            has_nsfw_content: false,
            categories: vec![],
            versions: vec![],
        };
        let cache = GamePackagesCache {
            chunks: vec![ChunkCache {
                url: "http://example.com/chunk1.json".to_string(),
                packages: vec![pkg],
            }],
        };
        let serialized = serde_json::to_vec(&cache).unwrap();
        let decompressed: GamePackagesCache = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(decompressed.chunks.len(), 1);
        assert_eq!(decompressed.chunks[0].url, "http://example.com/chunk1.json");
        assert_eq!(decompressed.chunks[0].packages[0].name, "TestMod");
    }
}
