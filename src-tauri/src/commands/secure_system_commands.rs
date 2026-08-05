use crate::models::shared::UpdateInfo;
use std::fs;
#[cfg(target_os = "windows")]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub use super::legacy_system_commands::{
    alert_dialog, compare_versions, confirm_dialog, fetch_communities, fetch_community_images,
    fetch_text_content, get_username, read_image, resolve_community_platforms, select_file,
    select_folder, select_import_path, PlatformLookupInput,
};

const MAX_UPDATE_BYTES: u64 = 512 * 1024 * 1024;
const EXPECTED_BUNDLE_IDENTIFIER: &str = "com.r2modmac";

fn expected_update_filename_for(os: &str, arch: &str) -> Result<String, String> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("r2modmac_macos_aarch64.dmg".to_string()),
        ("macos", "x86_64") => Ok("r2modmac_macos_x86_64.dmg".to_string()),
        ("windows", "x86_64") => Ok("r2modmac_windows_x64.zip".to_string()),
        ("windows", "x86") | ("windows", "i686") => Ok("r2modmac_windows_x86.zip".to_string()),
        ("windows", "aarch64") => Ok("r2modmac_windows_arm64.zip".to_string()),
        ("linux", "x86_64") => Ok("r2modmac_linux_x64.tar.gz".to_string()),
        ("linux", "aarch64") => Ok("r2modmac_linux_arm64.tar.gz".to_string()),
        (os, arch) => Err(format!(
            "Automatic updates are not supported on {} {}",
            os, arch
        )),
    }
}

fn expected_update_filename() -> Result<String, String> {
    expected_update_filename_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn select_update_asset<'a>(
    assets: &'a [serde_json::Value],
    os: &str,
    arch: &str,
) -> Option<&'a serde_json::Value> {
    let expected = expected_update_filename_for(os, arch).ok()?;
    assets.iter().find(|asset| {
        asset
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|name| name == expected)
    })
}

#[tauri::command]
pub async fn check_update(current_version: String) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/repos/Zard-Studios/r2modmac/releases/latest")
        .header("User-Agent", "r2modmac-updater")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| format!("Request failed: {}", error))?;

    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Ok(UpdateInfo {
            available: false,
            version: format!("v{}", current_version),
            notes: String::new(),
            download_url: None,
        });
    }
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }

    let release: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("Parse error: {}", error))?;
    let tag_name = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Release is missing tag_name".to_string())?;
    let clean_tag = tag_name.trim_start_matches('v');
    let available = compare_versions(clean_tag, &current_version);
    let assets = release
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let asset = select_update_asset(assets, std::env::consts::OS, std::env::consts::ARCH);
    let download_url = asset
        .and_then(|asset| asset.get("browser_download_url"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    eprintln!(
        "[check_update] Detected OS: {}, architecture: {}, selected asset: {:?}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        asset
            .and_then(|asset| asset.get("name"))
            .and_then(serde_json::Value::as_str)
    );

    Ok(UpdateInfo {
        available,
        version: tag_name.to_string(),
        notes: release
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        download_url,
    })
}

fn validate_update_url(raw_url: &str) -> Result<(reqwest::Url, String), String> {
    let url = reqwest::Url::parse(raw_url).map_err(|_| "Invalid update URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Update URL is not an official r2modmac GitHub release asset".to_string());
    }

    let segments = url
        .path_segments()
        .ok_or_else(|| "Invalid update URL path".to_string())?
        .collect::<Vec<_>>();
    if segments.len() != 6
        || !segments[0].eq_ignore_ascii_case("Zard-Studios")
        || segments[1] != "r2modmac"
        || segments[2] != "releases"
        || segments[3] != "download"
        || segments[4].is_empty()
    {
        return Err("Update URL is not an official r2modmac GitHub release asset".to_string());
    }

    let filename = urlencoding::decode(segments[5])
        .map_err(|_| "Invalid encoded update filename".to_string())?
        .into_owned();
    let expected = expected_update_filename()?;
    if filename != expected
        || filename.len() > 128
        || !filename
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(format!(
            "Update asset does not match this computer (expected {})",
            expected
        ));
    }

    Ok((url, filename))
}

fn validate_final_download_url(url: &reqwest::Url) -> Result<(), String> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err("Update download redirected to an unsafe URL".to_string());
    }

    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host == "github.com" || host.ends_with(".githubusercontent.com") {
        Ok(())
    } else {
        Err("Update download redirected outside GitHub".to_string())
    }
}

async fn download_update(
    app: &AppHandle,
    url: reqwest::Url,
    file_path: &Path,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("r2modmac-updater/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Update request failed: {}", error))?
        .error_for_status()
        .map_err(|error| format!("Update download failed: {}", error))?;
    validate_final_download_url(response.url())?;

    if response
        .content_length()
        .is_some_and(|size| size > MAX_UPDATE_BYTES)
    {
        return Err(format!(
            "Update exceeds the {} MB download limit",
            MAX_UPDATE_BYTES / 1024 / 1024
        ));
    }

    let total_size = response.content_length();
    let mut output = fs::File::create(file_path).map_err(|error| error.to_string())?;
    let mut downloaded = 0u64;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Update download was interrupted: {}", error))?
    {
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "Update size accounting overflowed".to_string())?;
        if downloaded > MAX_UPDATE_BYTES {
            drop(output);
            let _ = fs::remove_file(file_path);
            return Err(format!(
                "Update exceeds the {} MB download limit",
                MAX_UPDATE_BYTES / 1024 / 1024
            ));
        }
        output
            .write_all(&chunk)
            .map_err(|error| error.to_string())?;

        if let Some(total) = total_size.filter(|total| *total > 0) {
            let percent = ((downloaded as f64 / total as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            let _ = app.emit("update-progress", percent);
        }
    }

    output.sync_all().map_err(|error| error.to_string())?;
    if let Some(total) = total_size {
        if downloaded != total {
            let _ = fs::remove_file(file_path);
            return Err(format!(
                "Update download is incomplete: received {} of {} bytes",
                downloaded, total
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn stage_windows_update(file_path: &Path, temp_dir: &Path) -> Result<PathBuf, String> {
    let file = fs::File::open(file_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Downloaded update is not a valid zip: {}", error))?;
    if archive.len() > 32 {
        return Err("Windows update archive contains too many files".to_string());
    }

    let mut executable = archive
        .by_name("r2modmac.exe")
        .map_err(|_| "Windows update does not contain r2modmac.exe at archive root".to_string())?;
    if executable.is_dir() || executable.size() == 0 || executable.size() > MAX_UPDATE_BYTES {
        return Err("Windows update contains an invalid executable".to_string());
    }

    let staged_path = temp_dir.join("r2modmac.exe.new");
    let mut output = fs::File::create(&staged_path).map_err(|error| error.to_string())?;
    let copied = std::io::copy(&mut executable, &mut output).map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())?;
    if copied == 0 || copied > MAX_UPDATE_BYTES {
        let _ = fs::remove_file(&staged_path);
        return Err("Windows update executable failed size validation".to_string());
    }
    Ok(staged_path)
}

#[cfg(target_os = "windows")]
fn launch_windows_replacement(
    app: &AppHandle,
    staged_exe: &Path,
    temp_dir: &Path,
) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    if current_exe.file_name().and_then(|name| name.to_str()) != Some("r2modmac.exe") {
        return Err("Cannot auto-update an unexpected executable path".to_string());
    }

    let script_path = temp_dir.join("r2modmac_updater.bat");
    let script = r#"@echo off
setlocal
set "NEW_EXE=%~1"
set "CURRENT_EXE=%~2"
set /a ATTEMPTS=0
:replace
timeout /t 1 /nobreak >nul
move /y "%NEW_EXE%" "%CURRENT_EXE%" >nul 2>&1
if not exist "%NEW_EXE%" goto replaced
set /a ATTEMPTS+=1
if %ATTEMPTS% LSS 30 goto replace
exit /b 1
:replaced
start "" "%CURRENT_EXE%"
del "%~f0"
"#;
    fs::write(&script_path, script).map_err(|error| error.to_string())?;

    Command::new("cmd")
        .arg("/c")
        .arg(&script_path)
        .arg(staged_exe)
        .arg(&current_exe)
        .spawn()
        .map_err(|error| format!("Failed to launch updater script: {}", error))?;
    app.exit(0);
    Ok(())
}

#[cfg(target_os = "macos")]
fn command_succeeded(command: &mut Command, context: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("{}: {}", context, error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}: {}",
            context,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(target_os = "macos")]
fn verify_macos_bundle(app_path: &Path) -> Result<(), String> {
    let executable = app_path.join("Contents/MacOS/r2modmac");
    let info_plist = app_path.join("Contents/Info.plist");
    if !app_path.is_dir() || !executable.is_file() || !info_plist.is_file() {
        return Err("Downloaded macOS update contains an incomplete app bundle".to_string());
    }

    let output = Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg("Print :CFBundleIdentifier")
        .arg(&info_plist)
        .output()
        .map_err(|error| format!("Could not inspect update bundle identity: {}", error))?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != EXPECTED_BUNDLE_IDENTIFIER
    {
        return Err("Downloaded macOS update has an unexpected bundle identifier".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn current_macos_app_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    if executable.to_string_lossy().contains("/target/") {
        return Err("Cannot auto-update a development build".to_string());
    }
    let app_path = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| "Could not resolve the current app bundle".to_string())?
        .to_path_buf();
    if app_path.extension().and_then(|value| value.to_str()) != Some("app") {
        return Err("Current executable is not inside a macOS app bundle".to_string());
    }
    verify_macos_bundle(&app_path)?;
    Ok(app_path)
}

#[cfg(target_os = "macos")]
fn stage_macos_update(file_path: &Path, temp_dir: &Path) -> Result<PathBuf, String> {
    let mount_point = temp_dir.join("dmg_mount");
    let staged_app = temp_dir.join("r2modmac-staged.app");
    fs::create_dir_all(&mount_point).map_err(|error| error.to_string())?;

    command_succeeded(
        Command::new("/usr/bin/hdiutil")
            .arg("attach")
            .arg(file_path)
            .arg("-mountpoint")
            .arg(&mount_point)
            .arg("-nobrowse")
            .arg("-quiet")
            .arg("-readonly"),
        "Failed to mount update image",
    )?;

    let source_app = mount_point.join("r2modmac.app");
    let copy_result = if source_app.is_dir() {
        command_succeeded(
            Command::new("/usr/bin/ditto")
                .arg(&source_app)
                .arg(&staged_app),
            "Failed to stage update app",
        )
    } else {
        Err("Mounted update does not contain r2modmac.app".to_string())
    };

    let detach_result = command_succeeded(
        Command::new("/usr/bin/hdiutil")
            .arg("detach")
            .arg(&mount_point)
            .arg("-force")
            .arg("-quiet"),
        "Failed to detach update image",
    );
    copy_result?;
    detach_result?;
    verify_macos_bundle(&staged_app)?;
    Ok(staged_app)
}

#[cfg(target_os = "macos")]
fn launch_macos_replacement(
    app: &AppHandle,
    staged_app: &Path,
    temp_dir: &Path,
) -> Result<(), String> {
    let current_app = current_macos_app_path()?;
    let script_path = temp_dir.join("update.sh");
    let script = r#"#!/bin/sh
set -eu
PID="$1"
APP_PATH="$2"
STAGED_APP="$3"
UPDATE_DIR="$4"
BACKUP_APP="${APP_PATH}.r2modmac-backup"

while kill -0 "$PID" 2>/dev/null; do sleep 0.5; done
rm -rf "$BACKUP_APP"
if [ -d "$APP_PATH" ]; then mv "$APP_PATH" "$BACKUP_APP"; fi
if ! mv "$STAGED_APP" "$APP_PATH"; then
    rm -rf "$APP_PATH"
    if [ -d "$BACKUP_APP" ]; then mv "$BACKUP_APP" "$APP_PATH"; fi
    exit 1
fi
if ! /usr/bin/open "$APP_PATH"; then
    rm -rf "$APP_PATH"
    if [ -d "$BACKUP_APP" ]; then mv "$BACKUP_APP" "$APP_PATH"; fi
    /usr/bin/open "$APP_PATH" || true
    exit 1
fi
rm -rf "$BACKUP_APP"
rm -rf "$UPDATE_DIR"
"#;
    fs::write(&script_path, script).map_err(|error| error.to_string())?;

    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(&script_path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script_path, permissions).map_err(|error| error.to_string())?;

    Command::new("/bin/sh")
        .arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&current_app)
        .arg(staged_app)
        .arg(temp_dir)
        .spawn()
        .map_err(|error| format!("Failed to launch updater helper: {}", error))?;
    app.exit(0);
    Ok(())
}

#[cfg(target_os = "linux")]
fn stage_linux_update(file_path: &Path, temp_dir: &Path) -> Result<PathBuf, String> {
    use flate2::read::GzDecoder;
    use std::os::unix::fs::PermissionsExt;

    let file = fs::File::open(file_path).map_err(|error| error.to_string())?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let staged_path = temp_dir.join("r2modmac.new");
    let mut found_executable = false;
    let mut entry_count = 0usize;

    for entry in archive
        .entries()
        .map_err(|error| format!("Downloaded update is not a valid tar.gz: {}", error))?
    {
        entry_count += 1;
        if entry_count > 8 {
            return Err("Linux update archive contains too many entries".to_string());
        }
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path().map_err(|error| error.to_string())?;
        if path.as_ref() != Path::new("r2modmac")
            || !entry.header().entry_type().is_file()
            || found_executable
        {
            return Err(
                "Linux update archive must contain only the r2modmac executable at archive root"
                    .to_string(),
            );
        }
        let declared_size = entry.header().size().map_err(|error| error.to_string())?;
        if declared_size == 0 || declared_size > MAX_UPDATE_BYTES {
            return Err("Linux update contains an invalid executable".to_string());
        }
        let mut output = fs::File::create(&staged_path).map_err(|error| error.to_string())?;
        let copied = std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        if copied == 0 || copied != declared_size || copied > MAX_UPDATE_BYTES {
            let _ = fs::remove_file(&staged_path);
            return Err("Linux update executable failed size validation".to_string());
        }
        let mut permissions = fs::metadata(&staged_path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&staged_path, permissions).map_err(|error| error.to_string())?;
        found_executable = true;
    }

    if !found_executable {
        return Err("Linux update does not contain r2modmac at archive root".to_string());
    }
    Ok(staged_path)
}

#[cfg(target_os = "linux")]
fn launch_linux_replacement(
    app: &AppHandle,
    staged_exe: &Path,
    temp_dir: &Path,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    if current_exe.file_name().and_then(|name| name.to_str()) != Some("r2modmac")
        || current_exe.to_string_lossy().contains("/target/")
    {
        return Err("Cannot auto-update an unexpected Linux executable path".to_string());
    }

    let script_path = temp_dir.join("update.sh");
    let script = r#"#!/bin/sh
set -eu
PID="$1"
CURRENT_EXE="$2"
STAGED_EXE="$3"
UPDATE_DIR="$4"
BACKUP_EXE="${CURRENT_EXE}.r2modmac-backup"

while kill -0 "$PID" 2>/dev/null; do sleep 0.5; done
rm -f "$BACKUP_EXE"
mv "$CURRENT_EXE" "$BACKUP_EXE"
if ! mv "$STAGED_EXE" "$CURRENT_EXE"; then
    mv "$BACKUP_EXE" "$CURRENT_EXE"
    exit 1
fi
chmod 755 "$CURRENT_EXE"
if "$CURRENT_EXE" >/dev/null 2>&1 & then
    NEW_PID=$!
    sleep 2
    if kill -0 "$NEW_PID" 2>/dev/null; then
        rm -f "$BACKUP_EXE"
        rm -rf "$UPDATE_DIR"
        exit 0
    fi
fi
rm -f "$CURRENT_EXE"
mv "$BACKUP_EXE" "$CURRENT_EXE"
chmod 755 "$CURRENT_EXE"
"$CURRENT_EXE" >/dev/null 2>&1 &
exit 1
"#;
    fs::write(&script_path, script).map_err(|error| error.to_string())?;
    let mut permissions = fs::metadata(&script_path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script_path, permissions).map_err(|error| error.to_string())?;

    Command::new("/bin/sh")
        .arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&current_exe)
        .arg(staged_exe)
        .arg(temp_dir)
        .spawn()
        .map_err(|error| format!("Failed to launch Linux updater helper: {}", error))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    let (url, filename) = validate_update_url(&download_url)?;
    let temp_dir = app
        .path()
        .temp_dir()
        .map_err(|error| error.to_string())?
        .join("r2modmac_update");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&temp_dir).map_err(|error| error.to_string())?;

    let file_path = temp_dir.join(filename);
    if let Err(error) = download_update(&app, url, &file_path).await {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(error);
    }

    #[cfg(target_os = "windows")]
    {
        let staged_exe = stage_windows_update(&file_path, &temp_dir)?;
        launch_windows_replacement(&app, &staged_exe, &temp_dir)
    }

    #[cfg(target_os = "macos")]
    {
        let staged_app = stage_macos_update(&file_path, &temp_dir)?;
        launch_macos_replacement(&app, &staged_app, &temp_dir)
    }

    #[cfg(target_os = "linux")]
    {
        let staged_exe = stage_linux_update(&file_path, &temp_dir)?;
        launch_linux_replacement(&app, &staged_exe, &temp_dir)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = app;
        let _ = file_path;
        Err("Automatic updates are unsupported on this operating system".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_release_update_urls() {
        for url in [
            "http://github.com/Zard-Studios/r2modmac/releases/download/v1/r2modmac_macos_aarch64.dmg",
            "https://example.com/r2modmac_macos_aarch64.dmg",
            "https://github.com/other/repo/releases/download/v1/r2modmac_macos_aarch64.dmg",
            "https://github.com/Zard-Studios/r2modmac/releases/download/v1/../../evil.dmg",
        ] {
            assert!(validate_update_url(url).is_err());
        }
    }

    #[test]
    fn expected_update_name_is_path_safe() {
        let filename = expected_update_filename().unwrap();
        assert!(filename
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')));
        assert!(!filename.contains('/'));
        assert!(!filename.contains('\\'));
    }

    #[test]
    fn maps_linux_architectures_to_release_assets() {
        assert_eq!(
            expected_update_filename_for("linux", "x86_64").unwrap(),
            "r2modmac_linux_x64.tar.gz"
        );
        assert_eq!(
            expected_update_filename_for("linux", "aarch64").unwrap(),
            "r2modmac_linux_arm64.tar.gz"
        );
        assert!(expected_update_filename_for("linux", "x86").is_err());
    }

    #[test]
    fn selects_only_exact_linux_architecture_asset() {
        let assets = vec![
            serde_json::json!({"name":"r2modmac_linux_arm64.tar.gz"}),
            serde_json::json!({"name":"r2modmac_linux_x64.tar.gz"}),
        ];
        assert_eq!(
            select_update_asset(&assets, "linux", "x86_64")
                .unwrap()
                .get("name")
                .and_then(serde_json::Value::as_str),
            Some("r2modmac_linux_x64.tar.gz")
        );
        assert_eq!(
            select_update_asset(&assets, "linux", "aarch64")
                .unwrap()
                .get("name")
                .and_then(serde_json::Value::as_str),
            Some("r2modmac_linux_arm64.tar.gz")
        );
    }
}
