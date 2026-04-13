use crate::models::shared::*;
use base64::Engine;
use std::fs;
use tauri::{command, AppHandle, Manager};

#[command]
pub async fn export_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, String> {
    // 1. Read profiles.json
    let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profiles_path.exists() {
        return Err("No profiles found".to_string());
    }
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;

    let profile = profiles
        .iter()
        .find(|p| p["id"] == profile_id)
        .ok_or("Profile not found")?;

    // 2. Create export data
    let mods = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|m| {
            let full_name = m["fullName"].as_str().unwrap_or("");
            let version_number = m["versionNumber"].as_str().unwrap_or("0.0.0");
            let enabled = m["enabled"].as_bool().unwrap_or(true);

            // Clean name logic (strip version suffix)
            let clean_name = if full_name.ends_with(&format!("-{}", version_number)) {
                &full_name[0..full_name.len() - version_number.len() - 1]
            } else {
                full_name
            };

            let version_parts: Vec<&str> = version_number.split('.').collect();
            let major = version_parts.get(0).unwrap_or(&"0").parse().unwrap_or(0);
            let minor = version_parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
            let patch = version_parts.get(2).unwrap_or(&"0").parse().unwrap_or(0);

            serde_json::json!({
                "name": clean_name,
                "version": {
                    "major": major,
                    "minor": minor,
                    "patch": patch
                },
                "enabled": enabled
            })
        })
        .collect::<Vec<_>>();

    let mut export_data = serde_json::json!({
        "profileName": profile["name"],
        "mods": mods
    });

    if let Some(plat) = profile.get("platform") {
        export_data["platform"] = plat.clone();
    }

    // 3. Convert to YAML
    let yaml_content = serde_yaml::to_string(&export_data).map_err(|e| e.to_string())?;

    // 4. Create Zip
    let temp_dir = std::env::temp_dir().join(format!("r2modmac-export-{}", profile_id));
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let zip_path = temp_dir.join("profile.r2z");
    let file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);

    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("export.r2x", options)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    zip.write_all(yaml_content.as_bytes())
        .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;

    // 5. Save Dialog
    use tauri_plugin_dialog::DialogExt;
    let save_path = app
        .dialog()
        .file()
        .add_filter("r2modman Profile", &["r2z"])
        .set_file_name(&format!(
            "{}.r2z",
            profile["name"].as_str().unwrap_or("profile")
        ))
        .blocking_save_file();

    if let Some(path) = save_path {
        let path_str = path.to_string();
        fs::copy(&zip_path, &path_str).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "success": true, "path": path_str }))
    } else {
        Ok(serde_json::json!({ "success": false, "error": "Cancelled" }))
    }
}

#[command]
pub async fn share_profile(app: AppHandle, profile_id: String) -> Result<String, String> {
    // 1. Read profiles.json
    let profiles_path = app.path().app_data_dir().unwrap().join("profiles.json");
    if !profiles_path.exists() {
        return Err("No profiles found".to_string());
    }
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;

    let profile = profiles
        .iter()
        .find(|p| p["id"] == profile_id)
        .ok_or("Profile not found")?;

    // 2. Create export data (Same logic as export_profile)
    let mods = profile["mods"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|m| {
            let full_name = m["fullName"].as_str().unwrap_or("");
            let version_number = m["versionNumber"].as_str().unwrap_or("0.0.0");
            let enabled = m["enabled"].as_bool().unwrap_or(true);

            // Clean name logic (strip version suffix)
            let clean_name = if full_name.ends_with(&format!("-{}", version_number)) {
                &full_name[0..full_name.len() - version_number.len() - 1]
            } else {
                full_name
            };

            let version_parts: Vec<&str> = version_number.split('.').collect();
            let major = version_parts.get(0).unwrap_or(&"0").parse().unwrap_or(0);
            let minor = version_parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
            let patch = version_parts.get(2).unwrap_or(&"0").parse().unwrap_or(0);

            serde_json::json!({
                "name": clean_name,
                "version": {
                    "major": major,
                    "minor": minor,
                    "patch": patch
                },
                "enabled": enabled
            })
        })
        .collect::<Vec<_>>();

    let mut export_data = serde_json::json!({
        "profileName": profile["name"],
        "mods": mods
    });

    if let Some(plat) = profile.get("platform") {
        export_data["platform"] = plat.clone();
    }

    // 3. Convert to YAML
    let yaml_content = serde_yaml::to_string(&export_data).map_err(|e| e.to_string())?;

    // 4. Create Zip in Memory
    let mut zip_buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut zip_buffer);
        let mut zip = zip::ZipWriter::new(cursor);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file("export.r2x", options)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        zip.write_all(yaml_content.as_bytes())
            .map_err(|e| e.to_string())?;

        zip.finish().map_err(|e| e.to_string())?;
    }

    // 5. Base64 Encode and Prepend Header
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&zip_buffer);
    let payload = format!("#r2modman\n{}", base64_data);

    // 6. Upload to Thunderstore
    let client = reqwest::Client::new();
    let response = client
        .post("https://thunderstore.io/api/experimental/legacyprofile/create/")
        .header("Content-Type", "application/octet-stream")
        .body(payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Upload failed: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

    // 7. Return Key
    let key = json["key"]
        .as_str()
        .ok_or("Invalid response: missing key")?;
    Ok(key.to_string())
}

#[command]
pub async fn import_profile(_app: AppHandle, code: String) -> Result<serde_json::Value, String> {
    eprintln!("[import_profile] Starting import with code: {}", code);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Strategy 1: Profile Code
    let profile_url = format!(
        "https://thunderstore.io/api/experimental/legacyprofile/get/{}/",
        code
    );
    eprintln!(
        "[import_profile] Strategy 1: Trying profile code URL: {}",
        profile_url
    );

    let response = client.get(&profile_url).send().await;

    match response {
        Ok(res) => {
            eprintln!(
                "[import_profile] Strategy 1: Got response with status: {}",
                res.status()
            );
            if res.status().is_success() {
                let content = res.text().await.unwrap_or_default();
                eprintln!(
                    "[import_profile] Strategy 1: Content length: {}, starts with #r2modman: {}",
                    content.len(),
                    content.starts_with("#r2modman")
                );

                if content.starts_with("#r2modman") {
                    eprintln!("[import_profile] Strategy 1: Detected r2modman profile, decoding base64...");
                    let base64_data = content.trim_start_matches("#r2modman").trim();
                    let zip_data = base64::engine::general_purpose::STANDARD
                        .decode(base64_data)
                        .map_err(|e| {
                            eprintln!("[import_profile] Strategy 1: Base64 decode failed: {}", e);
                            e.to_string()
                        })?;
                    eprintln!(
                        "[import_profile] Strategy 1: Decoded {} bytes, creating zip archive...",
                        zip_data.len()
                    );
                    let cursor = std::io::Cursor::new(zip_data);
                    let archive = zip::ZipArchive::new(cursor).map_err(|e| {
                        eprintln!(
                            "[import_profile] Strategy 1: Zip archive creation failed: {}",
                            e
                        );
                        e.to_string()
                    })?;
                    eprintln!("[import_profile] Strategy 1: Processing zip archive...");
                    return process_zip_archive(archive);
                }
            }
        }
        Err(e) => {
            eprintln!("[import_profile] Strategy 1: Request failed: {}", e);
        }
    }

    // Strategy 2: Package UUID
    eprintln!("[import_profile] Strategy 2: Trying package UUID lookup");
    let resolve_url = format!(
        "https://thunderstore.io/api/experimental/namespace-by-id/{}/",
        code
    );
    eprintln!("[import_profile] Strategy 2: URL: {}", resolve_url);

    let response = client.get(&resolve_url).send().await.map_err(|e| {
        eprintln!("[import_profile] Strategy 2: Request failed: {}", e);
        e.to_string()
    })?;

    eprintln!(
        "[import_profile] Strategy 2: Got response with status: {}",
        response.status()
    );

    if !response.status().is_success() {
        return Err(format!(
            "Import failed: Code not found as Profile or Package UUID (status: {})",
            response.status()
        ));
    }

    let metadata: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    eprintln!("[import_profile] Strategy 2: Got metadata: {:?}", metadata);

    let namespace = metadata["namespace"]
        .as_str()
        .ok_or("Invalid metadata: missing namespace")?;
    let name = metadata["name"]
        .as_str()
        .ok_or("Invalid metadata: missing name")?;
    eprintln!(
        "[import_profile] Strategy 2: Namespace: {}, Name: {}",
        namespace, name
    );

    let package_url = format!(
        "https://thunderstore.io/api/v1/package/{}/{}/",
        namespace, name
    );
    eprintln!(
        "[import_profile] Strategy 2: Fetching package from: {}",
        package_url
    );

    let pkg_response = client
        .get(&package_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !pkg_response.status().is_success() {
        eprintln!(
            "[import_profile] Strategy 2: Package fetch failed with status: {}",
            pkg_response.status()
        );
        return Err(format!(
            "Package details not found (status: {})",
            pkg_response.status()
        ));
    }

    let pkg: serde_json::Value = pkg_response.json().await.map_err(|e| e.to_string())?;
    eprintln!("[import_profile] Strategy 2: Successfully got package");

    Ok(serde_json::json!({
        "type": "package",
        "package": pkg
    }))
}

#[command]
pub async fn import_profile_from_file(
    _app: AppHandle,
    path: String,
) -> Result<serde_json::Value, String> {
    eprintln!(
        "[import_profile_from_file] Starting import from file: {}",
        path
    );
    let bytes = fs::read(&path).map_err(|e| {
        eprintln!("[import_profile_from_file] Failed to read file: {}", e);
        e.to_string()
    })?;
    eprintln!("[import_profile_from_file] Read {} bytes", bytes.len());

    let cursor = std::io::Cursor::new(bytes);
    let archive = zip::ZipArchive::new(cursor).map_err(|e| {
        eprintln!(
            "[import_profile_from_file] Failed to create zip archive: {}",
            e
        );
        e.to_string()
    })?;

    eprintln!("[import_profile_from_file] Zip archive created, processing...");
    let result = process_zip_archive(archive)?;
    eprintln!("[import_profile_from_file] Result: {:?}", result);
    Ok(result)
}

fn process_zip_archive(
    mut archive: zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
) -> Result<serde_json::Value, String> {
    eprintln!(
        "[process_zip_archive] Processing zip with {} files",
        archive.len()
    );

    let mut content = String::new();
    let is_yaml;

    // Check if export.r2x exists first to avoid double borrow
    let has_r2x = archive.by_name("export.r2x").is_ok();
    eprintln!("[process_zip_archive] Has export.r2x: {}", has_r2x);

    if has_r2x {
        let mut file = archive.by_name("export.r2x").map_err(|e| e.to_string())?;
        use std::io::Read;
        file.read_to_string(&mut content)
            .map_err(|e| e.to_string())?;
        is_yaml = true;
        eprintln!(
            "[process_zip_archive] Read export.r2x ({} bytes)",
            content.len()
        );
    } else {
        let mut file = archive
            .by_name("manifest.json")
            .map_err(|_| "Invalid profile: missing export.r2x or manifest.json".to_string())?;
        use std::io::Read;
        file.read_to_string(&mut content)
            .map_err(|e| e.to_string())?;
        is_yaml = false;
        eprintln!(
            "[process_zip_archive] Read manifest.json ({} bytes)",
            content.len()
        );
    }

    eprintln!(
        "[process_zip_archive] Content preview: {}",
        &content[0..content.len().min(200)]
    );

    let parsed: serde_json::Value = if is_yaml {
        eprintln!("[process_zip_archive] Parsing as YAML");
        serde_yaml::from_str(&content).map_err(|e| {
            eprintln!("[process_zip_archive] YAML parse error: {}", e);
            e.to_string()
        })?
    } else {
        eprintln!("[process_zip_archive] Parsing as JSON");
        serde_json::from_str(&content).map_err(|e| {
            eprintln!("[process_zip_archive] JSON parse error: {}", e);
            e.to_string()
        })?
    };

    eprintln!("[process_zip_archive] Parsed data: {:?}", parsed);

    // Map to expected format
    let profile_name = parsed["profileName"].as_str().unwrap_or("Imported Profile");
    let platform_val = parsed["platform"].as_str();
    eprintln!(
        "[process_zip_archive] Profile name: {}, Platform: {:?}",
        profile_name, platform_val
    );

    let empty_vec = vec![];
    let mods_array = parsed["mods"].as_array().unwrap_or(&empty_vec);
    eprintln!("[process_zip_archive] Found {} mods", mods_array.len());

    let mods = mods_array
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let name = m["name"].as_str().unwrap_or("");

            // Handle version: could be string "1.0.0" or object {major, minor, patch}
            let version_str = if let Some(v_obj) = m["version"].as_object() {
                format!(
                    "{}.{}.{}",
                    v_obj.get("major").and_then(|v| v.as_u64()).unwrap_or(0),
                    v_obj.get("minor").and_then(|v| v.as_u64()).unwrap_or(0),
                    v_obj.get("patch").and_then(|v| v.as_u64()).unwrap_or(0)
                )
            } else {
                m["version"].as_str().unwrap_or("0.0.0").to_string()
            };

            let clean_name = clean_mod_name(name, &version_str);
            let enabled = m["enabled"].as_bool().unwrap_or(true);

            eprintln!(
                "[process_zip_archive] Mod {}: {} -> {} (v{}), enabled: {}",
                idx, name, clean_name, version_str, enabled
            );

            serde_json::json!({
                "name": clean_name,
                "version": version_str,
                "enabled": enabled
            })
        })
        .collect::<Vec<_>>();

    let mut result = serde_json::json!({
        "type": "profile",
        "name": profile_name,
        "mods": mods
    });

    if let Some(p) = platform_val {
        result["platform"] = serde_json::json!(p);
    }

    eprintln!("[process_zip_archive] Final result: {:?}", result);
    Ok(result)
}
