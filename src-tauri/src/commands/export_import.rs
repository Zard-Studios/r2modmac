use crate::models::shared::*;
use base64::Engine;
use std::fs;
use std::io::{Seek, Write};
use tauri::{command, AppHandle};

fn profile_has_local_mods(profile: &serde_json::Value) -> bool {
    profile["mods"]
        .as_array()
        .map(|mods| mods.iter().any(|m| m["source"].as_str() == Some("local")))
        .unwrap_or(false)
}

fn build_export_mods(profile: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(mods) = profile["mods"].as_array() else {
        return Vec::new();
    };

    mods.iter()
        .map(|m| {
            let full_name = m["fullName"].as_str().unwrap_or("");
            let version_number = m["versionNumber"].as_str().unwrap_or("0.0.0");
            let enabled = m["enabled"].as_bool().unwrap_or(true);

            let clean_name = if full_name.ends_with(&format!("-{}", version_number)) {
                &full_name[0..full_name.len() - version_number.len() - 1]
            } else {
                full_name
            };

            let version_parts: Vec<&str> = version_number.split('.').collect();
            let major = version_parts.get(0).unwrap_or(&"0").parse().unwrap_or(0);
            let minor = version_parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
            let patch = version_parts.get(2).unwrap_or(&"0").parse().unwrap_or(0);

            let mut value = serde_json::json!({
                "name": clean_name,
                "version": {
                    "major": major,
                    "minor": minor,
                    "patch": patch
                },
                "enabled": enabled
            });

            if m["source"].as_str() == Some("local") {
                if let Some(local_id) = m["localId"].as_str() {
                    value["source"] = serde_json::json!("local");
                    value["localId"] = serde_json::json!(local_id);
                    value["payload"] =
                        serde_json::json!(format!("r2modmac/local_mods/{}/payload.zip", local_id));
                    value["displayName"] = m["displayName"].clone();
                    value["author"] = m["author"].clone();
                    value["description"] = m["description"].clone();
                    value["readme"] = m["readme"].clone();
                    value["iconUrl"] = m["iconUrl"].clone();
                    value["fileName"] = m["fileName"].clone();
                    value["fileSize"] = m["fileSize"].clone();
                    value["sha256"] = m["sha256"].clone();
                    value["manifestSha256"] = m["manifestSha256"].clone();
                    value["contentFingerprint"] = m["contentFingerprint"].clone();
                    value["platforms"] = m["platforms"].clone();
                    value["securityReport"] = m["securityReport"].clone();
                }
            }

            value
        })
        .collect::<Vec<_>>()
}

fn write_embedded_local_payloads<W: Write + std::io::Seek>(
    app: &AppHandle,
    profile_id: &str,
    profile: &serde_json::Value,
    zip: &mut zip::ZipWriter<W>,
    options: zip::write::FileOptions,
) -> Result<(), String> {
    let profile_dir = crate::utils::paths::app_data_dir(app)
        .map_err(|e| e.to_string())?
        .join("profiles")
        .join(profile_id);

    let Some(mods) = profile["mods"].as_array() else {
        return Ok(());
    };

    for m in mods {
        if m["source"].as_str() != Some("local") {
            continue;
        }
        let local_id = m["localId"]
            .as_str()
            .ok_or("Local mod is missing localId")?;
        let payload_path = profile_dir
            .join("local_mods")
            .join(local_id)
            .join("payload.zip");
        if !payload_path.exists() {
            return Err(format!(
                "Local mod payload is missing for {}",
                m["fullName"].as_str().unwrap_or(local_id)
            ));
        }
        let archive_path = format!("r2modmac/local_mods/{}/payload.zip", local_id);
        zip.start_file(archive_path, options)
            .map_err(|e| e.to_string())?;
        let mut payload_file = fs::File::open(payload_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut payload_file, zip).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn build_export_data(profile: &serde_json::Value) -> serde_json::Value {
    let mods = build_export_mods(profile);
    let mut export_data = serde_json::json!({
        "profileName": profile["name"],
        "mods": mods
    });

    if let Some(plat) = profile.get("platform") {
        export_data["platform"] = plat.clone();
    }

    export_data
}

fn write_profile_zip<W: Write + Seek>(
    app: &AppHandle,
    profile_id: &str,
    profile: &serde_json::Value,
    writer: W,
    include_local_payloads: bool,
) -> Result<W, String> {
    let export_data = build_export_data(profile);
    let yaml_content = serde_yaml::to_string(&export_data).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(writer);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("export.r2x", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(yaml_content.as_bytes())
        .map_err(|e| e.to_string())?;

    if include_local_payloads {
        write_embedded_local_payloads(app, profile_id, profile, &mut zip, options)?;
    }

    zip.finish().map_err(|e| e.to_string())
}

fn build_profile_zip_bytes(
    app: &AppHandle,
    profile_id: &str,
    profile: &serde_json::Value,
    include_local_payloads: bool,
) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(Vec::new());
    let cursor = write_profile_zip(app, profile_id, profile, cursor, include_local_payloads)?;
    Ok(cursor.into_inner())
}

fn find_profile(app: &AppHandle, profile_id: &str) -> Result<serde_json::Value, String> {
    let profiles_path = crate::utils::paths::app_data_dir(app).unwrap().join("profiles.json");
    if !profiles_path.exists() {
        return Err("No profiles found".to_string());
    }
    let profiles_data = fs::read_to_string(&profiles_path).map_err(|e| e.to_string())?;
    let profiles: Vec<serde_json::Value> =
        serde_json::from_str(&profiles_data).map_err(|e| e.to_string())?;

    profiles
        .into_iter()
        .find(|p| p["id"] == profile_id)
        .ok_or("Profile not found".to_string())
}

#[command]
pub async fn export_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, String> {
    let profile = find_profile(&app, &profile_id)?;

    let temp_dir = std::env::temp_dir().join(format!("r2modmac-export-{}", profile_id));
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let zip_path = temp_dir.join("profile.r2z");
    let file = fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    write_profile_zip(&app, &profile_id, &profile, file, true)?;

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
    let profile = find_profile(&app, &profile_id)?;

    // Outer Wilds (and any game using a non-Thunderstore mod source) cannot be shared
    // via Thunderstore codes: the mods are not in the Thunderstore registry, so the
    // generated code would be unresolvable on import. Use Export as File instead.
    let game_identifier = profile["gameIdentifier"].as_str().unwrap_or("").to_lowercase();
    if crate::models::shared::is_outerwilds_identifier(&game_identifier) {
        return Err("Outer Wilds profiles cannot be shared via code — the mods are not on Thunderstore. Use Export as File instead.".to_string());
    }

    if profile_has_local_mods(&profile) {
        return Err("This profile contains custom local mods. Thunderstore share codes cannot carry local files; use Export as File instead.".to_string());
    }


    let zip_buffer = build_profile_zip_bytes(&app, &profile_id, &profile, false)?;

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
    eprintln!("[import_profile] Starting import with provided code");
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
        "https://thunderstore.io/api/experimental/package/{}/{}/",
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

    let mut pkg: serde_json::Value = pkg_response.json().await.map_err(|e| e.to_string())?;
    eprintln!("[import_profile] Strategy 2: Successfully got package");

    if pkg["uuid4"].is_null() {
        pkg["uuid4"] = serde_json::json!("");
    }

    if pkg["versions"].is_null() || !pkg["versions"].is_array() {
        let mut latest_ver = pkg["latest"].clone();
        if !latest_ver.is_null() {
            if latest_ver["uuid4"].is_null() {
                latest_ver["uuid4"] = serde_json::json!("");
            }
            pkg["versions"] = serde_json::json!(vec![latest_ver]);
        } else {
            pkg["versions"] = serde_json::json!(Vec::<serde_json::Value>::new());
        }
    }

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
    let mut result = process_zip_archive(archive)?;
    result["archivePath"] = serde_json::json!(path);
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

    let mods_array = parsed["mods"]
        .as_array()
        .ok_or_else(|| "Invalid profile: missing mods array".to_string())?;
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
            let source = m["source"].as_str().unwrap_or("thunderstore");

            eprintln!(
                "[process_zip_archive] Mod {}: {} -> {} (v{}), enabled: {}",
                idx, name, clean_name, version_str, enabled
            );

            let mut value = serde_json::json!({
                "name": clean_name,
                "version": version_str,
                "enabled": enabled
            });

            if source == "local" {
                value["source"] = serde_json::json!("local");
                value["localId"] = m["localId"].clone();
                value["payload"] = m["payload"].clone();
                value["displayName"] = m["displayName"].clone();
                value["author"] = m["author"].clone();
                value["description"] = m["description"].clone();
                value["readme"] = m["readme"].clone();
                value["iconUrl"] = m["iconUrl"].clone();
                value["fileName"] = m["fileName"].clone();
                value["fileSize"] = m["fileSize"].clone();
                value["sha256"] = m["sha256"].clone();
                value["manifestSha256"] = m["manifestSha256"].clone();
                value["contentFingerprint"] = m["contentFingerprint"].clone();
                value["platforms"] = m["platforms"].clone();
                value["securityReport"] = m["securityReport"].clone();
            }

            value
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_archive_with_file(
        name: &str,
        content: &str,
    ) -> zip::ZipArchive<std::io::Cursor<Vec<u8>>> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::FileOptions::default();
        writer.start_file(name, options).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
        let mut cursor = writer.finish().unwrap();
        cursor.set_position(0);
        zip::ZipArchive::new(cursor).unwrap()
    }

    #[test]
    fn process_zip_archive_rejects_mod_manifest_without_profile_mods() {
        let archive = zip_archive_with_file(
            "manifest.json",
            r#"{"name":"UltraOptimizer","version_number":"0.4.0","website_url":""}"#,
        );

        let err = process_zip_archive(archive).unwrap_err();

        assert!(err.contains("missing mods array"));
    }

    #[test]
    fn process_zip_archive_accepts_profile_manifest_with_mods() {
        let archive = zip_archive_with_file(
            "manifest.json",
            r#"{"profileName":"test","mods":[{"name":"Author-Mod","version":"1.2.3","enabled":true}]}"#,
        );

        let result = process_zip_archive(archive).unwrap();

        assert_eq!(result["type"], "profile");
        assert_eq!(result["name"], "test");
        assert_eq!(result["mods"][0]["name"], "Author-Mod");
        assert_eq!(result["mods"][0]["version"], "1.2.3");
    }
}
