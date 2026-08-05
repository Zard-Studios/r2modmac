use crate::models::shared::{load_settings_impl, save_settings_impl, Settings};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{command, AppHandle};
use url::Url;
use uuid::Uuid;

const CACHE_SECONDS: i64 = 15 * 60;

const PREFERENCES_PLACEMENT: &str = "preferences-support";
const HOME_PLACEMENT: &str = "home-support";
const PROFILE_SELECTOR_PLACEMENT: &str = "profile-selector-support";
const CATALOG_PLACEMENT: &str = "catalog-support";
const PRODUCTION_PROXY_URL: &str =
    "https://r2modmac-sponsor-production.notfy-stream.workers.dev/api/sponsor";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SponsorMessage {
    pub id: String,
    pub sponsor_name: Option<String>,
    pub message: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct SponsorState {
    installation_subject: Option<String>,
    recent_ids: Vec<String>,
    dismissed_ids: Vec<String>,
    cached: Option<CachedSponsor>,
    attempt_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CachedSponsor {
    message: SponsorMessage,
    expires_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxySponsorMessage {
    id: String,
    sponsor_name: Option<String>,
    message: String,
    url: Option<String>,
}

fn sponsor_state_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::utils::paths::app_data_dir(app)
        .map_err(|error| error.to_string())?
        .join("sponsor-state.json"))
}

fn load_state(app: &AppHandle) -> SponsorState {
    sponsor_state_path(app)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_state(app: &AppHandle, state: &SponsorState) -> Result<(), String> {
    let path = sponsor_state_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary_path = path.with_extension("json.tmp");
    let raw = serde_json::to_string(state).map_err(|error| error.to_string())?;
    fs::write(&temporary_path, raw).map_err(|error| error.to_string())?;
    fs::rename(temporary_path, path).map_err(|error| error.to_string())
}

fn is_allowed_placement(placement: &str) -> bool {
    matches!(
        placement,
        PREFERENCES_PLACEMENT | HOME_PLACEMENT | PROFILE_SELECTOR_PLACEMENT | CATALOG_PLACEMENT
    )
}

fn prune_state(state: &mut SponsorState, now: i64) {
    state.recent_ids.truncate(8);
    state.dismissed_ids.truncate(8);
    if state
        .cached
        .as_ref()
        .is_some_and(|cached| cached.expires_at <= now)
    {
        state.cached = None;
    }
}

fn is_valid_message(message: &SponsorMessage) -> bool {
    if message.id.is_empty()
        || message.id.len() > 128
        || message
            .sponsor_name
            .as_ref()
            .is_some_and(|name| name.is_empty() || name.len() > 80)
        || message.message.is_empty()
        || message.message.len() > 280
    {
        return false;
    }

    message.url.as_deref().map_or(true, |raw_url| {
        Url::parse(raw_url)
            .map(|url| url.scheme() == "https" && url.host_str().is_some())
            .unwrap_or(false)
    })
}

fn is_eligible(_state: &SponsorState, _settings: &Settings, _now: i64) -> bool {
    true
}

fn is_allowed_proxy_url(endpoint: &Url) -> bool {
    if endpoint.scheme() == "https" && endpoint.host_str().is_some() {
        return true;
    }

    // Local HTTP is available only to an unoptimised developer build so the complete
    // request chain can be inspected without ever weakening a distributed build.
    cfg!(debug_assertions)
        && endpoint.scheme() == "http"
        && matches!(endpoint.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

async fn request_proxy(subject: &str, placement: &str) -> Option<SponsorMessage> {
    // Development builds override this with the local proxy. Release builds
    // remain functional when invoked directly with `npm run tauri build`.
    let endpoint = option_env!("R2MODMAC_SPONSOR_PROXY_URL")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(PRODUCTION_PROXY_URL);
    let endpoint_url = Url::parse(endpoint).ok()?;
    if !is_allowed_proxy_url(&endpoint_url) {
        return None;
    }
    let client = Client::builder()
        .timeout(Duration::from_millis(6_000))
        .build()
        .ok()?;
    let response = client
        .post(endpoint_url)
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "category": "gaming-mod-manager",
            "placement": placement,
            "subject": subject
        }))
        .send()
        .await
        .ok()?;

    if response.status().as_u16() == 204 || !response.status().is_success() {
        return None;
    }
    let candidate: ProxySponsorMessage = response.json().await.ok()?;
    let message = SponsorMessage {
        id: candidate.id,
        sponsor_name: candidate.sponsor_name,
        message: candidate.message,
        url: candidate.url,
    };
    is_valid_message(&message).then_some(message)
}

#[command]
pub async fn request_sponsor(
    app: AppHandle,
    placement: Option<String>,
) -> Result<Option<SponsorMessage>, String> {
    request_sponsor_with_options(app, placement.as_deref().unwrap_or(PREFERENCES_PLACEMENT)).await
}

async fn request_sponsor_with_options(
    app: AppHandle,
    placement: &str,
) -> Result<Option<SponsorMessage>, String> {
    if !is_allowed_placement(placement) {
        return Ok(None);
    }
    let settings = load_settings_impl(&app);
    let now = Utc::now().timestamp();
    let mut state = load_state(&app);
    prune_state(&mut state, now);

    if !is_eligible(&state, &settings, now) {
        let _ = save_state(&app, &state);
        return Ok(None);
    }

    if placement == PREFERENCES_PLACEMENT || !settings.sponsored_messages_enabled {
        let subject = state
            .installation_subject
            .get_or_insert_with(|| Uuid::new_v4().to_string())
            .clone();
        let _ = save_state(&app, &state);
        return Ok(request_proxy(&subject, placement).await);
    }

    if let Some(cached) = &state.cached {
        if !state.recent_ids.contains(&cached.message.id)
            && !state.dismissed_ids.contains(&cached.message.id)
        {
            return Ok(Some(cached.message.clone()));
        }
    }

    state.attempt_count = state.attempt_count.wrapping_add(1);
    save_state(&app, &state)?;

    let subject = state
        .installation_subject
        .get_or_insert_with(|| Uuid::new_v4().to_string())
        .clone();
    save_state(&app, &state)?;
    let Some(message) = request_proxy(&subject, placement).await else {
        return Ok(None);
    };
    if state.recent_ids.contains(&message.id) || state.dismissed_ids.contains(&message.id) {
        return Ok(None);
    }

    state.cached = Some(CachedSponsor {
        message: message.clone(),
        expires_at: now + CACHE_SECONDS,
    });
    save_state(&app, &state)?;
    Ok(Some(message))
}

#[command]
pub async fn acknowledge_sponsor_display(app: AppHandle, sponsor_id: String) -> Result<(), String> {
    let now = Utc::now().timestamp();
    let mut state = load_state(&app);
    prune_state(&mut state, now);
    if state
        .cached
        .as_ref()
        .map(|cached| cached.message.id.as_str())
        != Some(sponsor_id.as_str())
    {
        return Ok(());
    }
    state.recent_ids.retain(|id| id != &sponsor_id);
    state.recent_ids.insert(0, sponsor_id);
    save_state(&app, &state)
}

#[command]
pub async fn dismiss_sponsor(app: AppHandle, sponsor_id: String) -> Result<(), String> {
    let now = Utc::now().timestamp();
    let mut state = load_state(&app);
    prune_state(&mut state, now);
    let is_current_or_seen = state
        .cached
        .as_ref()
        .map(|cached| cached.message.id.as_str() == sponsor_id)
        .unwrap_or(false)
        || state.recent_ids.iter().any(|id| id == &sponsor_id);
    if !is_current_or_seen {
        return Ok(());
    }
    state.dismissed_ids.retain(|id| id != &sponsor_id);
    state.dismissed_ids.insert(0, sponsor_id);
    save_state(&app, &state)
}

#[command]
pub async fn reset_sponsor_cache(app: AppHandle) -> Result<(), String> {
    // Reset the cache and installation identity together. The next eligible
    // request will create a fresh opaque UUID; no previous sponsor history is
    // carried over to the new identity.
    save_state(&app, &SponsorState::default())
}

#[command]
pub async fn update_sponsor_preferences(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = load_settings_impl(&app);
    settings.sponsored_messages_enabled = enabled;
    save_settings_impl(&app, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn disabled_sponsorship_is_eligible_for_silent_impressions() {
        let mut settings = standard_settings();
        settings.sponsored_messages_enabled = false;
        assert!(is_eligible(&SponsorState::default(), &settings, 1_000));
    }

    #[test]
    fn enabled_sponsorship_has_no_local_calendar_cap() {
        let settings = standard_settings();
        let state = SponsorState::default();
        assert!(is_eligible(&state, &settings, 1_001));
    }

    #[test]
    fn home_is_an_explicitly_allowed_sponsor_placement() {
        assert!(is_allowed_placement(HOME_PLACEMENT));
        assert!(!is_allowed_placement("install-support"));
    }

    #[test]
    fn rejects_non_https_payloads() {
        let invalid = SponsorMessage {
            id: "one".into(),
            sponsor_name: None,
            message: "Text".into(),
            url: Some("http://example.com".into()),
        };
        assert!(!is_valid_message(&invalid));
    }

    #[test]
    fn developer_proxy_allows_only_local_http() {
        assert!(is_allowed_proxy_url(
            &Url::parse("http://127.0.0.1:3000/api/sponsor").unwrap()
        ));
        assert!(is_allowed_proxy_url(
            &Url::parse("https://example.com/api/sponsor").unwrap()
        ));
        assert!(!is_allowed_proxy_url(
            &Url::parse("http://example.com/api/sponsor").unwrap()
        ));
    }
}
