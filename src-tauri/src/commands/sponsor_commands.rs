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

const REDUCED_ATTEMPT_BACKOFF_SECONDS: i64 = 5 * 60;
const MONTH_SECONDS: i64 = 30 * 24 * 60 * 60;
const CACHE_SECONDS: i64 = 15 * 60;

const PREFERENCES_PLACEMENT: &str = "preferences-support";
const PROFILE_SELECTOR_PLACEMENT: &str = "profile-selector-support";
const CATALOG_PLACEMENT: &str = "catalog-support";

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
    last_attempt_at: Option<i64>,
    last_shown_at: Option<i64>,
    shown_at: Vec<i64>,
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
        PREFERENCES_PLACEMENT | PROFILE_SELECTOR_PLACEMENT | CATALOG_PLACEMENT
    )
}

fn prune_state(state: &mut SponsorState, now: i64) {
    state.shown_at.retain(|shown| *shown > now - MONTH_SECONDS);
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

fn is_eligible(state: &SponsorState, settings: &Settings, now: i64) -> bool {
    if !settings.sponsored_messages_enabled {
        return false;
    }
    !settings.sponsored_messages_less_frequently
        || !state
            .last_attempt_at
            .is_some_and(|attempt| attempt > now - REDUCED_ATTEMPT_BACKOFF_SECONDS)
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
    let endpoint = option_env!("R2MODMAC_SPONSOR_PROXY_URL")?;
    let endpoint_url = Url::parse(endpoint).ok()?;
    if !is_allowed_proxy_url(&endpoint_url) {
        return None;
    }
    let client = Client::builder()
        .timeout(Duration::from_millis(2_500))
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

    if !settings.sponsored_messages_enabled {
        let _ = save_state(&app, &state);
        return Ok(None);
    }
    if !is_eligible(&state, &settings, now) {
        let _ = save_state(&app, &state);
        return Ok(None);
    }

    if let Some(cached) = &state.cached {
        if !state.recent_ids.contains(&cached.message.id)
            && !state.dismissed_ids.contains(&cached.message.id)
        {
            return Ok(Some(cached.message.clone()));
        }
    }

    state.last_attempt_at = Some(now);
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
    state.last_shown_at = Some(now);
    state.shown_at.push(now);
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
    state.last_shown_at = Some(now);
    state.shown_at.push(now);
    state.dismissed_ids.retain(|id| id != &sponsor_id);
    state.dismissed_ids.insert(0, sponsor_id);
    save_state(&app, &state)
}

#[command]
pub async fn reset_sponsor_cache(app: AppHandle) -> Result<(), String> {
    let subject = load_state(&app).installation_subject;
    save_state(
        &app,
        &SponsorState {
            installation_subject: subject,
            ..Default::default()
        },
    )
}

#[command]
pub async fn update_sponsor_preferences(
    app: AppHandle,
    enabled: bool,
    less_frequently: bool,
) -> Result<(), String> {
    let mut settings = load_settings_impl(&app);
    settings.sponsored_messages_enabled = enabled;
    settings.sponsored_messages_less_frequently = less_frequently;
    save_settings_impl(&app, &settings)?;

    if !enabled {
        save_state(&app, &SponsorState::default())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard_settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn disabled_sponsorship_is_never_eligible() {
        let mut settings = standard_settings();
        settings.sponsored_messages_enabled = false;
        assert!(!is_eligible(&SponsorState::default(), &settings, 1_000));
    }

    #[test]
    fn enabled_sponsorship_has_no_local_calendar_cap() {
        let settings = standard_settings();
        let state = SponsorState { shown_at: vec![1_000; 100], ..Default::default() };
        assert!(is_eligible(&state, &settings, 1_001));
    }

    #[test]
    fn reduced_frequency_uses_a_short_local_backoff() {
        let mut settings = standard_settings();
        settings.sponsored_messages_less_frequently = true;
        let state = SponsorState {
            last_attempt_at: Some(1_000),
            ..Default::default()
        };
        assert!(!is_eligible(&state, &settings, 1_001));
        assert!(is_eligible(&state, &settings, 1_301));
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
