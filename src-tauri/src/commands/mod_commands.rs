use std::fs;
use futures_util::StreamExt;
use tauri::{command, AppHandle, Emitter, Manager};
use crate::models::shared::*;
use crate::utils::file_ops::*;
use crate::utils::mod_manifest::{
    backup_existing_mod_files,
    save_owned_mod_manifest,
    GAME_MANIFEST_SCOPE,
};
use crate::commands::game_commands::get_game_path;

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
        if keys.iter().any(|needle| key.trim().eq_ignore_ascii_case(needle)) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

pub(crate) fn normalize_macos_doorstop_config_file(
    path: &std::path::Path,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let dll_search_path_override =
        extract_ini_value(&content, &["dllSearchPathOverride", "dll_search_path_override"])
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

fn profile_is_vanilla(app: &AppHandle, profile_id: &str) -> bool {
    let profiles_path = match app.path().app_data_dir() {
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
            .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
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
        .user_agent("r2modmac/0.6.0")
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
                    asset["browser_download_url"].as_str().map(|s| s.to_string())
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
            candidates.push(format!("{}.{}.{}.{}", major, minor, patch_major, patch_minor));
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

async fn download_official_macos_bepinex_pack(thunderstore_version: &str) -> Result<Vec<u8>, String> {
    let version_candidates = official_bepinex_version_candidates(thunderstore_version);
    let client = reqwest::Client::builder()
        .user_agent("r2modmac/0.6.0")
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
    let build_number = thunderstore_version
        .split('.')
        .nth(2)
        .ok_or_else(|| format!("Could not parse BepInEx 6 build number from {}", thunderstore_version))?;
    let client = reqwest::Client::builder()
        .user_agent("r2modmac/0.6.0")
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
        &[
            "bepinex-unity.mono-macos-x64",
            "bepinex_unitymono_unix",
        ]
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
            let mut archive = zip::ZipArchive::new(cursor)
                .map_err(|e| format!("Downloaded official BepInEx 6 runtime is not a zip: {}", e))?;
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

        let Some(normalized_relative_path) = normalize_bepinex_pack_entry(relative_path, target_is_macos) else {
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

fn normalize_macos_bepinex_runtime_overlay_entry(relative_path: &str) -> Option<std::path::PathBuf> {
    let normalized = normalize_bepinex_pack_entry(relative_path, true)?;
    let lower = normalized.to_string_lossy().replace('\\', "/").to_lowercase();

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

        if normalized_relative_path == std::path::PathBuf::from("doorstop_config.ini") && outpath.exists() {
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

fn normalize_bepinex_pack_entry(relative_path: &str, target_is_macos: bool) -> Option<std::path::PathBuf> {
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

    if matches!(lower_trimmed.as_str(), "doorstop_config.ini" | "libdoorstop.dylib" | "winhttp.dll") {
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

fn find_mod_entry_recursive(base: &std::path::Path, mod_name: &str, depth: usize) -> Option<std::path::PathBuf> {
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
    fs::read_dir(path)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|entry| {
                entry.file_name().to_string_lossy().ends_with(".app")
            })
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
pub async fn install_mod(app: AppHandle, profile_id: String, download_url: String, mod_name: String, game_path: String, use_profile_cache: Option<bool>) -> Result<serde_json::Value, String> {
    // Install DIRECTLY to game folder
    let game_dir = std::path::Path::new(&game_path);
    let target_is_macos = is_macos_game_dir(game_dir);
    let target_is_balatro = target_is_macos && is_balatro_game_path(game_dir);
    let install_into_disabled_runtime =
        target_is_macos && !target_is_balatro && profile_is_vanilla(&app, &profile_id);

    eprintln!("[install_mod] Installing {} directly to game: {:?}", mod_name, game_dir);

    // Download with live progress events (bytes + speed)
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
    let mut stream = response.bytes_stream();
    let mut bytes: Vec<u8> = Vec::new();
    let mut downloaded: u64 = 0;
    let download_started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Some(next_chunk) = stream.next().await {
        let chunk = next_chunk.map_err(|e| e.to_string())?;
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
    
    let mut runtime_bytes = bytes;

    if target_is_balatro {
        if is_balatro_lovely_mod(&mod_name) {
            let version_number = extract_version_number_from_full_name(&mod_name)
                .ok_or_else(|| format!("Could not parse Lovely version from {}", mod_name))?;
            let cursor = std::io::Cursor::new(&runtime_bytes);
            let mut archive_for_detect = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
            let (has_macos_runtime, _has_windows_runtime) = detect_lovely_runtime_in_zip(&mut archive_for_detect);

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
                let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
                    .join("profiles").join(&profile_id);
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

        let mods_root = get_balatro_mods_dir()
            .ok_or_else(|| "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string())?;
        fs::create_dir_all(&mods_root).map_err(|e| e.to_string())?;

        let target_folder = balatro_target_folder_name(&mod_name);
        let mod_dir = mods_root.join(&target_folder);
        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
        extract_zip_directory_to_target(&mut archive, &mod_dir)?;

        if use_profile_cache.unwrap_or(false) {
            let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
                .join("profiles").join(&profile_id);
            let profile_mod_dir = profile_dir.join("Balatro").join("Mods").join(&target_folder);
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
    let (mut has_macos_loader, mut has_windows_loader) = detect_bepinex_pack_platform(&mut archive_for_detect);
    let mut macos_runtime_overlay_bytes: Option<Vec<u8>> = None;

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
            let (fallback_is_bepinex_pack, _) =
                detect_bepinex_structure(&mut fallback_archive);
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
            files.extend(collect_macos_bepinex_runtime_overlay_files(&mut overlay_archive)?);
        }

        files.sort();
        files.dedup();
        files
            .into_iter()
            .map(|file| {
                remap_disabled_macos_runtime_path(&file, install_into_disabled_runtime)
            })
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
        let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
            .join("profiles").join(&profile_id);
        eprintln!("[install_mod] LEGACY: Also caching to profile: {:?}", profile_dir);

        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

        if is_bepinex_pack {
            // Cache BepInExPack to profile root
            extract_bepinex_pack_to_root(&mut archive, &profile_dir, target_is_macos, false)?;

            if let Some(overlay_bytes) = macos_runtime_overlay_bytes.as_ref() {
                let cursor = std::io::Cursor::new(overlay_bytes);
                let mut overlay_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
                extract_macos_bepinex_runtime_overlay_to_root(
                    &mut overlay_archive,
                    &profile_dir,
                    false,
                )?;
            }
        } else {
            eprintln!("[install_mod] Updating profile cache root for {:?}", profile_dir);
            extract_regular_mod_to_root(&mut archive, &profile_dir, &mod_name, false)?;
            if target_is_macos {
                migrate_root_plugins_into_bepinex(&profile_dir)?;
            }
        }

        if target_is_macos {
            dequarantine_recursive(game_dir);
        }
    }

    eprintln!("[install_mod] Successfully installed {} to game folder", mod_name);
    Ok(serde_json::json!({ "success": true }))
}

#[command]
pub async fn remove_mod(app: AppHandle, profile_id: String, mod_name: String) -> Result<bool, String> {
    let profile_dir = app.path().app_data_dir().unwrap().join("profiles").join(&profile_id);
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
    let game_path = get_game_path(app.clone(), game_identifier, platform)
        .await?
        .ok_or_else(|| "GAME_PATH_NOT_CONFIGURED".to_string())?;
    let game_root = std::path::Path::new(&game_path);
    let mod_key = extract_mod_key(&mod_name);

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
                open::that(game_root).map_err(|e| format!("Failed to open Balatro root folder: {}", e))?;
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
                found_path.parent().map(|p| p.to_path_buf()).unwrap_or(found_path)
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
            open::that(&bepinex_root).map_err(|e| format!("Failed to open BepInEx folder: {}", e))?;
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
            found_path.parent().map(|p| p.to_path_buf()).unwrap_or(found_path)
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
            open::that(&patchers_dir).map_err(|e| format!("Failed to open patchers folder: {}", e))?;
            return Ok(());
        }
    }

    Err("MOD_NOT_INSTALLED".to_string())
}

#[command]
pub async fn toggle_mod(app: AppHandle, profile_id: String, mod_name: String, enabled: bool, game_identifier: Option<String>, platform: Option<String>) -> Result<(), String> {
    eprintln!("[toggle_mod] Toggle mod: {} enabled: {} in profile: {}", mod_name, enabled, profile_id);
    
    let use_disabled_runtime = profile_is_vanilla(&app, &profile_id)
        && platform.as_deref().map(|value| value.eq_ignore_ascii_case("mac")).unwrap_or(false);

    // Get game path for sync (optional - toggle still works without it)
    let game_plugins = if let Some(ref game_id) = game_identifier {
        if let Ok(Some(game_path_str)) = get_game_path(app.clone(), game_id.clone(), platform.clone()).await {
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
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
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
                    eprintln!("[toggle_mod] Enabling mod - copying from cache to game: {}", folder_name);
                    copy_dir_recursive(&profile_mod_path, &game_mod_path)
                        .map_err(|e| format!("Failed to sync mod to game: {}", e))?;
                }
            } else {
                // Remove mod from game folder (keep in cache)
                if game_mod_path.exists() || game_mod_path.is_symlink() {
                    eprintln!("[toggle_mod] Disabling mod - removing from game: {}", folder_name);
                    if game_mod_path.is_symlink() || game_mod_path.is_file() {
                        fs::remove_file(&game_mod_path)
                            .map_err(|e| format!("Failed to remove mod symlink/file from game: {}", e))?;
                    } else {
                        fs::remove_dir_all(&game_mod_path)
                            .map_err(|e| format!("Failed to remove mod directory from game: {}", e))?;
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
pub async fn copy_mod_from_cache(app: AppHandle, profile_id: String, mod_name: String, game_path: String) -> Result<serde_json::Value, String> {
    let profile_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("profiles").join(&profile_id);
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
            let game_mods_dir = get_balatro_mods_dir()
                .ok_or_else(|| "Could not resolve ~/Library/Application Support/Balatro/Mods".to_string())?;
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

        eprintln!("[copy_mod_from_cache] Balatro mod {} not found in profile cache", mod_name);
        return Ok(serde_json::json!({ "success": false, "copied": false }));
    }

    if is_macos_game_dir(game_dir) && mod_name_lower.contains("bepinexpack") {
        let profile_has_runtime = profile_dir.join("BepInEx").join("core").is_dir()
            && profile_dir.join("doorstop_libs").is_dir()
            && profile_dir.join("run_bepinex.sh").exists();

        if profile_has_runtime {
            let root_dirs = [
                ("BepInEx", runtime_bepinex_dir_name(use_disabled_runtime)),
                ("doorstop_libs", if use_disabled_runtime { "doorstop_libs_DISABLED" } else { "doorstop_libs" }),
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
                ("doorstop_config.ini", if use_disabled_runtime { "doorstop_config.ini_DISABLED" } else { "doorstop_config.ini" }),
                ("libdoorstop.dylib", if use_disabled_runtime { "libdoorstop.dylib_DISABLED" } else { "libdoorstop.dylib" }),
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
        let runtime_bytes = download_official_macos_bepinex_runtime(&version_number, runtime_kind).await?;

        let cursor = std::io::Cursor::new(&runtime_bytes);
        let mut game_archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
        extract_bepinex_pack_to_root(
            &mut game_archive,
            game_dir,
            true,
            use_disabled_runtime,
        )?;

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
            
            if folder_name.to_lowercase().contains(&mod_name_lower) || 
               mod_name_lower.contains(&folder_name.to_lowercase()) {
                let src_path = entry.path();
                let dst_path = game_plugins_dir.join(&folder_name);
                
                if src_path.is_dir() {
                    eprintln!("[copy_mod_from_cache] Copying {} from cache to game", folder_name);
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
    eprintln!("[copy_mod_from_cache] Mod {} not found in profile cache", mod_name);
    Ok(serde_json::json!({ "success": false, "copied": false }))
}

#[command]
pub async fn fetch_packages(app: AppHandle, state: tauri::State<'_, AppState>, game_id: String) -> Result<usize, String> {
    use std::time::SystemTime;
    use std::time::Duration;

    let start_time = SystemTime::now();
    
    // 0. Check if we already have packages in memory (instant return)
    {
        let packages_lock = state.packages.read().await;
        if let Some(packages) = packages_lock.get(&game_id) {
            if !packages.is_empty() {
                eprintln!("[fetch_packages] Serving {} packages from memory (instant)", packages.len());
                return Ok(packages.len());
            }
        }
    }
    
    // 1. Fetch the index (list of chunk URLs)
    let index_url = format!("https://thunderstore.io/c/{}/api/v1/package-listing-index/", game_id);
    eprintln!("[fetch_packages] Fetching index from: {}", index_url);
    
    let client = reqwest::Client::builder()
        .user_agent("r2modmac/0.6.0")
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(25))
        .gzip(true)
        .build()
        .map_err(|e| e.to_string())?;

    fn decode_gzip_or_plain(bytes: &[u8], label: &str) -> Result<String, String> {
        // Thunderstore blobs are usually raw gzip bytes, but support plain JSON fallback.
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

    let resp = client
        .get(&index_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch index: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Index request failed with status {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let index_json = decode_gzip_or_plain(&bytes, "index")?;
    let chunk_urls: Vec<String> = parse_index_chunk_urls(&index_json)?;
    let total_chunks = chunk_urls.len();
    eprintln!("[fetch_packages] Found {} chunks", total_chunks);
    if total_chunks == 0 {
        return Ok(0);
    }

    // 2. Load chunks directly from the network and keep them in memory only.
    // This avoids creating large per-game cache files on disk.
    async fn load_chunk(client: &reqwest::Client, url: &str) -> Result<Vec<serde_json::Value>, String> {
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
                last_error = Some(format!("Attempt {} failed with status {}", attempt, resp.status()));
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
                        last_error = Some(format!("Attempt {} failed decompressing gzip: {}", attempt, e));
                        tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                        continue;
                    }
                }
            } else {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(out) => out,
                    Err(e) => {
                        last_error = Some(format!("Attempt {} invalid utf-8 chunk: {}", attempt, e));
                        tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                        continue;
                    }
                }
            };

            let mut packages: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
                Ok(packages) => packages,
                Err(e) => {
                    last_error = Some(format!("Attempt {} failed parsing JSON chunk: {}", attempt, e));
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                    continue;
                }
            };
        
            // Filter out Manager packages (e.g. r2modman, Thunderstore Mod Manager) if they appear
            packages.retain(|pkg| {
                let full_name = pkg["full_name"].as_str().unwrap_or("");
                !full_name.contains("ebkr-r2modman")
                    && !full_name.contains("Tslat-ThunderstoreModManager")
                    && !full_name.contains("Kesomannen-GaleModManager")
            });

            return Ok(packages);
        }

        Err(last_error.unwrap_or_else(|| "Failed to load chunk".to_string()))
    }

    // 3. Load FIRST successful chunk immediately for instant UI
    // If chunk #0 is slow/unavailable, try next few chunks to avoid infinite spinner.
    let mut first_loaded_index: Option<usize> = None;
    let first_attempts = std::cmp::min(5, chunk_urls.len());
    for idx in 0..first_attempts {
        let first_url = &chunk_urls[idx];
        match load_chunk(&client, first_url).await {
            Ok(first_packages) => {
                let count = first_packages.len();
                eprintln!("[fetch_packages] First chunk loaded (index {}): {} packages", idx, count);
                
                // Update state immediately so UI can show something
                let mut packages_lock = state.packages.write().await;
                packages_lock.insert(game_id.clone(), first_packages);
                first_loaded_index = Some(idx);
                break;
            }
            Err(e) => {
                eprintln!("[fetch_packages] Failed first chunk attempt idx {}: {}", idx, e);
            }
        }
    }

    if first_loaded_index.is_none() {
        return Err("Failed to load initial package chunks (network timeout or CDN issue)".to_string());
    }

    // 4. Load remaining chunks in parallel (streaming to state)
    let remaining_urls: Vec<String> = chunk_urls
        .into_iter()
        .enumerate()
        .filter_map(|(idx, url)| if Some(idx) == first_loaded_index { None } else { Some(url) })
        .collect();
    
    if !remaining_urls.is_empty() {
        let packages_arc = state.packages.clone();
        let game_id_clone = game_id.clone();
        let app_handle = app.clone();
        
        // Spawn background task for remaining chunks
        tokio::spawn(async move {
            let parallelism = 10usize;
            let mut stream = futures_util::stream::iter(remaining_urls)
                .map(|url| {
                    let client = client.clone();
                    async move { load_chunk(&client, &url).await }
                })
                .buffer_unordered(parallelism);

            // Collect and add to state as they complete
            while let Some(result) = stream.next().await {
                match result {
                    Ok(packages) => {
                        let mut packages_lock = packages_arc.write().await;
                        if let Some(existing) = packages_lock.get_mut(&game_id_clone) {
                            existing.extend(packages);
                        }
                    }
                    Err(e) => eprintln!("[fetch_packages] Chunk error: {}", e),
                }
            }
            
            // Get final count and emit event for frontend
            let final_count = {
                let packages_lock = packages_arc.read().await;
                packages_lock.get(&game_id_clone).map(|p| p.len()).unwrap_or(0)
            };
            
            eprintln!("[fetch_packages] Background loading complete. Total: {} packages", final_count);
            
            // Emit event so frontend knows more packages are available
            let _ = app_handle.emit("packages-loaded", serde_json::json!({
                "game_id": game_id_clone,
                "total_count": final_count
            }));
        });
    }

    // 5. Return immediately with first chunk count
    let packages_lock = state.packages.read().await;
    let count = packages_lock.get(&game_id).map(|p| p.len()).unwrap_or(0);
    
    if let Ok(elapsed) = start_time.elapsed() {
        eprintln!("[fetch_packages] Initial load in {:.2?} ({} packages ready, {} chunks loading in background)", 
            elapsed, count, total_chunks - 1);
    }

    Ok(count)
}

#[command]
pub async fn get_available_categories(
    state: tauri::State<'_, AppState>,
    game_id: String
) -> Result<Vec<String>, String> {
    let packages_lock = state.packages.read().await;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        let mut categories: std::collections::HashSet<String> = std::collections::HashSet::new();
        
        for p in packages.iter() {
            if let Some(cats) = p.get("categories").and_then(|v| v.as_array()) {
                for cat in cats {
                    if let Some(cat_str) = cat.as_str() {
                        categories.insert(cat_str.to_string());
                    }
                }
            }
        }
        
        let mut result: Vec<String> = categories.into_iter().collect();
        result.sort();
        Ok(result)
    } else {
        Ok(vec![])
    }
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
    modpacks: Option<bool>
) -> Result<Vec<serde_json::Value>, String> {
    let packages_lock = state.packages.read().await;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        // Initial filtering
        let mut filtered: Vec<&serde_json::Value> = packages.iter().filter(|p| {
            // 1. Search Filter
            if !search.is_empty() {
                let search_lower = search.to_lowercase();
                let name = p["name"].as_str().unwrap_or("").to_lowercase();
                let full_name = p["full_name"].as_str().unwrap_or("").to_lowercase();
                if !name.contains(&search_lower) && !full_name.contains(&search_lower) {
                    return false;
                }
            }

            // 2. NSFW Filter
            // If nsfw tag is FALSE (default): Hide NSFW content
            // If nsfw tag is TRUE: Show ONLY NSFW content
            let nsfw_tag_active = nsfw.unwrap_or(false);
            let is_nsfw = p.get("has_nsfw_content").and_then(|v| v.as_bool()).unwrap_or(false);
            if nsfw_tag_active {
                // Show ONLY NSFW
                if !is_nsfw { return false; }
            } else {
                // Hide NSFW
                if is_nsfw { return false; }
            }

            // 3. Deprecated Filter
            // If deprecated tag is FALSE (default): Hide Deprecated content
            // If deprecated tag is TRUE: Show ONLY Deprecated content
            let deprecated_tag_active = deprecated.unwrap_or(false);
            let is_deprecated = p.get("is_deprecated").and_then(|v| v.as_bool()).unwrap_or(false);
            if deprecated_tag_active {
                // Show ONLY Deprecated
                if !is_deprecated { return false; }
            } else {
                // Hide Deprecated
                if is_deprecated { return false; }
            }

            // 4. Mods/Modpacks Filter
            // Logic: Both OFF = show all, Both ON = show all, Only one ON = show only that type
            let mods_active = mods.unwrap_or(false);
            let modpacks_active = modpacks.unwrap_or(false);
            
            // Check if package is a modpack (has "Modpacks" in categories)
            let pkg_categories: Vec<String> = p.get("categories")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|c| c.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let is_modpack = pkg_categories.iter().any(|c| c.to_lowercase() == "modpacks");
            
            // Apply filter only if exactly one is active
            if mods_active != modpacks_active {
                if mods_active && is_modpack {
                    return false; // Show only mods, this is a modpack - hide it
                }
                if modpacks_active && !is_modpack {
                    return false; // Show only modpacks, this is a mod - hide it
                }
            }

            // 5. Category/Tag Filter
            // If categories is empty or None, show all
            // If categories has values, package must match at least one:
            // - Match in categories array
            // - OR match in package name
            // - OR match in package description
            if let Some(ref filter_cats) = categories {
                if !filter_cats.is_empty() {
                    let pkg_categories: Vec<String> = p.get("categories")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|c| c.as_str().map(|s| s.to_lowercase())).collect())
                        .unwrap_or_default();
                    
                    let pkg_name = p.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let pkg_full_name = p.get("full_name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    
                    // Check if any filter tag matches
                    let has_match = filter_cats.iter().any(|fc| {
                        let fc_lower = fc.to_lowercase();
                        // Match in categories
                        pkg_categories.iter().any(|c| c.contains(&fc_lower)) ||
                        // Match in name
                        pkg_name.contains(&fc_lower) ||
                        pkg_full_name.contains(&fc_lower)
                    });
                    
                    if !has_match {
                        return false;
                    }
                }
            }

            true
        }).collect();

        // Sorting
        if let Some(sort_by) = sort {
            let direction = sort_direction.unwrap_or("desc".to_string());
            let is_asc = direction == "asc";

            match sort_by.as_str() {
                "downloads" => filtered.sort_by(|a, b| {
                    let get_downloads = |p: &serde_json::Value| -> u64 {
                        p.get("versions")
                         .and_then(|v| v.as_array())
                         .and_then(|arr| arr.first()) 
                         .and_then(|ver| ver.get("downloads"))
                         .and_then(|d| d.as_u64())
                         .unwrap_or(0)
                    };
                    let da = get_downloads(a);
                    let db = get_downloads(b);
                    if is_asc { da.cmp(&db) } else { db.cmp(&da) }
                }),
                "rating" => filtered.sort_by(|a, b| {
                    let ra = a.get("rating_score").and_then(|v| v.as_i64()).unwrap_or(0);
                    let rb = b.get("rating_score").and_then(|v| v.as_i64()).unwrap_or(0);
                    if is_asc { ra.cmp(&rb) } else { rb.cmp(&ra) }
                }),
                "updated" => filtered.sort_by(|a, b| {
                    let da = a.get("date_updated").and_then(|v| v.as_str()).unwrap_or("");
                    let db = b.get("date_updated").and_then(|v| v.as_str()).unwrap_or("");
                    if is_asc { da.cmp(db) } else { db.cmp(da) }
                }),
                "alphabetical" => filtered.sort_by(|a, b| {
                    let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    // Ascending: A-Z (na vs nb)
                    // Descending: Z-A (nb vs na)
                    if is_asc { na.cmp(&nb) } else { nb.cmp(&na) }
                }),
                _ => {}
            }
        }

        let start = page * page_size;
        if start >= filtered.len() {
            return Ok(vec![]);
        }
        
        let end = std::cmp::min(start + page_size, filtered.len());
        let slice: Vec<serde_json::Value> = filtered[start..end].iter().map(|&v| v.clone()).collect();
        Ok(slice)
    } else {
        Ok(vec![])
    }
}

#[command]
pub async fn lookup_packages_by_names(
    state: tauri::State<'_, AppState>,
    game_id: String,
    names: Vec<String>
) -> Result<serde_json::Value, String> {
    let packages_lock = state.packages.read().await;
    
    if let Some(packages) = packages_lock.get(&game_id) {
        let mut found = Vec::new();
        let mut unknown = Vec::new();
        
        let re = regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").unwrap();
        
        for name in names {
            // Strip version if present: "Author-Mod-1.0.0" -> "Author-Mod"
            let clean_name = if let Some(caps) = re.captures(&name) {
                caps.get(1).map_or(name.clone(), |m| m.as_str().to_string())
            } else {
                name.clone()
            };
            
            if let Some(pkg) = packages.iter().find(|p| {
                p["full_name"].as_str().unwrap_or("") == clean_name
            }) {
                found.push(pkg.clone());
            } else {
                unknown.push(name.clone());
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
pub async fn fetch_package_by_name(state: tauri::State<'_, AppState>, name: String, game_id: Option<String>) -> Result<Option<serde_json::Value>, String> {
    // name might be "Namespace-Name" or "Namespace-Name-Version"
    
    // 1. Strip version if present (Regex: ^(.*)-(\d+\.\d+\.\d+)$)
    let re = regex::Regex::new(r"^(.*)-(\d+\.\d+\.\d+)$").unwrap();
    let clean_name = if let Some(caps) = re.captures(&name) {
        caps.get(1).map_or(name.clone(), |m| m.as_str().to_string())
    } else {
        name.clone()
    };

    // 2. Check Cache if game_id is provided
    if let Some(gid) = game_id {
        let packages_lock = state.packages.read().await;
        if let Some(packages) = packages_lock.get(&gid) {
            // Find package in cache
            // Cache structure: Array of package objects
            // We need to match full_name or name
            // The cache stores objects with "full_name": "Namespace-Name"
            
            let target_name = clean_name.to_lowercase();
            
            if let Some(pkg) = packages.iter().find(|p| {
                 p["full_name"].as_str().unwrap_or("").to_lowercase() == target_name
            }) {
                eprintln!("[fetch_package_by_name] Found {} in cache for game {}", clean_name, gid);
                return Ok(Some(pkg.clone()));
            }
        }
    }

    eprintln!("[fetch_package_by_name] Cache miss for {}. Fetching from API...", clean_name);

    // 3. Split Namespace and Name
    let parts: Vec<&str> = clean_name.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Ok(None);
    }
    let namespace = parts[0];
    let package_name = parts[1];

    let url = format!("https://thunderstore.io/api/v1/package/{}/{}/", namespace, package_name);
    let client = reqwest::Client::builder()
        .user_agent("r2modmac/0.0.1")
        .build()
        .map_err(|e| e.to_string())?;
    
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;

    if response.status() == 404 {
        return Ok(None);
    }
    
    if !response.status().is_success() {
        return Err(format!("Failed to fetch package: {}", response.status()));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(Some(json))
}
