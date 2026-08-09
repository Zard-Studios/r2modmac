use crate::models::shared::*;
use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tauri::{command, AppHandle, Emitter, Manager, State};

const COMMUNITIES_CACHE_FILE: &str = "communities_v1.json.gz";
const COMMUNITY_IMAGES_CACHE_FILE: &str = "community_images_v1.json.gz";

fn read_gz_json_cache<T: serde::de::DeserializeOwned>(
    app: &AppHandle,
    file_name: &str,
) -> Option<T> {
    use std::io::Read;
    let cache_dir = crate::utils::paths::app_cache_dir(app).ok()?;
    let cache_file = cache_dir.join(file_name);
    let file = std::fs::File::open(&cache_file).ok()?;
    let mut gz = flate2::read::GzDecoder::new(file);
    let mut data = Vec::new();
    gz.read_to_end(&mut data).ok()?;
    serde_json::from_slice(&data).ok()
}

fn write_gz_json_cache<T: serde::Serialize>(app: &AppHandle, file_name: &str, value: &T) {
    use std::io::Write;
    let Ok(cache_dir) = crate::utils::paths::app_cache_dir(app) else {
        return;
    };
    if std::fs::create_dir_all(&cache_dir).is_err() {
        return;
    }
    let Ok(file) = std::fs::File::create(cache_dir.join(file_name)) else {
        return;
    };
    let Ok(serialized) = serde_json::to_vec(value) else {
        return;
    };
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    if encoder.write_all(&serialized).is_err() {
        return;
    }
    let _ = encoder.finish();
}

async fn fetch_communities_live() -> Result<Vec<serde_json::Value>, String> {
    let mut url = Some("https://thunderstore.io/api/experimental/community/".to_string());
    let mut all_results = Vec::new();

    while let Some(current_url) = url {
        let resp = reqwest::get(&current_url)
            .await
            .map_err(|e| e.to_string())?;
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

        if let Some(results) = json.get("results").and_then(|v| v.as_array()) {
            log::debug!(
                "[fetch_communities] Fetched {} communities from {}",
                results.len(),
                current_url
            );
            all_results.extend(results.clone());
        }

        // API uses pagination.next_link instead of "next"
        url = json
            .get("pagination")
            .and_then(|p| p.get("next_link"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    // Inject the synthetic Outer Wilds community (not on Thunderstore; uses ow-mod-db).
    all_results.push(serde_json::json!({
        "identifier": "outerwilds",
        "name": "Outer Wilds",
        "discord_url": "https://discord.gg/outerwilds",
        "wiki_url": "https://outerwildsmods.com",
        "require_package_listing_approval": false
    }));

    log::debug!(
        "[fetch_communities] Total communities fetched: {}",
        all_results.len()
    );
    Ok(all_results)
}

async fn fetch_community_images_live() -> Result<std::collections::HashMap<String, String>, String>
{
    let url = "https://thunderstore.io/communities/";
    let resp = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let html = resp.text().await.map_err(|e| e.to_string())?;

    let mut images = extract_community_images_from_html(&html)?;

    // The listing page stops at 300 entries while the API keeps growing, so the
    // newest communities never appear on it and end up with no cover at all.
    // Their own pages do carry one, so the gap is closed from there.
    fill_missing_community_images(&mut images).await;

    // Inject Outer Wilds cover image (not on Thunderstore CDN).
    images.entry("outerwilds".to_string()).or_insert_with(|| {
        "https://i.ibb.co/xKCBqXM7/apps-7475-67120997535715720-38c3e502-0019-4560-826e-634bbaf5cb4b.jpg".to_string()
    });

    Ok(images)
}

/// How many community pages a single refresh will fetch to fill gaps.
///
/// Only the communities past the listing page's limit are normally missing — a
/// handful. The cap exists so that a change at Thunderstore that broke the main
/// scrape entirely would degrade to a few requests rather than three hundred.
const MAX_COMMUNITY_PAGE_LOOKUPS: usize = 40;

/// How many community pages are read at once while backfilling covers.
const COMMUNITY_PAGE_CONCURRENCY: usize = 6;

/// Fetch covers, one community page at a time, for whatever the listing missed.
///
/// The obvious shortcut — deriving `assets/<id>/<id>-cover-360x480.webp` — was
/// measured against the live site and matches only 255 of 299 communities, so
/// the filename genuinely cannot be assumed. Reading each page is slower but
/// right.
async fn fill_missing_community_images(images: &mut HashMap<String, String>) {
    let communities = match fetch_communities_live().await {
        Ok(list) => list,
        Err(e) => {
            log::warn!("[fetch_community_images] Could not list communities: {}", e);
            return;
        }
    };

    let missing: Vec<String> = communities
        .iter()
        .filter_map(|c| c.get("identifier").and_then(|v| v.as_str()))
        .map(|id| id.trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty() && !images.contains_key(id))
        .take(MAX_COMMUNITY_PAGE_LOOKUPS)
        .collect();

    if missing.is_empty() {
        return;
    }
    log::debug!(
        "[fetch_community_images] {} communities missing a cover; reading their pages",
        missing.len()
    );

    // Read the pages a few at a time rather than one after another. Forty pages
    // at a couple of hundred milliseconds each is the better part of ten seconds
    // of waiting, and it runs as a background refresh after every launch — which
    // is the burst of network traffic that shows up once the app already looks
    // ready. Six at a time keeps it well short of anything Thunderstore would
    // consider a hammering.
    let covers = stream::iter(missing.into_iter().map(|id| async move {
        let url = format!("https://thunderstore.io/c/{}/", id);
        let page = reqwest::get(&url).await.ok()?.text().await.ok()?;
        let cover = extract_cover_from_community_page(&page, &id)?;
        Some((id, cover))
    }))
    .buffer_unordered(COMMUNITY_PAGE_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    for (id, cover) in covers.into_iter().flatten() {
        insert_community_image(images, &id, &cover, true);
    }
}

/// Pick the cover out of one community's own page.
///
/// The page also carries a wide background and a square icon for the same
/// community, so the choice is narrowed to assets under that community's own
/// path and then left to the usual cover preference.
fn extract_cover_from_community_page(html: &str, community_id: &str) -> Option<String> {
    let prefix = format!("https://gcdn.thunderstore.io/assets/{}/", community_id);
    let re = regex::Regex::new(r#"https://gcdn\.thunderstore\.io/[^"'\\\s<>]+"#).ok()?;

    let mut fallback = None;
    for m in re.find_iter(html) {
        let url = m.as_str();
        if !url.starts_with(&prefix) {
            continue;
        }
        if looks_like_community_cover(url) {
            return Some(url.to_string());
        }
        if fallback.is_none() && is_usable_community_image(url) {
            fallback = Some(url.to_string());
        }
    }
    fallback
}

/// The game list (name/identifier), used both for the home screen and to
/// label whichever single game a startup "default game" skip lands on.
///
/// `refresh` distinguishes two callers with very different needs:
///   - `false` (the startup-skip path): serve the on-disk cache only, with
///     zero network activity — reading one game's name out of a cached list
///     that already covers all ~300 is free, whereas the live endpoint has
///     no per-game filter and would mean fetching the whole catalog just to
///     read one entry.
///   - `true` (opening the home screen): serve the cache immediately for a
///     fast paint, then refresh it from Thunderstore in the background so
///     the *next* load — cached or not — is current. Mirrors the existing
///     package-listing cache in mod_commands.rs.
/// When there is no cache yet (fresh install), a live fetch is unavoidable
/// either way — there is nothing else to serve.
///
/// This is also what keeps the app usable offline: with a cache on disk,
/// `refresh: true` still returns instantly from the cache even when the
/// background refresh's network request fails.
#[command]
pub async fn fetch_communities(
    app: AppHandle,
    refresh: Option<bool>,
) -> Result<Vec<serde_json::Value>, String> {
    let refresh = refresh.unwrap_or(true);
    if let Some(cached) = read_gz_json_cache::<Vec<serde_json::Value>>(&app, COMMUNITIES_CACHE_FILE)
    {
        if refresh {
            let app_for_task = app.clone();
            tokio::spawn(async move {
                match fetch_communities_live().await {
                    Ok(fresh) => write_gz_json_cache(&app_for_task, COMMUNITIES_CACHE_FILE, &fresh),
                    Err(e) => log::warn!("[fetch_communities] Background refresh failed: {}", e),
                }
            });
        }
        return Ok(cached);
    }

    let fresh = fetch_communities_live().await?;
    write_gz_json_cache(&app, COMMUNITIES_CACHE_FILE, &fresh);
    Ok(fresh)
}

/// See `fetch_communities` for the `refresh` contract — identical here.
#[command]
pub async fn fetch_community_images(
    app: AppHandle,
    refresh: Option<bool>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let refresh = refresh.unwrap_or(true);
    if let Some(cached) = read_gz_json_cache::<std::collections::HashMap<String, String>>(
        &app,
        COMMUNITY_IMAGES_CACHE_FILE,
    ) {
        if refresh {
            let app_for_task = app.clone();
            tokio::spawn(async move {
                match fetch_community_images_live().await {
                    Ok(fresh) => {
                        write_gz_json_cache(&app_for_task, COMMUNITY_IMAGES_CACHE_FILE, &fresh)
                    }
                    Err(e) => {
                        log::warn!("[fetch_community_images] Background refresh failed: {}", e)
                    }
                }
            });
        }
        return Ok(cached);
    }

    let fresh = fetch_community_images_live().await?;
    write_gz_json_cache(&app, COMMUNITY_IMAGES_CACHE_FILE, &fresh);
    Ok(fresh)
}

fn normalize_community_image_url(url: &str) -> String {
    url.replace("&amp;", "&").replace("\\/", "/")
}

fn is_community_cdn_image(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://gcdn.thunderstore.io/live/community/")
        || lower.starts_with("https://gcdn.thunderstore.io/assets/")
        // Thunderstore has used this shorter path for newer communities.
        || lower.starts_with("https://gcdn.thunderstore.io/community/")
}

fn looks_like_community_cover(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let has_cover_hint = lower.contains("360x480")
        || lower.contains("cover")
        || lower.contains("-cov")
        || lower.contains("_cov");
    let obvious_non_cover = lower.contains("-bg-")
        || lower.contains("_bg")
        || lower.contains("-icon-")
        || lower.contains("_icon")
        || lower.contains("icon-192")
        || lower.contains("logo");

    is_community_cdn_image(url) && has_cover_hint && !obvious_non_cover
}

fn is_usable_community_image(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let obvious_non_cover = lower.contains("-bg-")
        || lower.contains("_bg")
        || lower.contains("-icon-")
        || lower.contains("_icon")
        || lower.contains("icon-192")
        || lower.contains("logo");

    is_community_cdn_image(url) && !obvious_non_cover
}

fn insert_community_image(
    images: &mut HashMap<String, String>,
    community_id: &str,
    image_url: &str,
    require_cover_hint: bool,
) {
    let community_id = community_id.trim().trim_matches('/').to_ascii_lowercase();
    if community_id.is_empty() {
        return;
    }

    let image_url = normalize_community_image_url(image_url);
    if !is_community_cdn_image(&image_url) {
        return;
    }
    if require_cover_hint && !looks_like_community_cover(&image_url) {
        return;
    }
    if !require_cover_hint && !is_usable_community_image(&image_url) {
        return;
    }

    images.entry(community_id).or_insert(image_url);
}

fn extract_community_images_from_html(html: &str) -> Result<HashMap<String, String>, String> {
    let mut images = HashMap::new();

    // Thunderstore renders community cards with either /live/community/... or /assets/...
    // CDN paths. Prefer the image inside the community link because the path filename
    // can drift independently from the community identifier.
    let re_community_link = regex::Regex::new(r#"<a[^>]+href=["']/c/([^/"']+)/["'][^>]*>"#)
        .map_err(|e| e.to_string())?;
    let re_img_src = regex::Regex::new(r#"src=["'](https://gcdn\.thunderstore\.io/[^"']+)["']"#)
        .map_err(|e| e.to_string())?;

    for cap in re_community_link.captures_iter(html) {
        let (Some(link), Some(community_id)) = (cap.get(0), cap.get(1)) else {
            continue;
        };
        let rest = &html[link.end()..];
        let Some(anchor_end) = rest.find("</a>") else {
            continue;
        };
        let anchor_body = &rest[..anchor_end];
        let Some(img_cap) = re_img_src.captures(anchor_body) else {
            continue;
        };
        let Some(image_url) = img_cap.get(1) else {
            continue;
        };

        insert_community_image(
            &mut images,
            community_id.as_str(),
            image_url.as_str(),
            false,
        );
    }

    // Fallback for preload tags and serialized React Router payloads. Thunderstore
    // does not guarantee a stable cover filename, so derive the community id from
    // any CDN asset while still rejecting obvious hero/icon/logo assets.
    let re_direct_cover = regex::Regex::new(
        r#"(https://gcdn\.thunderstore\.io/(?:live/community|assets|community)/([^/"'\\\s<>]+)/[^"'\\\s<>]+)"#,
    )
    .map_err(|e| e.to_string())?;

    for cap in re_direct_cover.captures_iter(html) {
        if let (Some(image_url), Some(community_id)) = (cap.get(1), cap.get(2)) {
            insert_community_image(
                &mut images,
                community_id.as_str(),
                image_url.as_str(),
                false,
            );
        }
    }

    Ok(images)
}

const MAX_TEXT_CONTENT_BYTES: usize = 5 * 1024 * 1024;

fn validate_text_content_url(raw_url: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw_url).map_err(|_| "Invalid URL".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err("Only standard HTTPS URLs are allowed".to_string());
    }

    match url.host_str().map(|host| host.to_ascii_lowercase()) {
        Some(host)
            if matches!(
                host.as_str(),
                "api.github.com" | "thunderstore.io" | "www.thunderstore.io"
            ) =>
        {
            Ok(url)
        }
        _ => Err("URL host is not allowed".to_string()),
    }
}

#[command]
pub async fn fetch_text_content(url: String) -> Result<String, String> {
    let url = validate_text_content_url(&url)?;
    let client = reqwest::Client::builder()
        .user_agent("r2modmac")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Remote server returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TEXT_CONTENT_BYTES as u64)
    {
        return Err("Remote text content is too large".to_string());
    }

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_TEXT_CONTENT_BYTES {
        return Err("Remote text content is too large".to_string());
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| "Remote response is not valid UTF-8 text".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformLookupInput {
    pub identifier: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SteamSearchItem {
    id: u32,
    name: String,
    #[serde(default)]
    platforms: Option<SteamPlatforms>,
}

#[derive(Debug, Clone, Deserialize)]
struct SteamPlatforms {
    windows: bool,
    mac: bool,
    linux: bool,
}

#[derive(Debug, Deserialize)]
struct SteamAppData {
    platforms: SteamPlatforms,
}

#[derive(Debug, Deserialize)]
struct SteamAppDetailsEntry {
    success: bool,
    data: Option<SteamAppData>,
}

fn normalize_for_match(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn similarity_score(query: &str, candidate: &str) -> f32 {
    let q = normalize_for_match(query);
    let c = normalize_for_match(candidate);
    if q.is_empty() || c.is_empty() {
        return 0.0;
    }
    if q == c {
        return 1.0;
    }
    if c.contains(&q) || q.contains(&c) {
        return 0.85;
    }

    let q_tokens: Vec<&str> = q.split_whitespace().collect();
    let c_tokens: Vec<&str> = c.split_whitespace().collect();
    if q_tokens.is_empty() || c_tokens.is_empty() {
        return 0.0;
    }

    let overlap = q_tokens.iter().filter(|t| c_tokens.contains(t)).count() as f32;
    overlap / (q_tokens.len().max(c_tokens.len()) as f32)
}

async fn fetch_steam_platforms(client: &reqwest::Client, app_id: u32) -> Option<PlatformInfo> {
    let url = format!(
        "https://store.steampowered.com/api/appdetails?appids={}&cc=us&l=en&filters=platforms",
        app_id
    );
    let resp = client
        .get(&url)
        .header("User-Agent", "r2modmac-platform-resolver/1.0")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let entry = json.get(app_id.to_string())?;
    let parsed: SteamAppDetailsEntry = serde_json::from_value(entry.clone()).ok()?;
    if !parsed.success {
        return None;
    }

    let data = parsed.data?;
    Some(PlatformInfo {
        windows: data.platforms.windows,
        mac: data.platforms.mac,
        linux: data.platforms.linux,
        confidence: 0.9,
        source: "steam_store".to_string(),
    })
}

async fn resolve_from_steam(
    client: &reqwest::Client,
    input: &PlatformLookupInput,
) -> Option<PlatformInfo> {
    let mut best: Option<(SteamSearchItem, f32)> = None;
    let mut search_terms = vec![input.name.clone(), input.identifier.replace('-', " ")];
    // Composite community names like "Titanfall 2: Northstar" should also try base game title.
    // This avoids weak matches on the mod framework name.
    if let Some((base, _)) = input.name.split_once(':') {
        let trimmed = base.trim();
        if !trimmed.is_empty() {
            search_terms.push(trimmed.to_string());
        }
    }
    if let Some((base, _)) = input.name.split_once(" - ") {
        let trimmed = base.trim();
        if !trimmed.is_empty() {
            search_terms.push(trimmed.to_string());
        }
    }
    search_terms.sort();
    search_terms.dedup();
    for term in search_terms {
        let search_url = format!(
            "https://store.steampowered.com/api/storesearch/?term={}&l=en&cc=us",
            urlencoding::encode(&term)
        );
        let resp = client
            .get(&search_url)
            .header("User-Agent", "r2modmac-platform-resolver/1.0")
            .send()
            .await
            .ok();
        let Some(resp) = resp else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let json: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let items = match json.get("items").and_then(|v| v.as_array()) {
            Some(v) => v,
            None => continue,
        };
        for item in items.iter().take(8) {
            let id = match item.get("id").and_then(|v| v.as_u64()) {
                Some(v) => v as u32,
                None => continue,
            };
            let item_name = match item.get("name").and_then(|v| v.as_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            let platforms = item.get("platforms").and_then(|p| {
                Some(SteamPlatforms {
                    windows: p.get("windows")?.as_bool()?,
                    mac: p.get("mac")?.as_bool()?,
                    linux: p.get("linux")?.as_bool()?,
                })
            });
            let parsed_item = SteamSearchItem {
                id,
                name: item_name,
                platforms,
            };
            let score_by_name = similarity_score(&input.name, &parsed_item.name);
            let score_by_id = similarity_score(&input.identifier, &parsed_item.name);
            let score = score_by_name.max(score_by_id);
            if score > best.as_ref().map(|(_, s)| *s).unwrap_or(0.0) {
                best = Some((parsed_item, score));
            }
        }
    }

    let (best_item, score) = best?;
    if score < 0.55 {
        return None;
    }

    // Prefer platforms from storesearch itself (already public Steam Store API data),
    // fallback to appdetails if missing.
    let mut info = if let Some(p) = best_item.platforms {
        PlatformInfo {
            windows: p.windows,
            mac: p.mac,
            linux: p.linux,
            confidence: 0.0,
            source: String::new(),
        }
    } else {
        fetch_steam_platforms(client, best_item.id).await?
    };

    info.confidence = if score >= 0.8 {
        // High-confidence Steam match is authoritative for platform support.
        (0.9 + (score - 0.8) * 0.45).min(0.99)
    } else {
        (0.45 + score * 0.5).min(0.89)
    };
    info.source = format!("steam_store:{}:{}", best_item.id, best_item.name);
    Some(info)
}

async fn resolve_from_wikipedia(
    client: &reqwest::Client,
    input: &PlatformLookupInput,
) -> Option<PlatformInfo> {
    let query = format!("{} {}", input.name, input.identifier.replace('-', " "));
    for lang in ["en", "it"] {
        let search_url = format!(
            "https://{}.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&format=json&srlimit=1",
            lang,
            urlencoding::encode(&query)
        );

        let search_resp = match client
            .get(&search_url)
            .header("User-Agent", "r2modmac-platform-resolver/1.0")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(_) => continue,
        };
        if !search_resp.status().is_success() {
            continue;
        }

        let search_json: serde_json::Value = match search_resp.json().await {
            Ok(json) => json,
            Err(_) => continue,
        };
        let first = match search_json
            .get("query")
            .and_then(|q| q.get("search"))
            .and_then(|s| s.as_array())
            .and_then(|arr| arr.first())
        {
            Some(v) => v,
            None => continue,
        };
        let page_title = match first.get("title").and_then(|t| t.as_str()) {
            Some(v) => v,
            None => continue,
        };

        let page_title_lower = page_title.to_lowercase();
        if page_title_lower.contains("disambiguation")
            || page_title_lower.starts_with("list of ")
            || page_title_lower.starts_with("lista di ")
        {
            continue;
        }
        let title_score = similarity_score(&input.name, page_title)
            .max(similarity_score(&input.identifier, page_title));
        if title_score < 0.55 {
            continue;
        }
        let snippet = first
            .get("snippet")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_lowercase();
        let mentions_video_game = snippet.contains("video game")
            || snippet.contains("videogame")
            || snippet.contains("videogioco")
            || snippet.contains("game");
        if !mentions_video_game && title_score < 0.7 {
            continue;
        }

        // Full plaintext extract is slower than exintro but catches platform data in body/infobox text.
        let extract_url = format!(
            "https://{}.wikipedia.org/w/api.php?action=query&prop=extracts&explaintext=1&titles={}&format=json",
            lang,
            urlencoding::encode(page_title)
        );

        let extract_resp = match client
            .get(&extract_url)
            .header("User-Agent", "r2modmac-platform-resolver/1.0")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(_) => continue,
        };
        if !extract_resp.status().is_success() {
            continue;
        }

        let extract_json: serde_json::Value = match extract_resp.json().await {
            Ok(json) => json,
            Err(_) => continue,
        };
        let pages = match extract_json
            .get("query")
            .and_then(|q| q.get("pages"))
            .and_then(|p| p.as_object())
        {
            Some(v) => v,
            None => continue,
        };
        let page = match pages.values().next() {
            Some(v) => v,
            None => continue,
        };
        let extract = match page.get("extract").and_then(|e| e.as_str()) {
            Some(v) => v.to_lowercase(),
            None => continue,
        };

        let windows = extract.contains("windows") || extract.contains("microsoft windows");
        let mac = extract.contains("macos")
            || extract.contains("mac os")
            || extract.contains("mac os x")
            || extract.contains("os x");
        let linux = extract.contains("linux");

        if !windows && !mac && !linux {
            continue;
        }

        let mut confidence = 0.42 + title_score * 0.33;
        if mentions_video_game {
            confidence += 0.08;
        }

        return Some(PlatformInfo {
            windows,
            mac,
            linux,
            confidence: confidence.min(0.84),
            source: format!("wikipedia:{}:{}", lang, page_title),
        });
    }

    None
}

async fn resolve_from_wikidata(
    client: &reqwest::Client,
    input: &PlatformLookupInput,
) -> Option<PlatformInfo> {
    let mut best_qid: Option<(String, f32, bool)> = None;
    for term in [input.name.as_str(), input.identifier.as_str()] {
        let search_url = format!(
            "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language=en&format=json&limit=5&type=item",
            urlencoding::encode(term)
        );
        let resp = match client
            .get(&search_url)
            .header("User-Agent", "r2modmac-platform-resolver/1.0")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(_) => continue,
        };
        if !resp.status().is_success() {
            continue;
        }
        let json: serde_json::Value = match resp.json().await {
            Ok(json) => json,
            Err(_) => continue,
        };
        let items = match json.get("search").and_then(|s| s.as_array()) {
            Some(v) => v,
            None => continue,
        };
        for item in items {
            let qid = match item.get("id").and_then(|v| v.as_str()) {
                Some(v) => v.to_string(),
                None => continue,
            };
            let label = item
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let description_lc = description.to_lowercase();
            let is_video_game =
                description_lc.contains("video game") || description_lc.contains("videogame");
            let mut score = similarity_score(&input.name, label)
                .max(similarity_score(&input.identifier, label));
            if is_video_game {
                score += 0.12;
            }
            if !is_video_game && score < 0.72 {
                continue;
            }
            if score > best_qid.as_ref().map(|(_, s, _)| *s).unwrap_or(0.0) {
                best_qid = Some((qid, score, is_video_game));
            }
        }
    }

    let (qid, score, is_video_game) = best_qid?;
    if score < 0.5 {
        return None;
    }

    let entity_url = format!(
        "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={}&props=claims|labels&languages=en&format=json",
        qid
    );
    let resp = client
        .get(&entity_url)
        .header("User-Agent", "r2modmac-platform-resolver/1.0")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let claims = json
        .get("entities")
        .and_then(|e| e.get(&qid))
        .and_then(|ent| ent.get("claims"))
        .and_then(|c| c.get("P400"))
        .and_then(|arr| arr.as_array())?;

    let mut platform_qids: Vec<String> = Vec::new();
    for claim in claims {
        if let Some(pid) = claim
            .get("mainsnak")
            .and_then(|m| m.get("datavalue"))
            .and_then(|d| d.get("value"))
            .and_then(|v| v.get("id"))
            .and_then(|id| id.as_str())
        {
            platform_qids.push(pid.to_string());
        }
    }
    if platform_qids.is_empty() {
        return None;
    }

    let labels_url = format!(
        "https://www.wikidata.org/w/api.php?action=wbgetentities&ids={}&props=labels&languages=en&format=json",
        platform_qids.join("|")
    );
    let labels_resp = client
        .get(&labels_url)
        .header("User-Agent", "r2modmac-platform-resolver/1.0")
        .send()
        .await
        .ok()?;
    if !labels_resp.status().is_success() {
        return None;
    }
    let labels_json: serde_json::Value = labels_resp.json().await.ok()?;
    let entities = labels_json.get("entities").and_then(|e| e.as_object())?;

    let mut windows = false;
    let mut mac = false;
    let mut linux = false;

    for ent in entities.values() {
        let lbl = ent
            .get("labels")
            .and_then(|l| l.get("en"))
            .and_then(|en| en.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_lowercase();
        if lbl.contains("windows") {
            windows = true;
        }
        if lbl.contains("macos")
            || lbl.contains("mac os")
            || lbl.contains("os x")
            || lbl.contains("macintosh")
        {
            mac = true;
        }
        if lbl.contains("linux") {
            linux = true;
        }
    }

    if !windows && !mac && !linux {
        return None;
    }

    let mut confidence = 0.5 + score * 0.3;
    if is_video_game {
        confidence += 0.08;
    }

    Some(PlatformInfo {
        windows,
        mac,
        linux,
        confidence: confidence.min(0.9),
        source: format!("wikidata:{}", qid),
    })
}

fn heuristic_platform(input: &PlatformLookupInput) -> PlatformInfo {
    let combined = format!("{} {}", input.identifier, input.name).to_lowercase();
    let legacy_mac_hints = [
        "btd6",
        "valheim",
        "slimerancher",
        "stardewvalley",
        "subnautica",
        "subnauticabelowzero",
        "hollowknight",
        "celeste",
        "kerbalspaceprogram",
        "outward",
        "inscryption",
        "cities_skylines",
        "20minutes-till-dawn",
        "stacklands",
        "timberborn",
        "dontstarvetogether",
        "factorio",
        "garrysmod",
        "oxygennotincluded",
        "projectzomboid",
        "rimworld",
        "terraria",
    ];
    if legacy_mac_hints.contains(&input.identifier.as_str()) {
        return PlatformInfo {
            windows: true,
            mac: true,
            linux: false,
            confidence: 0.7,
            source: "heuristic:legacy_mac_hint".to_string(),
        };
    }
    let java_like = combined.contains("minecraft")
        || combined.contains("hypixel")
        || combined.contains("hytale")
        || combined.contains("java");
    if java_like {
        return PlatformInfo {
            windows: true,
            mac: true,
            linux: true,
            confidence: 0.6,
            source: "heuristic:java_crossplatform".to_string(),
        };
    }

    // Fallback: unknown game -> conservative default (Windows only)
    // until a public source confirms macOS support.
    PlatformInfo {
        windows: true,
        mac: false,
        linux: false,
        confidence: 0.35,
        source: "heuristic:unknown".to_string(),
    }
}

fn merge_platform_candidates(
    input: &PlatformLookupInput,
    candidates: Vec<PlatformInfo>,
) -> PlatformInfo {
    if candidates.is_empty() {
        return heuristic_platform(input);
    }

    let is_steam_source = |source: &str| source.starts_with("steam_store:");
    let steam_candidate = candidates
        .iter()
        .filter(|c| is_steam_source(&c.source))
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

    // Strong Steam match is authoritative for platform flags.
    if let Some(authoritative_steam) = steam_candidate.clone() {
        if authoritative_steam.confidence >= 0.9 {
            return authoritative_steam;
        }
    }

    // If Steam confidently says macOS is not supported, avoid false positives from weaker sources.
    let steam_blocks_mac = steam_candidate
        .as_ref()
        .map(|s| s.confidence >= 0.8 && !s.mac)
        .unwrap_or(false);

    if let Some(authoritative_steam) = candidates
        .iter()
        .filter(|c| is_steam_source(&c.source))
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
    {
        if authoritative_steam.confidence >= 0.9 {
            return authoritative_steam;
        }
    }

    let mut ordered = candidates;
    ordered.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let best_confidence = ordered.first().map(|c| c.confidence).unwrap_or(0.0);

    let mut considered: Vec<PlatformInfo> = Vec::new();
    for candidate in ordered {
        let close_to_best = candidate.confidence + 0.12 >= best_confidence;
        let has_useful_signal = candidate.confidence >= 0.56;
        if close_to_best || has_useful_signal || is_steam_source(&candidate.source) {
            considered.push(candidate);
        }
    }

    if considered.is_empty() {
        return heuristic_platform(input);
    }

    let mut merged = considered[0].clone();
    for candidate in considered.iter().skip(1) {
        merged.windows = merged.windows || candidate.windows;
        if !steam_blocks_mac {
            merged.mac = merged.mac || candidate.mac;
        }
        merged.linux = merged.linux || candidate.linux;
        merged.confidence = merged.confidence.max(candidate.confidence);
    }
    if steam_blocks_mac {
        merged.mac = false;
    }

    let mut sources: Vec<String> = Vec::new();
    for c in &considered {
        if !sources.contains(&c.source) {
            sources.push(c.source.clone());
        }
    }
    merged.source = if sources.len() == 1 {
        sources[0].clone()
    } else {
        format!("merge:{}", sources.join("|"))
    };

    // If every provider returned no usable platform signal, fallback heuristic.
    if !merged.windows && !merged.mac && !merged.linux {
        heuristic_platform(input)
    } else {
        merged
    }
}

#[command]
pub async fn resolve_community_platforms(
    state: State<'_, AppState>,
    games: Vec<PlatformLookupInput>,
) -> Result<HashMap<String, PlatformInfo>, String> {
    let now = chrono::Utc::now().timestamp();
    let ttl_seconds: i64 = 24 * 60 * 60;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let mut result: HashMap<String, PlatformInfo> = HashMap::new();

    let mut unresolved: Vec<PlatformLookupInput> = Vec::new();
    for game in games {
        let cache_key = game.identifier.to_lowercase();
        if let Some(cached) = state.platform_cache.read().await.get(&cache_key).cloned() {
            let entry_ttl = if cached.info.source.starts_with("heuristic:unknown") {
                15 * 60
            } else if cached.info.source.starts_with("heuristic:") {
                6 * 60 * 60
            } else {
                ttl_seconds
            };
            if now - cached.fetched_at < entry_ttl {
                result.insert(cache_key.clone(), cached.info);
                continue;
            }
        }
        unresolved.push(game);
    }

    let resolved_pairs: Vec<(String, PlatformInfo)> =
        stream::iter(unresolved.into_iter().map(|game| {
            let client = client.clone();
            async move {
                let key = game.identifier.to_lowercase();
                let steam = resolve_from_steam(&client, &game).await;
                if let Some(steam_info) = steam.clone() {
                    if steam_info.confidence >= 0.9 {
                        return (key, steam_info);
                    }
                }

                let (wikidata, wiki) = tokio::join!(
                    resolve_from_wikidata(&client, &game),
                    resolve_from_wikipedia(&client, &game)
                );
                let mut candidates: Vec<PlatformInfo> = Vec::new();
                if let Some(s) = steam {
                    candidates.push(s);
                }
                if let Some(wd) = wikidata {
                    candidates.push(wd);
                }
                if let Some(w) = wiki {
                    candidates.push(w);
                }

                let resolved = merge_platform_candidates(&game, candidates);
                (key, resolved)
            }
        }))
        .buffer_unordered(10)
        .collect()
        .await;

    {
        let mut cache = state.platform_cache.write().await;
        for (key, resolved) in resolved_pairs {
            cache.insert(
                key.clone(),
                CachedPlatform {
                    info: resolved.clone(),
                    fetched_at: now,
                },
            );
            result.insert(key, resolved);
        }
    }

    Ok(result)
}

#[command]
pub async fn confirm_dialog(
    app: AppHandle,
    title: String,
    message: String,
) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_dialog::MessageDialogButtons;

    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;

    let ans = app
        .dialog()
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

    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;

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
pub fn get_username() -> Result<String, String> {
    if let Ok(user) = std::env::var("USER") {
        if !user.trim().is_empty() {
            return Ok(user);
        }
    }
    if let Ok(user) = std::env::var("USERNAME") {
        if !user.trim().is_empty() {
            return Ok(user);
        }
    }
    if let Some(home) = dirs::home_dir() {
        if let Some(name) = home.file_name() {
            if let Some(name_str) = name.to_str() {
                return Ok(name_str.to_string());
            }
        }
    }
    Err("Could not retrieve username".to_string())
}

#[command]
pub async fn select_file(
    app: AppHandle,
    filters: Option<Vec<FileFilter>>,
) -> Result<Option<String>, String> {
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
pub async fn select_import_path(app: AppHandle) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = &app;
        let output = std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(
                r#"POSIX path of (choose file or folder with prompt "Select a custom mod folder, .zip/.r2z mod archive, or r2modmac profile export")"#,
            )
            .output()
            .map_err(|e| format!("Failed to open import picker: {}", e))?;

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Ok((!path.is_empty()).then_some(path));
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_ascii_lowercase().contains("user canceled") {
            return Ok(None);
        }

        return Err(stderr.trim().to_string());
    }

    #[cfg(not(target_os = "macos"))]
    {
        select_file(
            app,
            Some(vec![FileFilter {
                name: "Mod or Profile Archives".to_string(),
                extensions: vec!["r2z".to_string(), "zip".to_string()],
            }]),
        )
        .await
    }
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
    let extension = path_buf
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };

    Ok(Some(format!("data:{};base64,{}", mime, base64_str)))
}

/// Breadth-limited recursive lookup for a file name below `dir`.
#[cfg(target_os = "windows")]
fn find_file_in_dir(
    dir: &std::path::Path,
    file_name: &str,
    depth: usize,
) -> Option<std::path::PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut directories = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.is_file() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(file_name))
            {
                return Some(path);
            }
        } else if path.is_dir() {
            directories.push(path);
        }
    }
    if depth == 0 {
        return None;
    }
    directories
        .into_iter()
        .find_map(|child| find_file_in_dir(&child, file_name, depth - 1))
}

fn select_update_asset<'a>(
    assets: &'a [GithubAsset],
    os: &str,
    arch: &str,
) -> Option<&'a GithubAsset> {
    let arch_pattern = match (os, arch) {
        ("macos", "aarch64") => "aarch64",
        ("macos", "x86_64") => "x86_64",
        ("windows", "x86_64") => "x64",
        ("windows", "x86") | ("windows", "i686") => "x86",
        ("windows", "aarch64") => "arm64",
        _ => return None,
    };
    let allowed_extensions: &[&str] = match os {
        "windows" => &[".zip"],
        "macos" => &[".dmg", ".tar.gz", ".zip"],
        _ => return None,
    };

    assets.iter().find(|asset| {
        let name = asset.name.to_lowercase();
        allowed_extensions.iter().any(|ext| name.ends_with(ext))
            && name.contains(os)
            && name.contains(arch_pattern)
    })
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
        log::debug!(
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

    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    // Simple version comparison (naive string compare for now, ideally use semver)
    // Assume tag_name is "vX.X.X" and current_version is "X.X.X"
    let clean_tag = release.tag_name.trim_start_matches('v');

    // Use semver crate if available, or simple split compare
    let is_newer = compare_versions(clean_tag, &current_version);

    // Detect system architecture and OS
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    log::debug!("[check_update] Detected OS: {}, architecture: {}", os, arch);

    // Require an exact OS + architecture match. Installing a fallback built for
    // another CPU architecture is more dangerous than reporting it unavailable.
    let asset = select_update_asset(&release.assets, os, arch);

    log::debug!(
        "[check_update] current={} latest={} is_newer={} selected_asset={:?} available_assets={:?}",
        current_version,
        clean_tag,
        is_newer,
        asset.map(|a| &a.name),
        release
            .assets
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
    );

    Ok(UpdateInfo {
        available: is_newer,
        version: release.tag_name,
        notes: release.body,
        download_url: asset.map(|a| a.browser_download_url.clone()),
    })
}

pub async fn install_update(app: AppHandle, download_url: String) -> Result<(), String> {
    use std::process::Command;

    // 1. Download
    let temp_dir = app
        .path()
        .temp_dir()
        .map_err(|e| e.to_string())?
        .join("r2modmac_update");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let filename = download_url.split('/').last().unwrap_or("update.bin");
    let file_path = temp_dir.join(filename);

    log::info!(
        "[install_update] Downloading {} to {:?}",
        download_url,
        file_path
    );

    // Stream download to calculate progress
    use std::io::Write;

    let response = reqwest::Client::builder()
        .user_agent("r2modmac-updater")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Update download failed: {}", e))?;
    let total_size = response.content_length().unwrap_or(0);

    let mut file = fs::File::create(&file_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut body = response;

    loop {
        match body.chunk().await {
            Ok(Some(chunk)) => {
                file.write_all(&chunk).map_err(|e| e.to_string())?;
                downloaded += chunk.len() as u64;
                if total_size > 0 {
                    let percent = (downloaded as f64 / total_size as f64 * 100.0) as u8;
                    let _ = app.emit("update-progress", percent);
                }
            }
            Ok(None) => break,
            Err(error) => return Err(format!("Update download was interrupted: {}", error)),
        }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    if total_size > 0 && downloaded != total_size {
        return Err(format!(
            "Update download is incomplete: received {} of {} bytes",
            downloaded, total_size
        ));
    }
    log::debug!(
        "[install_update] Downloaded {} bytes (advertised {}) to {:?}",
        downloaded,
        total_size,
        file_path
    );

    #[cfg(target_os = "windows")]
    {
        if filename.ends_with(".zip") {
            // Extract r2modmac.exe from the zip using PowerShell
            let new_exe_path = temp_dir.join("r2modmac.exe");

            log::debug!(
                "[install_update] Extracting zip {:?} to {:?}",
                file_path,
                temp_dir
            );

            // Expand-Archive raises non-terminating errors, so PowerShell still
            // exits 0 when extraction fails. Force a terminating error and a
            // non-zero exit code so the real cause reaches the user instead of a
            // bare "exe not found".
            let extract_output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "$ErrorActionPreference='Stop'; try {{ Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force }} catch {{ Write-Error $_; exit 1 }}",
                        file_path.to_string_lossy(),
                        temp_dir.to_string_lossy()
                    ),
                ])
                .output()
                .map_err(|e| format!("Failed to extract zip: {}", e))?;

            if !extract_output.status.success() {
                return Err(format!(
                    "Zip extraction failed: {}",
                    String::from_utf8_lossy(&extract_output.stderr).trim()
                ));
            }

            log::debug!(
                "[install_update] Extraction finished; {:?} now contains: {:?}",
                temp_dir,
                fs::read_dir(&temp_dir)
                    .map(|entries| entries
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.file_name().to_string_lossy().to_string())
                        .collect::<Vec<_>>())
                    .unwrap_or_else(|error| vec![format!("<unreadable: {}>", error)])
            );

            // Older releases (and any future repackaging) may nest the binary in
            // a folder, so fall back to a recursive lookup before giving up.
            if !new_exe_path.is_file() {
                match find_file_in_dir(&temp_dir, "r2modmac.exe", 4) {
                    Some(found) => {
                        fs::rename(&found, &new_exe_path).map_err(|e| {
                            format!("Failed to move extracted r2modmac.exe into place: {}", e)
                        })?;
                    }
                    None => {
                        let stderr = String::from_utf8_lossy(&extract_output.stderr);
                        return Err(format!(
                            "r2modmac.exe was not found after extracting the update to {:?}. \
                             The archive may have been blocked or the extracted file removed by \
                             antivirus software. Extractor output: {}",
                            temp_dir,
                            if stderr.trim().is_empty() {
                                "(none)"
                            } else {
                                stderr.trim()
                            }
                        ));
                    }
                }
            }

            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

            // Write a batch script: wait for app to exit, replace binary, relaunch.
            let bat_path = temp_dir.join("r2modmac_updater.bat");
            let bat_content = format!(
                "@echo off\r\n\
                set /a attempts=0\r\n\
                :replace\r\n\
                timeout /t 1 /nobreak >nul\r\n\
                move /y \"{new_exe}\" \"{cur_exe}\" >nul 2>&1\r\n\
                if not exist \"{new_exe}\" goto replaced\r\n\
                set /a attempts+=1\r\n\
                if %attempts% LSS 30 goto replace\r\n\
                exit /b 1\r\n\
                :replaced\r\n\
                start \"\" \"{cur_exe}\"\r\n\
                del \"%~f0\"\r\n",
                new_exe = new_exe_path.to_string_lossy(),
                cur_exe = current_exe.to_string_lossy()
            );

            fs::write(&bat_path, bat_content).map_err(|e| e.to_string())?;

            log::info!(
                "[install_update] Launching updater batch script: {:?}",
                bat_path
            );
            Command::new("cmd")
                .args(["/c", bat_path.to_str().unwrap_or("")])
                .spawn()
                .map_err(|e| format!("Failed to launch updater script: {}", e))?;

            log::debug!("[install_update] Exiting app to allow update...");
            app.exit(0);
            Ok(())
        } else {
            // Legacy: run as a direct installer (.exe)
            log::debug!(
                "[install_update] Spawning Windows installer: {:?}",
                file_path
            );
            Command::new(&file_path)
                .spawn()
                .map_err(|e| format!("Failed to launch installer: {}", e))?;
            app.exit(0);
            Ok(())
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // 2. Prepare Update Script
        let script_path = temp_dir.join("update.sh");

        // GUARD: Check if we are in dev mode or not in a standard .app bundle
        // If current_exe is inside "target/debug" or "target/release", we are likely in dev/build.
        // Abort update to prevent deleting source code!
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_string = current_exe.to_string_lossy();
        if exe_string.contains("/target/debug/") || exe_string.contains("/target/release/") {
            log::debug!("Dev/Build environment detected. Skipping destructive update.");
            return Err(
                "Cannot auto-update in development environment. Build a release bundle to test."
                    .to_string(),
            );
        }

        // Determine extraction/mount commands based on file type
        let (extract_command, app_source) = if filename.ends_with(".tar.gz") {
            (
                format!(
                    "tar -xzf '{}' -C '{}'",
                    file_path.to_string_lossy(),
                    temp_dir.to_string_lossy()
                ),
                format!("{}/r2modmac.app", temp_dir.to_string_lossy()),
            )
        } else if filename.ends_with(".zip") {
            (
                format!(
                    "unzip -o '{}' -d '{}'",
                    file_path.to_string_lossy(),
                    temp_dir.to_string_lossy()
                ),
                format!("{}/r2modmac.app", temp_dir.to_string_lossy()),
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
                    format!("{}/r2modmac.app", mount_point),
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
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or("/Applications/r2modmac.app".to_string());

        // Build script with conditional DMG unmount
        let unmount_command = if is_dmg {
            format!("hdiutil detach '{}' -force -quiet || true", mount_point)
        } else {
            String::new()
        };

        // The script stages and validates the new bundle before moving the old
        // app aside. If replacement fails, the previous app is restored.
        let script = format!(
            r#"#!/bin/bash
set -u
PID={}
APP_PATH="{}"
UPDATE_DIR="{}"
APP_SOURCE="{}"
STAGED_APP="$UPDATE_DIR/r2modmac-staged.app"
BACKUP_APP="${{APP_PATH}}.r2modmac-backup"

echo "Waiting for PID $PID to exit..."
while kill -0 $PID 2>/dev/null; do sleep 0.5; done

echo "Extracting/Mounting Update..."
{}

if [ ! -d "$APP_SOURCE" ]; then
    echo "Error: New app not found at $APP_SOURCE"
    exit 1
fi

echo "Staging update..."
rm -rf "$STAGED_APP"
if ! cp -R "$APP_SOURCE" "$STAGED_APP"; then
    echo "Error: Could not stage the new app"
    {}
    exit 1
fi

if [ ! -x "$STAGED_APP/Contents/MacOS/r2modmac" ]; then
    echo "Error: Staged app bundle is incomplete"
    rm -rf "$STAGED_APP"
    {}
    exit 1
fi

# Unmount if needed (DMG)
{}

echo "Replacing app at $APP_PATH..."
rm -rf "$BACKUP_APP"
if [ -d "$APP_PATH" ]; then
    mv "$APP_PATH" "$BACKUP_APP" || exit 1
fi

if ! mv "$STAGED_APP" "$APP_PATH"; then
    echo "Error: Could not install the staged app; restoring previous version"
    rm -rf "$APP_PATH"
    if [ -d "$BACKUP_APP" ]; then mv "$BACKUP_APP" "$APP_PATH"; fi
    exit 1
fi

echo "Launching new app..."
if ! open "$APP_PATH"; then
    echo "Error: Could not launch updated app; restoring previous version"
    rm -rf "$APP_PATH"
    if [ -d "$BACKUP_APP" ]; then mv "$BACKUP_APP" "$APP_PATH"; fi
    open "$APP_PATH" || true
    exit 1
fi

rm -rf "$BACKUP_APP"

echo "Cleaning up..."
rm -rf "$UPDATE_DIR"
"#,
            std::process::id(),
            current_app_path,
            temp_dir.to_string_lossy(),
            app_source,
            extract_command,
            unmount_command,
            unmount_command,
            unmount_command
        );

        fs::write(&script_path, script).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        // 3. Launch Script detached
        log::info!("[install_update] Launching update script...");
        Command::new("sh")
            .arg(&script_path)
            .spawn()
            .map_err(|e| format!("Failed to launch script: {}", e))?;

        // 4. Exit App to allow script to proceed
        log::debug!("[install_update] Exiting app to allow update...");
        app.exit(0);

        Ok(())
    }
}

pub fn compare_versions(v1: &str, v2: &str) -> bool {
    let v1_parts: Vec<u32> = v1.split('.').filter_map(|s| s.parse().ok()).collect();
    let v2_parts: Vec<u32> = v2.split('.').filter_map(|s| s.parse().ok()).collect();

    for i in 0..std::cmp::max(v1_parts.len(), v2_parts.len()) {
        let p1 = v1_parts.get(i).unwrap_or(&0);
        let p2 = v2_parts.get(i).unwrap_or(&0);
        if p1 > p2 {
            return true;
        }
        if p1 < p2 {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_community_page_yields_its_cover_and_not_its_backdrop() {
        // Shape taken from the live pages of the two communities that the
        // listing page stops short of: it carries a wide backdrop, a square
        // icon and the cover, all under the same asset path.
        let page = r#"
            <img src="https://gcdn.thunderstore.io/assets/inside-the-backrooms/inside-the-backrooms-bg-1920x620.webp">
            <img src="https://gcdn.thunderstore.io/assets/inside-the-backrooms/inside-the-backrooms-icon-192x192.webp">
            <img src="https://gcdn.thunderstore.io/assets/inside-the-backrooms/inside-the-backrooms-cover-360x480.webp">
            <img src="https://gcdn.thunderstore.io/live/repository/icons/BepInEx-BepInExPack-5.4.2305.png">
        "#;

        let cover = extract_cover_from_community_page(page, "inside-the-backrooms")
            .expect("expected a cover");
        assert!(cover.ends_with("inside-the-backrooms-cover-360x480.webp"), "{}", cover);
    }

    #[test]
    fn another_community_asset_is_never_borrowed() {
        // Community pages list mod icons and can mention neighbours; a cover
        // must come from the community being looked up, not from whatever CDN
        // URL happens to appear first.
        let page = r#"
            <img src="https://gcdn.thunderstore.io/assets/some-other-game/some-other-game-cover-360x480.webp">
        "#;
        assert!(extract_cover_from_community_page(page, "modulus-factory-automation").is_none());
    }
    use super::*;

    fn release_asset(name: &str) -> GithubAsset {
        GithubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{}", name),
        }
    }

    #[test]
    fn selects_only_exact_update_os_and_architecture() {
        let assets = [
            release_asset("r2modmac_macos_aarch64.dmg"),
            release_asset("r2modmac_macos_x86_64.dmg"),
            release_asset("r2modmac_windows_x64.zip"),
            release_asset("r2modmac_windows_x86.zip"),
            release_asset("r2modmac_windows_arm64.zip"),
        ];

        assert_eq!(
            select_update_asset(&assets, "macos", "aarch64")
                .unwrap()
                .name,
            assets[0].name
        );
        assert_eq!(
            select_update_asset(&assets, "macos", "x86_64")
                .unwrap()
                .name,
            assets[1].name
        );
        assert_eq!(
            select_update_asset(&assets, "windows", "x86_64")
                .unwrap()
                .name,
            assets[2].name
        );
        assert_eq!(
            select_update_asset(&assets, "windows", "x86").unwrap().name,
            assets[3].name
        );
        assert_eq!(
            select_update_asset(&assets, "windows", "aarch64")
                .unwrap()
                .name,
            assets[4].name
        );
        assert!(select_update_asset(&assets[..1], "macos", "x86_64").is_none());
        assert!(select_update_asset(&assets[..2], "windows", "x86_64").is_none());
    }

    #[test]
    fn extracts_assets_cover_from_community_card() {
        let html = r#"
            <div class="card-community">
                <a tabindex="-1" title="Book of Travels" class="link" href="/c/book-of-travels/" data-discover="true">
                    <div class="image image--variant--primary card-community__image image--is3by4">
                        <div class="image__content image__content--variant--primary image--fullwidth">
                            <img class="image__src" loading="lazy" alt="" width="360" height="480" src="https://gcdn.thunderstore.io/assets/book-of-travels/book-of-travels-cover-360x480.webp"/>
                        </div>
                    </div>
                </a>
            </div>
        "#;

        let images = extract_community_images_from_html(html).unwrap();

        assert_eq!(
            images.get("book-of-travels").map(String::as_str),
            Some("https://gcdn.thunderstore.io/assets/book-of-travels/book-of-travels-cover-360x480.webp")
        );
    }

    #[test]
    fn extracts_assets_cover_urls_from_serialized_payload() {
        let html = r#"
            "https://gcdn.thunderstore.io/assets/paralives/paralives-cover-360x480.webp",
            "https://gcdn.thunderstore.io/assets/everything-is-crab/everything-is-crab-cover-360x480.webp",
            "https://gcdn.thunderstore.io/assets/crashout-crew/crashout-crew-cover-360x480.webp",
            "https://gcdn.thunderstore.io/assets/book-of-travels/book-of-travels-cover-360x480.webp"
        "#;

        let images = extract_community_images_from_html(html).unwrap();

        assert_eq!(
            images.get("paralives").map(String::as_str),
            Some("https://gcdn.thunderstore.io/assets/paralives/paralives-cover-360x480.webp")
        );
        assert_eq!(
            images.get("everything-is-crab").map(String::as_str),
            Some("https://gcdn.thunderstore.io/assets/everything-is-crab/everything-is-crab-cover-360x480.webp")
        );
        assert_eq!(
            images.get("crashout-crew").map(String::as_str),
            Some("https://gcdn.thunderstore.io/assets/crashout-crew/crashout-crew-cover-360x480.webp")
        );
        assert_eq!(
            images.get("book-of-travels").map(String::as_str),
            Some("https://gcdn.thunderstore.io/assets/book-of-travels/book-of-travels-cover-360x480.webp")
        );
    }

    #[test]
    fn fallback_ignores_obvious_hero_and_icon_urls() {
        let html = r#"
            "https://gcdn.thunderstore.io/live/community/sample/sample-bg-1920x620.webp",
            "https://gcdn.thunderstore.io/live/community/sample/sample-icon-192x192.webp",
            "https://gcdn.thunderstore.io/live/community/sample/sample-cover-360x480.webp"
        "#;

        let images = extract_community_images_from_html(html).unwrap();

        assert_eq!(
            images.get("sample").map(String::as_str),
            Some("https://gcdn.thunderstore.io/live/community/sample/sample-cover-360x480.webp")
        );
    }

    #[test]
    fn fallback_accepts_a_cover_with_a_nonstandard_filename() {
        let html = r#"
            "https://gcdn.thunderstore.io/assets/viva-pinata-trouble-in-paradise-recompiled/viva-pinata-tip.webp",
            "https://gcdn.thunderstore.io/assets/viva-pinata-trouble-in-paradise-recompiled/viva-pinata-tip-bg.webp"
        "#;

        let images = extract_community_images_from_html(html).unwrap();

        assert_eq!(
            images.get("viva-pinata-trouble-in-paradise-recompiled").map(String::as_str),
            Some("https://gcdn.thunderstore.io/assets/viva-pinata-trouble-in-paradise-recompiled/viva-pinata-tip.webp")
        );
    }

    #[test]
    fn fallback_accepts_the_new_community_cdn_path() {
        let html = r#"
            "https://gcdn.thunderstore.io/community/superhot-mind-control-delete/superhot-mcd.webp"
        "#;

        let images = extract_community_images_from_html(html).unwrap();

        assert_eq!(
            images.get("superhot-mind-control-delete").map(String::as_str),
            Some("https://gcdn.thunderstore.io/community/superhot-mind-control-delete/superhot-mcd.webp")
        );
    }

    #[test]
    fn validates_remote_text_content_urls() {
        assert!(validate_text_content_url(
            "https://api.github.com/repos/Zard-Studios/r2modmac/readme"
        )
        .is_ok());
        assert!(validate_text_content_url(
            "https://thunderstore.io/api/cyberstorm/package/owner/mod/v/1.0.0/readme/"
        )
        .is_ok());

        for blocked in [
            "http://api.github.com/repos/example/example",
            "https://127.0.0.1/private",
            "https://localhost/private",
            "https://api.github.com.evil.example/private",
            "https://api.github.com:444/private",
        ] {
            assert!(validate_text_content_url(blocked).is_err(), "{}", blocked);
        }
    }

    #[test]
    fn retrieves_username() {
        let username = get_username();
        assert!(username.is_ok());
        let name = username.unwrap();
        assert!(!name.trim().is_empty());
    }
}
