use crate::models::shared::UpdateInfo;
use serde_json::Value;

use super::legacy_system_commands::compare_versions;

fn expected_update_filename_for(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("r2modmac_macos_aarch64.dmg"),
        ("macos", "x86_64") => Ok("r2modmac_macos_x86_64.dmg"),
        ("windows", "x86_64") => Ok("r2modmac_windows_x64.zip"),
        ("windows", "x86") | ("windows", "i686") => Ok("r2modmac_windows_x86.zip"),
        ("windows", "aarch64") => Ok("r2modmac_windows_arm64.zip"),
        ("linux", "x86_64") => Ok("r2modmac_linux_x64.tar.gz"),
        ("linux", "aarch64") => Ok("r2modmac_linux_arm64.tar.gz"),
        (os, arch) => Err(format!(
            "Automatic updates are not supported on {} {}",
            os, arch
        )),
    }
}

fn select_update_asset<'a>(assets: &'a [Value], os: &str, arch: &str) -> Option<&'a Value> {
    let expected = expected_update_filename_for(os, arch).ok()?;
    assets.iter().find(|asset| {
        asset
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name == expected)
    })
}

#[tauri::command]
pub async fn check_update_secure(current_version: String) -> Result<UpdateInfo, String> {
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

    let release: Value = response
        .json()
        .await
        .map_err(|error| format!("Parse error: {}", error))?;
    let tag_name = release
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Release is missing tag_name".to_string())?;
    let clean_tag = tag_name.trim_start_matches('v');
    let available = compare_versions(clean_tag, &current_version);
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let asset = select_update_asset(assets, std::env::consts::OS, std::env::consts::ARCH);
    let download_url = asset
        .and_then(|asset| asset.get("browser_download_url"))
        .and_then(Value::as_str)
        .map(str::to_string);

    eprintln!(
        "[check_update] Detected OS: {}, architecture: {}, selected asset: {:?}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        asset
            .and_then(|asset| asset.get("name"))
            .and_then(Value::as_str)
    );

    Ok(UpdateInfo {
        available,
        version: tag_name.to_string(),
        notes: release
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        download_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_architectures_to_exact_assets() {
        assert_eq!(
            expected_update_filename_for("linux", "x86_64").unwrap(),
            "r2modmac_linux_x64.tar.gz"
        );
        assert_eq!(
            expected_update_filename_for("linux", "aarch64").unwrap(),
            "r2modmac_linux_arm64.tar.gz"
        );
        assert_eq!(
            expected_update_filename_for("macos", "aarch64").unwrap(),
            "r2modmac_macos_aarch64.dmg"
        );
        assert_eq!(
            expected_update_filename_for("windows", "x86_64").unwrap(),
            "r2modmac_windows_x64.zip"
        );
        assert!(expected_update_filename_for("linux", "x86").is_err());
    }

    #[test]
    fn selects_only_the_exact_linux_architecture_asset() {
        let assets = vec![
            serde_json::json!({"name":"r2modmac_linux_arm64.tar.gz"}),
            serde_json::json!({"name":"r2modmac_linux_x64.tar.gz"}),
        ];
        assert_eq!(
            select_update_asset(&assets, "linux", "x86_64")
                .unwrap()
                .get("name")
                .and_then(Value::as_str),
            Some("r2modmac_linux_x64.tar.gz")
        );
        assert_eq!(
            select_update_asset(&assets, "linux", "aarch64")
                .unwrap()
                .get("name")
                .and_then(Value::as_str),
            Some("r2modmac_linux_arm64.tar.gz")
        );
    }
}
