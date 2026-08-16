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

const SPONSOR_ROTATION_SECONDS: i64 = 15;
const _: () = assert!(SPONSOR_ROTATION_SECONDS >= 15);

const DISMISS_QUIET_SECONDS: i64 = SPONSOR_ROTATION_SECONDS;
const SESSION_SUBJECT_COOLDOWN_SECS: i64 = 60;

const PREFERENCES_PLACEMENT: &str = "preferences-support";
const HOME_PLACEMENT: &str = "home-support";
const PROFILE_SELECTOR_PLACEMENT: &str = "profile-selector-support";
const CATALOG_PLACEMENT: &str = "catalog-support";
const SPONSOR_CATEGORY: &str = "general";

const PRODUCTION_PROXY_URL: &str =
    "https://r2modmac-sponsor-production.notfy-stream.workers.dev/api/sponsor";

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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
    session_subject: Option<String>,
    session_subject_minted_at: Option<i64>,
    recent_ids: Vec<String>,
    dismissed_until: Option<i64>,
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
    let raw = crate::utils::stable_json::to_pretty_string(state)?;
    fs::write(&temporary_path, raw).map_err(|error| error.to_string())?;
    fs::rename(temporary_path, path).map_err(|error| error.to_string())
}

fn should_rotate_session_subject(state: &SponsorState, now: i64) -> bool {
    match (
        state.session_subject.as_ref(),
        state.session_subject_minted_at,
    ) {
        (Some(_), Some(minted_at)) => {
            now.saturating_sub(minted_at) >= SESSION_SUBJECT_COOLDOWN_SECS
        }
        _ => true,
    }
}

/// Mint a fresh sponsor identity for this app session, unless one was already
/// minted within the last minute — closing and immediately relaunching would
/// otherwise churn through a new subject on every restart.
///
/// This only ever touches `session_subject`, the identity used by the normal
/// (enabled) sponsor surfaces. It never reads or writes `installation_subject`.
pub fn rotate_session_subject(app: &AppHandle) {
    let now = Utc::now().timestamp();
    let mut state = load_state(app);
    prune_state(&mut state, now);

    if should_rotate_session_subject(&state, now) {
        state.session_subject = Some(Uuid::new_v4().to_string());
        state.session_subject_minted_at = Some(now);
        state.recent_ids.clear();
        state.dismissed_until = None;
        state.cached = None;
    }

    let _ = save_state(app, &state);
}

fn is_allowed_placement(placement: &str) -> bool {
    matches!(
        placement,
        PREFERENCES_PLACEMENT | HOME_PLACEMENT | PROFILE_SELECTOR_PLACEMENT | CATALOG_PLACEMENT
    )
}

fn prune_state(state: &mut SponsorState, now: i64) {
    state.recent_ids.truncate(8);
    if state.dismissed_until.is_some_and(|until| until <= now) {
        state.dismissed_until = None;
    }
    if state
        .cached
        .as_ref()
        .is_some_and(|cached| cached.expires_at <= now)
    {
        state.cached = None;
    }
}

#[derive(Debug, PartialEq)]
enum CacheDecision {
    Serve(SponsorMessage),
    StayQuiet,
    Fetch,
}

fn decide_from_cache(state: &SponsorState, now: i64) -> CacheDecision {
    if state.dismissed_until.is_some_and(|until| until > now) {
        return CacheDecision::StayQuiet;
    }

    match &state.cached {
        Some(cached) => CacheDecision::Serve(cached.message.clone()),
        None => CacheDecision::Fetch,
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

fn resolve_proxy_endpoint(compile_time_override: Option<&'static str>) -> &'static str {
    compile_time_override
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(PRODUCTION_PROXY_URL)
}

async fn request_proxy(subject: &str, placement: &str) -> Option<SponsorMessage> {
    // Development builds override this with the local proxy. Release builds
    // remain functional when invoked directly with `npm run tauri build`.
    let endpoint = resolve_proxy_endpoint(option_env!("R2MODMAC_SPONSOR_PROXY_URL"));
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
            "category": SPONSOR_CATEGORY,
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

    match decide_from_cache(&state, now) {
        CacheDecision::Serve(message) => return Ok(Some(message)),
        CacheDecision::StayQuiet => return Ok(None),
        CacheDecision::Fetch => {}
    }

    state.attempt_count = state.attempt_count.wrapping_add(1);
    save_state(&app, &state)?;

    let subject = state
        .session_subject
        .get_or_insert_with(|| Uuid::new_v4().to_string())
        .clone();
    save_state(&app, &state)?;
    let Some(message) = request_proxy(&subject, placement).await else {
        return Ok(None);
    };

    state.cached = Some(CachedSponsor {
        message: message.clone(),
        expires_at: now + SPONSOR_ROTATION_SECONDS,
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
    let is_current = state
        .cached
        .as_ref()
        .is_some_and(|cached| cached.message.id == sponsor_id);
    if !is_current {
        return Ok(());
    }
    state.cached = None;
    state.dismissed_until = Some(now + DISMISS_QUIET_SECONDS);
    save_state(&app, &state)
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

    // ──────────────────────────────────────────────────────────────────────────
    // should_rotate_session_subject — session identity rotation, cooldown-gated
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn rotates_on_first_use_with_no_prior_subject() {
        assert!(should_rotate_session_subject(
            &SponsorState::default(),
            1_000
        ));
    }

    #[test]
    fn does_not_rotate_within_the_cooldown_window() {
        let state = SponsorState {
            session_subject: Some("existing".into()),
            session_subject_minted_at: Some(1_000),
            ..Default::default()
        };
        assert!(!should_rotate_session_subject(
            &state,
            1_000 + SESSION_SUBJECT_COOLDOWN_SECS - 1
        ));
    }

    #[test]
    fn rotates_once_the_cooldown_has_elapsed() {
        let state = SponsorState {
            session_subject: Some("existing".into()),
            session_subject_minted_at: Some(1_000),
            ..Default::default()
        };
        assert!(should_rotate_session_subject(
            &state,
            1_000 + SESSION_SUBJECT_COOLDOWN_SECS
        ));
    }

    #[test]
    fn session_subject_rotation_never_reads_or_implies_installation_subject() {
        // installation_subject backs the preferences-support / sponsorship-disabled
        // path and must stay completely independent of session rotation: a state
        // with only installation_subject set (no session_subject) must still be
        // treated as needing its first session rotation.
        let state = SponsorState {
            installation_subject: Some("stable-forever".into()),
            ..Default::default()
        };
        assert!(should_rotate_session_subject(&state, 1_000));
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

    // ──────────────────────────────────────────────────────────────────────────
    // resolve_proxy_endpoint — simulates the GitHub Actions / release-build path
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn proxy_url_falls_back_to_production_when_env_var_is_absent() {
        // Simulates: secret not set at all (option_env! returns None)
        assert_eq!(
            resolve_proxy_endpoint(None),
            PRODUCTION_PROXY_URL,
            "A missing env var must use the hardcoded production URL"
        );
    }

    #[test]
    fn proxy_url_falls_back_to_production_when_env_var_is_empty() {
        // Simulates: GitHub Actions secret exists but value is "" (common misconfiguration)
        assert_eq!(
            resolve_proxy_endpoint(Some("")),
            PRODUCTION_PROXY_URL,
            "An empty env var must not override the production URL"
        );
    }

    #[test]
    fn proxy_url_falls_back_to_production_when_env_var_is_whitespace_only() {
        // Simulates: secret set to "  " (accidental whitespace)
        assert_eq!(
            resolve_proxy_endpoint(Some("   ")),
            PRODUCTION_PROXY_URL,
            "A whitespace-only env var must be treated as absent"
        );
    }

    #[test]
    fn proxy_url_uses_custom_override_when_present() {
        // Simulates: developer local proxy configured correctly
        let custom = "https://custom.example.com/api/sponsor";
        assert_eq!(
            resolve_proxy_endpoint(Some(custom)),
            custom,
            "A non-empty env var must override the production URL"
        );
    }

    #[test]
    fn proxy_url_trims_surrounding_whitespace_before_use() {
        // Simulates: secret value accidentally padded with spaces
        let custom = "  https://custom.example.com/api/sponsor  ";
        let resolved = resolve_proxy_endpoint(Some(custom));
        assert_eq!(resolved, custom.trim());
    }

    #[test]
    fn production_proxy_url_is_valid_https() {
        // Guards against accidental typos in the hardcoded constant
        let url = Url::parse(PRODUCTION_PROXY_URL)
            .expect("PRODUCTION_PROXY_URL must be a well-formed URL");
        assert_eq!(url.scheme(), "https", "Production proxy must use HTTPS");
        assert!(
            url.host_str().is_some(),
            "Production proxy must have a hostname"
        );
    }

    #[test]
    fn production_proxy_url_is_allowed_by_gateway_check() {
        // Ensures the hardcoded URL would actually pass is_allowed_proxy_url()
        // i.e. the build would never silently drop every ad request
        let url = Url::parse(PRODUCTION_PROXY_URL).unwrap();
        assert!(
            is_allowed_proxy_url(&url),
            "Production proxy URL must pass is_allowed_proxy_url — if this fails ads are dead in every release build"
        );
    }

    // ──────────────────────────────────────────────────────────────────────────
    // is_valid_message — validates ad payload before showing/counting
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn valid_message_with_no_url_passes() {
        let m = SponsorMessage {
            id: "abc123".into(),
            sponsor_name: Some("Acme".into()),
            message: "Try Acme today!".into(),
            url: None,
        };
        assert!(is_valid_message(&m));
    }

    #[test]
    fn valid_message_with_https_url_passes() {
        let m = SponsorMessage {
            id: "abc123".into(),
            sponsor_name: None,
            message: "Try Acme today!".into(),
            url: Some("https://acme.example.com".into()),
        };
        assert!(is_valid_message(&m));
    }

    #[test]
    fn rejects_message_with_empty_id() {
        let m = SponsorMessage {
            id: "".into(),
            sponsor_name: None,
            message: "Text".into(),
            url: None,
        };
        assert!(!is_valid_message(&m));
    }

    #[test]
    fn rejects_message_with_oversized_id() {
        let m = SponsorMessage {
            id: "x".repeat(129),
            sponsor_name: None,
            message: "Text".into(),
            url: None,
        };
        assert!(!is_valid_message(&m));
    }

    #[test]
    fn rejects_message_with_empty_text() {
        let m = SponsorMessage {
            id: "id1".into(),
            sponsor_name: None,
            message: "".into(),
            url: None,
        };
        assert!(!is_valid_message(&m));
    }

    #[test]
    fn rejects_message_exceeding_280_chars() {
        let m = SponsorMessage {
            id: "id1".into(),
            sponsor_name: None,
            message: "x".repeat(281),
            url: None,
        };
        assert!(!is_valid_message(&m));
    }

    #[test]
    fn rejects_message_with_empty_sponsor_name() {
        let m = SponsorMessage {
            id: "id1".into(),
            sponsor_name: Some("".into()),
            message: "Text".into(),
            url: None,
        };
        assert!(!is_valid_message(&m));
    }

    #[test]
    fn rejects_message_with_non_https_url() {
        let m = SponsorMessage {
            id: "id1".into(),
            sponsor_name: None,
            message: "Text".into(),
            url: Some("http://evil.com".into()),
        };
        assert!(!is_valid_message(&m));
    }

    #[test]
    fn rejects_message_with_malformed_url() {
        let m = SponsorMessage {
            id: "id1".into(),
            sponsor_name: None,
            message: "Text".into(),
            url: Some("not-a-url".into()),
        };
        assert!(!is_valid_message(&m));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // prune_state — cache expiry and list trimming
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn the_cached_ad_expires_so_the_next_one_can_take_its_place() {
        let mut state = cached_state("imp_1");
        state.cached.as_mut().unwrap().expires_at = 1_000 + SPONSOR_ROTATION_SECONDS;

        prune_state(&mut state, 1_000 + SPONSOR_ROTATION_SECONDS);

        assert_eq!(
            decide_from_cache(&state, 1_000 + SPONSOR_ROTATION_SECONDS),
            CacheDecision::Fetch,
            "once the window passes the surface must ask for the next sponsor"
        );
    }

    fn cached_state(id: &str) -> SponsorState {
        SponsorState {
            cached: Some(CachedSponsor {
                message: SponsorMessage {
                    id: id.into(),
                    sponsor_name: None,
                    message: "Text".into(),
                    url: None,
                },
                expires_at: i64::MAX,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn an_ad_that_was_already_shown_is_still_served_from_cache() {
        let mut state = cached_state("imp_1");
        state.recent_ids = vec!["imp_1".into()];

        match decide_from_cache(&state, 1_000) {
            CacheDecision::Serve(message) => assert_eq!(message.id, "imp_1"),
            other => panic!("a shown ad must stay cached, got {:?}", other),
        }
    }

    #[test]
    fn dismissing_keeps_the_surface_quiet_without_asking_the_network_again() {
        let mut state = cached_state("imp_1");
        state.dismissed_until = Some(2_000);

        assert_eq!(decide_from_cache(&state, 1_000), CacheDecision::StayQuiet);
    }

    #[test]
    fn a_dismissal_wears_off_and_the_same_creative_may_return() {
        let mut state = cached_state("imp_1");
        state.dismissed_until = Some(2_000);
        prune_state(&mut state, 2_000);

        assert_eq!(state.dismissed_until, None);
        match decide_from_cache(&state, 2_000) {
            CacheDecision::Serve(message) => assert_eq!(message.id, "imp_1"),
            other => panic!("the quiet window must expire, got {:?}", other),
        }
    }

    #[test]
    fn an_empty_cache_is_the_only_reason_to_go_to_the_network() {
        assert_eq!(
            decide_from_cache(&SponsorState::default(), 1_000),
            CacheDecision::Fetch
        );
    }

    #[test]
    fn a_fresh_ad_is_served() {
        match decide_from_cache(&cached_state("imp_2"), 1_000) {
            CacheDecision::Serve(message) => assert_eq!(message.id, "imp_2"),
            other => panic!("expected Serve, got {:?}", other),
        }
    }

    #[test]
    fn prune_state_clears_expired_cache() {
        let msg = SponsorMessage {
            id: "id1".into(),
            sponsor_name: None,
            message: "Text".into(),
            url: None,
        };
        let mut state = SponsorState {
            cached: Some(CachedSponsor {
                message: msg,
                expires_at: 999,
            }),
            ..Default::default()
        };
        prune_state(&mut state, 1_000);
        assert!(state.cached.is_none(), "Expired cache must be cleared");
    }

    #[test]
    fn prune_state_keeps_fresh_cache() {
        let msg = SponsorMessage {
            id: "id1".into(),
            sponsor_name: None,
            message: "Text".into(),
            url: None,
        };
        let mut state = SponsorState {
            cached: Some(CachedSponsor {
                message: msg,
                expires_at: 2_000,
            }),
            ..Default::default()
        };
        prune_state(&mut state, 1_000);
        assert!(state.cached.is_some(), "Non-expired cache must be kept");
    }

    #[test]
    fn prune_state_trims_recent_ids_to_eight() {
        let mut state = SponsorState {
            recent_ids: (0..12).map(|i| i.to_string()).collect(),
            ..Default::default()
        };
        prune_state(&mut state, 0);
        assert_eq!(state.recent_ids.len(), 8);
    }

    #[test]
    fn prune_state_keeps_a_quiet_window_that_has_not_elapsed() {
        let mut state = SponsorState {
            dismissed_until: Some(2_000),
            ..Default::default()
        };
        prune_state(&mut state, 1_999);
        assert_eq!(state.dismissed_until, Some(2_000));
    }

    // ──────────────────────────────────────────────────────────────────────────
    // is_allowed_placement — gate for unknown/injected placement strings
    // ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn all_known_placements_are_allowed() {
        for placement in [
            PREFERENCES_PLACEMENT,
            HOME_PLACEMENT,
            PROFILE_SELECTOR_PLACEMENT,
            CATALOG_PLACEMENT,
        ] {
            assert!(
                is_allowed_placement(placement),
                "Placement '{placement}' should be allowed"
            );
        }
    }

    #[test]
    fn unknown_placement_strings_are_rejected() {
        for bad in [
            "",
            "install-support",
            "CATALOG-SUPPORT",
            "catalog_support",
            "admin",
        ] {
            assert!(
                !is_allowed_placement(bad),
                "Placement '{bad}' should be rejected"
            );
        }
    }
}
