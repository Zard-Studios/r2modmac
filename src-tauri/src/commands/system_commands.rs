use std::fs;
use tauri::{command, AppHandle, Emitter, Manager};
use crate::models::shared::*;

#[command]
pub async fn fetch_communities() -> Result<Vec<serde_json::Value>, String> {
    let mut url = Some("https://thunderstore.io/api/experimental/community/".to_string());
    let mut all_results = Vec::new();

    while let Some(current_url) = url {
        let resp = reqwest::get(&current_url).await.map_err(|e| e.to_string())?;
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        
        if let Some(results) = json.get("results").and_then(|v| v.as_array()) {
            eprintln!("[fetch_communities] Fetched {} communities from {}", results.len(), current_url);
            all_results.extend(results.clone());
        }

        // API uses pagination.next_link instead of "next"
        url = json.get("pagination")
            .and_then(|p| p.get("next_link"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    
    eprintln!("[fetch_communities] Total communities fetched: {}", all_results.len());
    Ok(all_results)
}

#[command]
pub async fn fetch_community_images() -> Result<std::collections::HashMap<String, String>, String> {
    let url = "https://thunderstore.io/communities/";
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let html = resp.text().await.map_err(|e| e.to_string())?;
    
    let mut images = std::collections::HashMap::new();
    
    // Regex for preload links
    // Matches: <link rel="preload" href="https://gcdn.thunderstore.io/live/community/risk-of-rain-2/..." as="image">
    let re_preload = regex::Regex::new(r#"<link rel="preload" href="(https://gcdn\.thunderstore\.io/live/community/([^/]+)/[^"]+)" as="image">"#)
        .map_err(|e| e.to_string())?;
    
    for cap in re_preload.captures_iter(&html) {
        if let (Some(url), Some(id)) = (cap.get(1), cap.get(2)) {
            images.insert(id.as_str().to_string(), url.as_str().to_string());
        }
    }

    // Regex for img tags (fallback)
    // Matches: <img ... src="https://gcdn.thunderstore.io/live/community/risk-of-rain-2/..." ...>
    let re_img = regex::Regex::new(r#"src="(https://gcdn\.thunderstore\.io/live/community/([^/]+)/[^"]+)""#)
        .map_err(|e| e.to_string())?;
        
    for cap in re_img.captures_iter(&html) {
        if let (Some(url), Some(id)) = (cap.get(1), cap.get(2)) {
             images.entry(id.as_str().to_string()).or_insert(url.as_str().to_string());
        }
    }
    
    Ok(images)
}

#[command]
pub async fn fetch_text_content(url: String) -> Result<String, String> {
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

#[command]
pub async fn confirm_dialog(app: AppHandle, title: String, message: String) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_dialog::MessageDialogButtons;
    
    let window = app.get_webview_window("main").ok_or("Main window not found")?;

    let ans = app.dialog()
        .message(message)
        .title(title)
        .buttons(MessageDialogButtons::OkCancel)
        .parent(&window)
        .blocking_show();
        
    Ok(ans)
}

#[command]
pub async fn alert_dialog(app: AppHandle, title: String, message: String) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_dialog::MessageDialogButtons;
    
    let window = app.get_webview_window("main").ok_or("Main window not found")?;
    
    app.dialog()
        .message(message)
        .title(title)
        .buttons(MessageDialogButtons::Ok)
        .parent(&window)
        .blocking_show();
        
    Ok(())
}

#[command]
pub async fn select_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file_path = app.dialog().file().blocking_pick_folder();
    Ok(file_path.map(|p| p.to_string()))
}

#[command]
pub async fn select_file(app: AppHandle, filters: Option<Vec<FileFilter>>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();

    if let Some(fs) = filters {
        for f in fs {
            // Convert Vec<String> to Vec<&str>
            let exts: Vec<&str> = f.extensions.iter().map(|s| s.as_str()).collect();
            builder = builder.add_filter(f.name, &exts);
        }
    } else {
        // Default to r2modman profile if no filters provided
        builder = builder.add_filter("r2modman Profile", &["r2z", "zip"]);
    }

    let file_path = builder.blocking_pick_file();
    Ok(file_path.map(|p| p.to_string()))
}

#[command]
pub async fn read_image(path: String) -> Result<Option<String>, String> {
    use base64::Engine;
    let path_buf = std::path::PathBuf::from(&path);
    if !path_buf.exists() {
        return Ok(None);
    }
    
    let bytes = fs::read(&path_buf).map_err(|e| e.to_string())?;
    let base64_str = base64::engine::general_purpose::STANDARD.encode(&bytes);
    
    // Determine mime type based on extension
    let extension = path_buf.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let mime = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream"
    };
    
    Ok(Some(format!("data:{};base64,{}", mime, base64_str)))
}

#[command]
pub async fn check_update(current_version: String) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/repos/Zard-Studios/r2modmac/releases/latest")
        .header("User-Agent", "r2modmac-updater")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        let remaining = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");
        eprintln!(
            "[check_update] GitHub API rate limited (status: {}, remaining: {}). Skipping this check.",
            resp.status(),
            remaining
        );
        return Ok(UpdateInfo {
            available: false,
            version: format!("v{}", current_version),
            notes: String::new(),
            download_url: None,
        });
    }

    if !resp.status().is_success() {
        return Err(format!("GitHub API error: {}", resp.status()));
    }

    let release: GithubRelease = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
    
    // Simple version comparison (naive string compare for now, ideally use semver)
    // Assume tag_name is "vX.X.X" and current_version is "X.X.X"
    let clean_tag = release.tag_name.trim_start_matches('v');
    
    // Use semver crate if available, or simple split compare
    let is_newer = compare_versions(clean_tag, &current_version);
    
    // Detect system architecture
    let arch = std::env::consts::ARCH; // "aarch64" for Apple Silicon, "x86_64" for Intel
    eprintln!("[check_update] Detected architecture: {}", arch);
    
    // Map architecture to expected asset name patterns
    let arch_pattern = match arch {
        "aarch64" => "aarch64",
        "x86_64" => "x64",
        _ => "universal", // Fallback to universal for unknown archs
    };
    
    // Find matching DMG/archive: prioritize exact arch match, fallback to universal
    let asset = release.assets.iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            (name.ends_with(".dmg") || name.ends_with(".tar.gz") || name.ends_with(".zip"))
                && name.contains(arch_pattern)
        })
        .or_else(|| {
            // Fallback to universal if exact arch not found
            release.assets.iter().find(|a| {
                let name = a.name.to_lowercase();
                (name.ends_with(".dmg") || name.ends_with(".tar.gz") || name.ends_with(".zip"))
                    && name.contains("universal")
            })
        });
    
    eprintln!("[check_update] Selected asset: {:?}", asset.map(|a| &a.name));

    Ok(UpdateInfo {
        available: is_newer,
        version: release.tag_name,
        notes: release.body,
        download_url: asset.map(|a| a.browser_download_url.clone()),
    })
}

#[command]
pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    // 1. Download
    let temp_dir = app.path().temp_dir().map_err(|e| e.to_string())?.join("r2modmac_update");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let filename = download_url.split('/').last().unwrap_or("update.bin");
    let file_path = temp_dir.join(filename);

    eprintln!("[install_update] Downloading to {:?}", file_path);
    
    // Stream download to calculate progress
    use std::io::Write;
    use futures_util::StreamExt;

    let response = reqwest::get(&download_url).await.map_err(|e| e.to_string())?;
    let total_size = response.content_length().unwrap_or(0);
    
    let mut file = fs::File::create(&file_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        
        downloaded += chunk.len() as u64;
        
        if total_size > 0 {
            let percent = (downloaded as f64 / total_size as f64 * 100.0) as u8;
            // Emit progress event
            let _ = app.emit("update-progress", percent);
        }
    }

    // 2. Prepare Update Script
    let script_path = temp_dir.join("update.sh");
    
    // GUARD: Check if we are in dev mode or not in a standard .app bundle
    // If current_exe is inside "target/debug" or "target/release", we are likely in dev/build.
    // Abort update to prevent deleting source code!
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_string = current_exe.to_string_lossy();
    if exe_string.contains("/target/debug/") || exe_string.contains("/target/release/") {
        eprintln!("Dev/Build environment detected. Skipping destructive update.");
        return Err("Cannot auto-update in development environment. Build a release bundle to test.".to_string());
    }

    // Determine extraction/mount commands based on file type
    let (extract_command, app_source) = if filename.ends_with(".tar.gz") {
        (
            format!("tar -xzf '{}' -C '{}'", file_path.to_string_lossy(), temp_dir.to_string_lossy()),
            format!("{}/r2modmac.app", temp_dir.to_string_lossy())
        )
    } else if filename.ends_with(".zip") {
        (
            format!("unzip -o '{}' -d '{}'", file_path.to_string_lossy(), temp_dir.to_string_lossy()),
            format!("{}/r2modmac.app", temp_dir.to_string_lossy())
        )
    } else if filename.ends_with(".dmg") {
        // DMG: mount readonly to private folder, copy app, unmount
        // "Extracting with style": hdiutil attach ...
        let mount_point = format!("{}/dmg_mount", temp_dir.to_string_lossy());
        (
            format!(
                "mkdir -p '{}' && hdiutil attach '{}' -mountpoint '{}' -nobrowse -quiet -readonly",
                mount_point, file_path.to_string_lossy(), mount_point
            ),
            format!("{}/r2modmac.app", mount_point)
        )
    } else {
        return Err("Unknown update format".to_string());
    };

    let is_dmg = filename.ends_with(".dmg");
    let mount_point = format!("{}/dmg_mount", temp_dir.to_string_lossy());

    // Navigate up from Contents/MacOS/executable to .app
    // Bundle path usually ends in .app. 
    // Guard ensured we are likely safe, but let's defaulting to /Applications just in case.
    let current_app_path = current_exe
        .parent().and_then(|p| p.parent()).and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or("/Applications/r2modmac.app".to_string());

    // Build script with conditional DMG unmount
    let unmount_command = if is_dmg {
        format!("hdiutil detach '{}' -force -quiet || true", mount_point)
    } else {
        String::new()
    };

    // The script waits for PID, mounts/extracts, deletes OLD app, moves NEW app, launches NEW app, cleans up.
    let script = format!(
r#"#!/bin/bash
PID={}
APP_PATH="{}"
UPDATE_DIR="{}"
APP_SOURCE="{}"

echo "Waiting for PID $PID to exit..."
while kill -0 $PID 2>/dev/null; do sleep 0.5; done

echo "Extracting/Mounting Update..."
{}

if [ ! -d "$APP_SOURCE" ]; then
    echo "Error: New app not found at $APP_SOURCE"
    exit 1
fi

echo "Replacing app at $APP_PATH..."
rm -rf "$APP_PATH"
cp -R "$APP_SOURCE" "$APP_PATH"

# Unmount if needed (DMG)
{}

echo "Launching new app..."
open "$APP_PATH"

echo "Cleaning up..."
rm -rf "$UPDATE_DIR"
"#,
        std::process::id(),
        current_app_path,
        temp_dir.to_string_lossy(),
        app_source,
        extract_command,
        unmount_command
    );

    fs::write(&script_path, script).map_err(|e| e.to_string())?;
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;

    // 3. Launch Script detached
    eprintln!("[install_update] Launching update script...");
    Command::new("sh")
        .arg(&script_path)
        .spawn()
        .map_err(|e| format!("Failed to launch script: {}", e))?;

    // 4. Exit App to allow script to proceed
    eprintln!("[install_update] Exiting app to allow update...");
    app.exit(0);

    Ok(())
}

pub fn compare_versions(v1: &str, v2: &str) -> bool {
    let v1_parts: Vec<u32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
    let v2_parts: Vec<u32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();
    
    for i in 0..std::cmp::max(v1_parts.len(), v2_parts.len()) {
        let p1 = v1_parts.get(i).unwrap_or(&0);
        let p2 = v2_parts.get(i).unwrap_or(&0);
        if p1 > p2 { return true; }
        if p1 < p2 { return false; }
    }
    false
}
