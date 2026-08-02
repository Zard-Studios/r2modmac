use super::legacy_persistent_download as legacy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

pub use super::legacy_persistent_download::{CompletedDownload, DownloadProgress, DOWNLOAD_CANCELLED};

const COMPLETED_CACHE_TTL: Duration = Duration::from_secs(30);
const COMPLETED_CACHE_MAX_ENTRIES: usize = 32;

#[derive(Clone)]
struct CachedCompletedDownload {
    completed: CompletedDownload,
    payload_len: u64,
    payload_modified: Option<SystemTime>,
    cached_at: Instant,
}

static COMPLETED_DOWNLOADS: OnceLock<Mutex<HashMap<String, CachedCompletedDownload>>> =
    OnceLock::new();

fn cache_key(url: &str, mod_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.update([0]);
    hasher.update(mod_name.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn payload_snapshot(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some((metadata.len(), metadata.modified().ok()))
}

fn prune_cache(cache: &mut HashMap<String, CachedCompletedDownload>, now: Instant) {
    cache.retain(|_, entry| {
        now.duration_since(entry.cached_at) <= COMPLETED_CACHE_TTL
            && payload_snapshot(&entry.completed.payload_path)
                .is_some_and(|(len, modified)| {
                    len == entry.payload_len && modified == entry.payload_modified
                })
    });

    while cache.len() >= COMPLETED_CACHE_MAX_ENTRIES {
        let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.cached_at)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        cache.remove(&oldest_key);
    }
}

fn get_cached(key: &str) -> Option<CompletedDownload> {
    let cache = COMPLETED_DOWNLOADS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;
    let now = Instant::now();
    prune_cache(&mut cache, now);
    cache.get(key).map(|entry| entry.completed.clone())
}

fn remember(key: String, completed: &CompletedDownload) {
    let Some((payload_len, payload_modified)) = payload_snapshot(&completed.payload_path) else {
        return;
    };
    let cache = COMPLETED_DOWNLOADS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    let now = Instant::now();
    prune_cache(&mut cache, now);
    cache.insert(
        key,
        CachedCompletedDownload {
            completed: completed.clone(),
            payload_len,
            payload_modified,
            cached_at: now,
        },
    );
}

pub async fn download_persistent<F>(
    client: &reqwest::Client,
    cache_dir: &Path,
    url: &str,
    mod_name: &str,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<CompletedDownload, String>
where
    F: FnMut(DownloadProgress),
{
    let key = cache_key(url, mod_name);
    if let Some(completed) = get_cached(&key) {
        let downloaded_bytes = std::fs::metadata(&completed.payload_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        progress(DownloadProgress {
            downloaded_bytes,
            total_bytes: Some(downloaded_bytes),
            speed_bps: 0.0,
            done: true,
        });
        return Ok(completed);
    }

    let completed = legacy::download_persistent(
        client,
        cache_dir,
        url,
        mod_name,
        cancelled,
        progress,
    )
    .await?;
    remember(key, &completed);
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cache_is_bounded_and_rejects_changed_payloads() {
        let dir = std::env::temp_dir().join(format!(
            "r2modmac-completed-download-cache-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let payload = dir.join("payload.part");
        let metadata = dir.join("payload.json");
        fs::write(&payload, b"first").unwrap();
        let completed = CompletedDownload {
            payload_path: payload.clone(),
            metadata_path: metadata,
        };
        remember("test".to_string(), &completed);
        assert!(get_cached("test").is_some());

        fs::write(&payload, b"changed-size").unwrap();
        assert!(get_cached("test").is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
