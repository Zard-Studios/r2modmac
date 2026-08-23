use super::legacy_mod_commands as legacy;
use crate::models::shared::normalize_zip_entry_name;
use crate::utils::persistent_download::{download_persistent, DownloadProgress};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub use super::legacy_mod_commands::{
    cancel_custom_mod_import, copy_mod_from_cache, delete_local_mod_payload, fetch_package_by_name,
    fetch_packages, get_available_categories, get_packages, import_custom_mod,
    import_embedded_custom_mod, inspect_custom_mod, install_local_mod, lookup_packages_by_names,
    open_mod_folder, refresh_local_mod_metadata, remove_mod, toggle_mod,
};

pub(crate) use super::legacy_mod_commands::{
    detect_unity_runtime_kind, download_official_macos_bepinex_runtime,
    extract_bepinex_pack_to_root, extract_version_number_from_full_name,
    normalize_macos_doorstop_config_file, point_game_doorstop_ini_at_tree, relocate_bepinex_tree,
};

const APP_USER_AGENT: &str = concat!("r2modmac/", env!("CARGO_PKG_VERSION"));
const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SINGLE_FILE_BYTES: u64 = 768 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_ARCHIVE_PATH_CHARS: usize = 240;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 250;

static PREFLIGHT_CANCELLED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn begin_mod_operations() -> bool {
    PREFLIGHT_CANCELLED.store(false, Ordering::Release);
    legacy::begin_mod_operations()
}

#[tauri::command]
pub fn cancel_mod_operations() -> bool {
    PREFLIGHT_CANCELLED.store(true, Ordering::Release);
    legacy::cancel_mod_operations()
}

#[tauri::command]
pub fn mod_operations_cancelled() -> bool {
    PREFLIGHT_CANCELLED.load(Ordering::Acquire) || legacy::mod_operations_cancelled()
}

fn validate_standard_https_url(raw_url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw_url).map_err(|_| "Invalid mod download URL".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.host_str().is_none()
    {
        return Err("Mod downloads must use a standard HTTPS URL".to_string());
    }
    Ok(())
}

fn validate_single_path_component(value: &str, label: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_ARCHIVE_PATH_CHARS {
        return Err(format!("Blocked invalid {} in mod archive", label));
    }
    if trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.chars().any(|ch| ch == '\0' || ch.is_control())
    {
        return Err(format!(
            "Blocked unsafe {} in mod archive: {}",
            label, trimmed
        ));
    }

    let mut components = Path::new(trimmed).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "Blocked unsafe {} in mod archive: {}",
            label, trimmed
        ));
    }
    Ok(())
}

fn is_zip_symlink(file: &zip::read::ZipFile<'_>) -> bool {
    file.unix_mode()
        .map(|mode| (mode & 0o170000) == 0o120000)
        .unwrap_or(false)
}

fn read_limited(file: &mut zip::read::ZipFile<'_>, max_bytes: u64) -> Result<Vec<u8>, String> {
    if file.size() > max_bytes {
        return Err("Archive manifest is too large".to_string());
    }
    let mut bytes = Vec::with_capacity(file.size().min(max_bytes) as usize);
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err("Archive manifest is too large".to_string());
    }
    Ok(bytes)
}

fn parse_manifest_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    crate::utils::manifest_json::parse_manifest_bytes(bytes, "mod archive")
}

fn validate_manifest(file: &mut zip::read::ZipFile<'_>) -> Result<(), String> {
    let bytes = read_limited(file, MAX_MANIFEST_BYTES)?;
    let manifest = parse_manifest_json(&bytes)?;

    if let Some(unique_name) = manifest.get("uniqueName").and_then(|value| value.as_str()) {
        validate_single_path_component(unique_name, "Outer Wilds uniqueName")?;
    }
    Ok(())
}

fn validate_archive(path: &Path, mod_name: &str) -> Result<(), String> {
    validate_single_path_component(mod_name, "package name")?;

    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Downloaded mod payload is not a file".to_string());
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(format!(
            "Downloaded mod archive exceeds the {} MB safety limit",
            MAX_ARCHIVE_BYTES / 1024 / 1024
        ));
    }

    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Downloaded mod is not a valid zip archive: {}", error))?;
    if archive.len() == 0 {
        return Err("Downloaded mod archive is empty".to_string());
    }
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "Downloaded mod archive contains too many entries ({} > {})",
            archive.len(),
            MAX_ARCHIVE_ENTRIES
        ));
    }

    let mut normalized_paths = HashSet::new();
    let mut total_uncompressed = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let raw_name = entry.name().to_string();
        if is_zip_symlink(&entry) {
            return Err(format!(
                "Blocked symlink entry in mod archive: {}",
                raw_name
            ));
        }

        let normalized = normalize_zip_entry_name(&raw_name)
            .ok_or_else(|| format!("Blocked unsafe archive path: {}", raw_name))?;
        if normalized.chars().count() > MAX_ARCHIVE_PATH_CHARS {
            return Err(format!("Blocked overlong archive path: {}", normalized));
        }
        if !normalized_paths.insert(normalized.to_ascii_lowercase()) {
            return Err(format!("Blocked duplicate archive path: {}", normalized));
        }

        if raw_name.replace('\\', "/").ends_with('/') {
            continue;
        }

        if entry.size() > MAX_SINGLE_FILE_BYTES {
            return Err(format!(
                "Blocked oversized file in mod archive: {}",
                normalized
            ));
        }
        total_uncompressed = total_uncompressed
            .checked_add(entry.size())
            .ok_or_else(|| "Archive size accounting overflowed".to_string())?;
        if total_uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(format!(
                "Downloaded mod expands beyond the {} MB safety limit",
                MAX_UNCOMPRESSED_BYTES / 1024 / 1024
            ));
        }

        let compressed = entry.compressed_size();
        if compressed == 0 && entry.size() > 0 {
            return Err(format!(
                "Blocked suspicious zero-compressed-size entry: {}",
                normalized
            ));
        }
        if compressed > 0
            && entry.size() > 10 * 1024 * 1024
            && entry.size() / compressed.max(1) > MAX_COMPRESSION_RATIO
        {
            return Err(format!(
                "Blocked suspicious compression ratio for {}",
                normalized
            ));
        }

        if normalized
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("manifest.json"))
        {
            validate_manifest(&mut entry)?;
        }
    }

    Ok(())
}
/// The client this command downloads with.
///
/// Shared rather than built per install: a fresh client is a fresh connection
/// pool, so every mod in a batch paid for its own TCP and TLS handshake instead
/// of reusing the one the mod before it had already opened. The settings are
/// exactly the ones that were here before, so nothing about timeouts or
/// redirects changes — only that the pool now survives between installs.
static INSTALL_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn install_client() -> Result<&'static reqwest::Client, String> {
    if let Some(client) = INSTALL_CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::limited(10))
        .pool_max_idle_per_host(8)
        .build()
        .map_err(|error| error.to_string())?;
    Ok(INSTALL_CLIENT.get_or_init(|| client))
}

#[tauri::command]
pub async fn install_mod(
    app: AppHandle,
    profile_id: String,
    download_url: String,
    mod_name: String,
    game_path: String,
    use_profile_cache: Option<bool>,
) -> Result<serde_json::Value, String> {
    validate_standard_https_url(&download_url)?;
    validate_single_path_component(&mod_name, "package name")?;

    let client = install_client()?;
    let cache_dir = crate::utils::paths::app_cache_dir(&app).map_err(|error| error.to_string())?;
    let progress_app = app.clone();
    let progress_mod_name = mod_name.clone();

    let completed = download_persistent(
        &client,
        &cache_dir,
        &download_url,
        &mod_name,
        &PREFLIGHT_CANCELLED,
        move |progress: DownloadProgress| {
            let progress_percent = progress
                .total_bytes
                .filter(|total| *total > 0)
                .map(|total| {
                    ((progress.downloaded_bytes as f64 / total as f64) * 100.0)
                        .round()
                        .clamp(0.0, 100.0) as u8
                })
                .unwrap_or(if progress.done { 100 } else { 0 });
            let _ = progress_app.emit(
                "mod-download-progress",
                serde_json::json!({
                    "mod_name": progress_mod_name.as_str(),
                    "downloaded_bytes": progress.downloaded_bytes,
                    "total_bytes": progress.total_bytes,
                    "speed_bps": progress.speed_bps,
                    "progress_percent": progress_percent,
                    "done": progress.done
                }),
            );
        },
    )
    .await?;

    if let Err(error) = validate_archive(&completed.payload_path, &mod_name) {
        completed.cleanup();
        return Err(error);
    }

    legacy::install_mod(
        app,
        profile_id,
        download_url,
        mod_name,
        game_path,
        use_profile_cache,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static ARCHIVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn archive_with_entries(entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "r2modmac-secure-preflight-{}-{}.zip",
            std::process::id(),
            ARCHIVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let file = fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::default();
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn rejects_outer_wilds_unique_name_traversal() {
        let archive = archive_with_entries(&[(
            "manifest.json",
            br#"{"uniqueName":"../../outside","dependencies":[]}"#,
        )]);
        let error = validate_archive(&archive, "Author-Mod-1.0.0").unwrap_err();
        assert!(error.contains("uniqueName"));
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn rejects_archive_entry_traversal() {
        let archive = archive_with_entries(&[("../../outside.txt", b"bad")]);
        let error = validate_archive(&archive, "Author-Mod-1.0.0").unwrap_err();
        assert!(error.contains("unsafe archive path"), "{error}");
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn accepts_safe_archive() {
        let archive = archive_with_entries(&[
            (
                "manifest.json",
                br#"{"uniqueName":"Author.SafeMod","dependencies":[]}"#,
            ),
            ("SafeMod.dll", b"dll"),
        ]);
        validate_archive(&archive, "Author-SafeMod-1.0.0").unwrap();
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn accepts_utf8_bom_prefixed_manifest() {
        let manifest = b"\xEF\xBB\xBF{\"name\":\"ReturnsAPI\",\"version_number\":\"0.1.58\",\"dependencies\":[]}";
        let archive =
            archive_with_entries(&[("manifest.json", manifest), ("main.lua", b"return {}")]);
        validate_archive(&archive, "ReturnsAPI-ReturnsAPI-0.1.58").unwrap();
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn rejects_utf8_bom_malformed_json_manifest() {
        let manifest = b"\xEF\xBB\xBF{\"name\":";
        let archive = archive_with_entries(&[("manifest.json", manifest)]);
        let error = validate_archive(&archive, "Author-Mod-1.0.0").unwrap_err();
        assert!(
            error.contains("Invalid manifest.json in mod archive"),
            "{error}"
        );
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn accepts_well_formed_utf16_le_bom_manifest() {
        let mut manifest = vec![0xFF, 0xFE];
        for unit in r#"{"name":"UnicodeMod","version_number":"1.0.0"}"#.encode_utf16() {
            manifest.extend_from_slice(&unit.to_le_bytes());
        }
        let archive = archive_with_entries(&[("manifest.json", &manifest)]);
        validate_archive(&archive, "Author-UnicodeMod-1.0.0").unwrap();
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn rejects_truncated_utf16_le_bom_manifest() {
        // Odd byte count: not decodable as UTF-16 under any interpretation.
        let manifest = b"\xFF\xFE{\x00\"name\x00\":\x00}";
        let archive = archive_with_entries(&[("manifest.json", manifest)]);
        let error = validate_archive(&archive, "Author-Mod-1.0.0").unwrap_err();
        assert!(
            error.contains("Invalid manifest.json in mod archive"),
            "{error}"
        );
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn rejects_utf8_bom_outer_wilds_unique_name_traversal() {
        let manifest = b"\xEF\xBB\xBF{\"uniqueName\":\"../../outside\",\"dependencies\":[]}";
        let archive = archive_with_entries(&[("manifest.json", manifest)]);
        let error = validate_archive(&archive, "Author-Mod-1.0.0").unwrap_err();
        assert!(error.contains("uniqueName"), "{error}");
        let _ = fs::remove_file(archive);
    }

    #[test]
    fn accepts_utf8_bom_nested_manifest() {
        let manifest = b"\xEF\xBB\xBF{\"name\":\"ReturnsAPI\",\"version_number\":\"0.1.58\",\"dependencies\":[]}";
        let archive = archive_with_entries(&[("NestedDir/manifest.json", manifest)]);
        validate_archive(&archive, "ReturnsAPI-ReturnsAPI-0.1.58").unwrap();
        let _ = fs::remove_file(archive);
    }
}
