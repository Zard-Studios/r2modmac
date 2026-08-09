use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const METADATA_VERSION: u32 = 1;
const MAX_DOWNLOAD_ATTEMPTS: usize = 5;
pub const DOWNLOAD_CANCELLED: &str = "MOD_OPERATION_CANCELLED";

static DOWNLOAD_LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DownloadMetadata {
    schema_version: u32,
    #[serde(default)]
    generation: u64,
    url: String,
    mod_name: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    etag: Option<String>,
    last_modified: Option<String>,
    complete: bool,
    sha256: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: f64,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct CompletedDownload {
    pub payload_path: PathBuf,
    pub metadata_path: PathBuf,
}

impl CompletedDownload {
    pub fn cleanup(self) {
        let _ = fs::remove_file(self.payload_path);
        remove_metadata_files(&self.metadata_path);
    }
}

fn cache_key(url: &str, mod_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.update([0]);
    hasher.update(mod_name.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn cache_paths(cache_dir: &Path, url: &str, mod_name: &str) -> (PathBuf, PathBuf, String) {
    let key = cache_key(url, mod_name);
    let downloads_dir = cache_dir.join("mod-downloads");
    (
        downloads_dir.join(format!("{}.part", key)),
        downloads_dir.join(format!("{}.json", key)),
        key,
    )
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// `hash_file` on a blocking thread.
///
/// Digesting a 200 MB mod takes a few hundred milliseconds of solid CPU. Run on
/// the async runtime it holds a worker for that whole time, and with ten
/// installs in flight — the frontend's own limit — every worker can be busy
/// hashing at once, leaving nothing to service IPC or to keep the other
/// downloads reading. That is the freeze.
async fn hash_file_async(path: PathBuf) -> Result<String, String> {
    tokio::task::spawn_blocking(move || hash_file(&path))
        .await
        .map_err(|e| format!("Hashing task failed: {}", e))?
}

fn metadata_slots(path: &Path) -> [PathBuf; 2] {
    [path.with_extension("json.0"), path.with_extension("json.1")]
}

fn remove_metadata_files(path: &Path) {
    let _ = fs::remove_file(path);
    for slot in metadata_slots(path) {
        let _ = fs::remove_file(&slot);
        let _ = fs::remove_file(slot.with_extension("tmp"));
    }
}

fn save_metadata(path: &Path, metadata: &DownloadMetadata) -> Result<(), String> {
    let next_generation = load_metadata(path)
        .map(|stored| stored.generation.saturating_add(1))
        .unwrap_or(1);
    let mut next = metadata.clone();
    next.generation = next_generation;
    let slots = metadata_slots(path);
    let target = &slots[(next_generation % 2) as usize];
    let data = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
    let temp_path = target.with_extension("tmp");
    fs::write(&temp_path, data).map_err(|e| e.to_string())?;
    if target.exists() {
        fs::remove_file(target).map_err(|e| e.to_string())?;
    }
    fs::rename(&temp_path, target).map_err(|e| e.to_string())
}

fn load_metadata(path: &Path) -> Option<DownloadMetadata> {
    metadata_slots(path)
        .into_iter()
        .chain(std::iter::once(path.to_path_buf()))
        .filter_map(|candidate| fs::read(candidate).ok())
        .filter_map(|data| serde_json::from_slice::<DownloadMetadata>(&data).ok())
        .max_by_key(|metadata| metadata.generation)
}

fn reset_partial(payload_path: &Path, metadata_path: &Path) -> Result<(), String> {
    if payload_path.exists() {
        fs::remove_file(payload_path).map_err(|e| e.to_string())?;
    }
    remove_metadata_files(metadata_path);
    Ok(())
}

fn parse_content_range(value: &str) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, _) = range.split_once('-')?;
    Some((start.parse().ok()?, total.parse().ok()?))
}

fn ensure_not_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        Err(DOWNLOAD_CANCELLED.to_string())
    } else {
        Ok(())
    }
}

async fn wait_until_cancelled(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub async fn download_persistent<F>(
    client: &reqwest::Client,
    cache_dir: &Path,
    url: &str,
    mod_name: &str,
    cancelled: &AtomicBool,
    progress: F,
) -> Result<CompletedDownload, String>
where
    F: FnMut(DownloadProgress),
{
    download_persistent_with_attempts(
        client,
        cache_dir,
        url,
        mod_name,
        cancelled,
        MAX_DOWNLOAD_ATTEMPTS,
        progress,
    )
    .await
}

async fn download_persistent_with_attempts<F>(
    client: &reqwest::Client,
    cache_dir: &Path,
    url: &str,
    mod_name: &str,
    cancelled: &AtomicBool,
    max_attempts: usize,
    mut progress: F,
) -> Result<CompletedDownload, String>
where
    F: FnMut(DownloadProgress),
{
    let (payload_path, metadata_path, key) = cache_paths(cache_dir, url, mod_name);
    let download_lock = {
        let locks = DOWNLOAD_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = locks
            .lock()
            .map_err(|_| "Download lock registry is unavailable".to_string())?;
        guard
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _download_guard = download_lock.lock().await;

    let parent = payload_path
        .parent()
        .ok_or_else(|| "Invalid download cache path".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let mut metadata = load_metadata(&metadata_path).filter(|stored| {
        stored.schema_version == METADATA_VERSION
            && stored.url == url
            && stored.mod_name == mod_name
    });
    if metadata.is_none() && (payload_path.exists() || metadata_path.exists()) {
        reset_partial(&payload_path, &metadata_path)?;
    }

    if let Some(stored) = metadata.as_ref() {
        let actual_len = fs::metadata(&payload_path).map(|m| m.len()).unwrap_or(0);
        // Hoisted out of the condition so the cheap checks still short-circuit
        // before anything is digested.
        let looks_complete = stored.complete
            && stored.downloaded_bytes == actual_len
            && stored
                .total_bytes
                .map(|total| total == actual_len)
                .unwrap_or(true);
        let digest_matches = if looks_complete && stored.sha256.is_some() {
            stored.sha256.as_deref() == Some(hash_file_async(payload_path.clone()).await?.as_str())
        } else {
            false
        };
        if looks_complete && digest_matches {
            progress(DownloadProgress {
                downloaded_bytes: actual_len,
                total_bytes: stored.total_bytes,
                speed_bps: 0.0,
                done: true,
            });
            return Ok(CompletedDownload {
                payload_path,
                metadata_path,
            });
        }
        if !stored.complete && stored.total_bytes == Some(actual_len) && actual_len > 0 {
            let mut recovered = stored.clone();
            recovered.downloaded_bytes = actual_len;
            recovered.complete = true;
            recovered.sha256 = Some(hash_file_async(payload_path.clone()).await?);
            save_metadata(&metadata_path, &recovered)?;
            progress(DownloadProgress {
                downloaded_bytes: actual_len,
                total_bytes: recovered.total_bytes,
                speed_bps: 0.0,
                done: true,
            });
            return Ok(CompletedDownload {
                payload_path,
                metadata_path,
            });
        }
        if stored.complete {
            reset_partial(&payload_path, &metadata_path)?;
            metadata = None;
        }
    }

    let mut metadata = metadata.unwrap_or_else(|| DownloadMetadata {
        schema_version: METADATA_VERSION,
        generation: 0,
        url: url.to_string(),
        mod_name: mod_name.to_string(),
        downloaded_bytes: 0,
        total_bytes: None,
        etag: None,
        last_modified: None,
        complete: false,
        sha256: None,
    });
    metadata.downloaded_bytes = fs::metadata(&payload_path).map(|m| m.len()).unwrap_or(0);
    metadata.complete = false;
    metadata.sha256 = None;
    save_metadata(&metadata_path, &metadata)?;

    let started = Instant::now();
    let initial_bytes = metadata.downloaded_bytes;
    let mut last_emit = Instant::now();
    let mut last_metadata_save = Instant::now();
    let mut delay = Duration::from_millis(500);
    let mut last_error = "download did not start".to_string();

    for attempt in 1..=max_attempts.max(1) {
        ensure_not_cancelled(cancelled)?;
        let offset = fs::metadata(&payload_path).map(|m| m.len()).unwrap_or(0);
        metadata.downloaded_bytes = offset;

        let mut request = client.get(url).header(ACCEPT_ENCODING, "identity");
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={}-", offset));
            if let Some(validator) = metadata.etag.as_ref().or(metadata.last_modified.as_ref()) {
                request = request.header(IF_RANGE, validator);
            }
        }

        let mut response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("network error: {}", error);
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    continue;
                }
                break;
            }
        };

        let status = response.status();
        let mut append = false;
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            let range = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_range);
            match range {
                Some((start, total)) if start == offset => {
                    let response_etag = response
                        .headers()
                        .get(ETAG)
                        .and_then(|value| value.to_str().ok());
                    let response_modified = response
                        .headers()
                        .get(LAST_MODIFIED)
                        .and_then(|value| value.to_str().ok());
                    let validator_changed = metadata
                        .etag
                        .as_deref()
                        .zip(response_etag)
                        .map(|(old, new)| old != new)
                        .unwrap_or(false)
                        || metadata
                            .last_modified
                            .as_deref()
                            .zip(response_modified)
                            .map(|(old, new)| old != new)
                            .unwrap_or(false);
                    if validator_changed {
                        reset_partial(&payload_path, &metadata_path)?;
                        metadata.downloaded_bytes = 0;
                        metadata.total_bytes = None;
                        metadata.etag = None;
                        metadata.last_modified = None;
                        save_metadata(&metadata_path, &metadata)?;
                        last_error = "download validator changed while resuming".to_string();
                        continue;
                    }
                    append = offset > 0;
                    metadata.total_bytes = Some(total);
                }
                _ => {
                    reset_partial(&payload_path, &metadata_path)?;
                    metadata.downloaded_bytes = 0;
                    metadata.total_bytes = None;
                    save_metadata(&metadata_path, &metadata)?;
                    last_error = "server returned an invalid Content-Range".to_string();
                    continue;
                }
            }
        } else if status.is_success() {
            if offset > 0 {
                // Range unsupported or validator changed: restart only this package.
                let _ = fs::remove_file(&payload_path);
                metadata.downloaded_bytes = 0;
            }
            metadata.total_bytes = response.content_length();
        } else {
            last_error = format!("server returned {}", status);
            if attempt < max_attempts
                && (status.is_server_error()
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || status == reqwest::StatusCode::FORBIDDEN)
            {
                let wait = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(delay);
                tokio::time::sleep(wait).await;
                delay *= 2;
                continue;
            }
            break;
        }

        metadata.etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .or(metadata.etag);
        metadata.last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .or(metadata.last_modified);
        save_metadata(&metadata_path, &metadata)?;

        let mut output = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&payload_path)
            .map_err(|e| e.to_string())?;
        let mut stream_failed = None;
        loop {
            ensure_not_cancelled(cancelled)?;
            let next_chunk = tokio::select! {
                _ = wait_until_cancelled(cancelled) => {
                    output.flush().map_err(|e| e.to_string())?;
                    output.sync_data().map_err(|e| e.to_string())?;
                    metadata.downloaded_bytes = fs::metadata(&payload_path)
                        .map_err(|e| e.to_string())?
                        .len();
                    save_metadata(&metadata_path, &metadata)?;
                    return Err(DOWNLOAD_CANCELLED.to_string());
                }
                chunk = response.chunk() => chunk,
            };
            match next_chunk {
                Ok(Some(chunk)) => {
                    output.write_all(&chunk).map_err(|e| e.to_string())?;
                    metadata.downloaded_bytes += chunk.len() as u64;
                    let now = Instant::now();
                    if now.duration_since(last_metadata_save) >= Duration::from_secs(1) {
                        output.flush().map_err(|e| e.to_string())?;
                        save_metadata(&metadata_path, &metadata)?;
                        last_metadata_save = now;
                    }
                    if now.duration_since(last_emit) >= Duration::from_millis(120) {
                        let elapsed = started.elapsed().as_secs_f64().max(0.001);
                        progress(DownloadProgress {
                            downloaded_bytes: metadata.downloaded_bytes,
                            total_bytes: metadata.total_bytes,
                            speed_bps: metadata.downloaded_bytes.saturating_sub(initial_bytes)
                                as f64
                                / elapsed,
                            done: false,
                        });
                        last_emit = now;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    stream_failed = Some(error.to_string());
                    break;
                }
            }
        }
        output.flush().map_err(|e| e.to_string())?;
        output.sync_data().map_err(|e| e.to_string())?;
        metadata.downloaded_bytes = fs::metadata(&payload_path)
            .map_err(|e| e.to_string())?
            .len();
        save_metadata(&metadata_path, &metadata)?;

        if let Some(error) = stream_failed {
            last_error = format!(
                "stream interrupted after {} bytes: {}",
                metadata.downloaded_bytes, error
            );
            if attempt < max_attempts {
                tokio::time::sleep(delay).await;
                delay *= 2;
                continue;
            }
            break;
        }
        if let Some(total) = metadata.total_bytes {
            if metadata.downloaded_bytes != total {
                last_error = format!(
                    "incomplete response: received {} of {} bytes",
                    metadata.downloaded_bytes, total
                );
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    continue;
                }
                break;
            }
        }

        metadata.complete = true;
        metadata.sha256 = Some(hash_file_async(payload_path.clone()).await?);
        save_metadata(&metadata_path, &metadata)?;
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        progress(DownloadProgress {
            downloaded_bytes: metadata.downloaded_bytes,
            total_bytes: metadata.total_bytes,
            speed_bps: metadata.downloaded_bytes.saturating_sub(initial_bytes) as f64 / elapsed,
            done: true,
        });
        return Ok(CompletedDownload {
            payload_path,
            metadata_path,
        });
    }

    Err(format!(
        "Download failed for {} after {} attempt(s); partial data was kept for resume: {}",
        mod_name,
        max_attempts.max(1),
        last_error
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_cache(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("r2modmac-{}-{}", label, nonce))
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8_lossy(&bytes).to_string()
    }

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn resumes_persisted_partial_with_range_and_if_range() {
        let payload = b"abcdefghijklmnopqrstuvwxyz".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_payload = payload.clone();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let first_request = read_request(&mut first);
            assert!(!first_request.to_ascii_lowercase().contains("range:"));
            write!(
                first,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"asset-v1\"\r\nConnection: close\r\n\r\n",
                server_payload.len()
            )
            .unwrap();
            first.write_all(&server_payload[..8]).unwrap();
            first.flush().unwrap();
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            let second_request = read_request(&mut second).to_ascii_lowercase();
            assert!(second_request.contains("range: bytes=8-"));
            assert!(second_request.contains("if-range: \"asset-v1\""));
            write!(
                second,
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 8-{}/{}\r\nETag: \"asset-v1\"\r\nConnection: close\r\n\r\n",
                server_payload.len() - 8,
                server_payload.len() - 1,
                server_payload.len()
            )
            .unwrap();
            second.write_all(&server_payload[8..]).unwrap();
        });

        let cache = temp_cache("range-resume");
        let url = format!("http://{}/asset.zip", address);
        let paused = AtomicBool::new(false);
        let first = download_persistent_with_attempts(
            &test_client(),
            &cache,
            &url,
            "Author-Mod-1.0.0",
            &paused,
            1,
            |_| {},
        )
        .await;
        assert!(first.is_err());

        let completed = download_persistent_with_attempts(
            &test_client(),
            &cache,
            &url,
            "Author-Mod-1.0.0",
            &paused,
            1,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&completed.payload_path).unwrap(), payload);
        server.join().unwrap();
        let _ = fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn restarts_only_package_when_server_ignores_range() {
        let payload = b"complete-package-payload".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_payload = payload.clone();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let _ = read_request(&mut first);
            write!(
                first,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                server_payload.len()
            )
            .unwrap();
            first.write_all(&server_payload[..5]).unwrap();
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            let request = read_request(&mut second).to_ascii_lowercase();
            assert!(request.contains("range: bytes=5-"));
            // Deliberately ignore Range. The downloader must truncate, not append.
            write!(
                second,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                server_payload.len()
            )
            .unwrap();
            second.write_all(&server_payload).unwrap();
        });

        let cache = temp_cache("range-fallback");
        let url = format!("http://{}/asset.zip", address);
        let paused = AtomicBool::new(false);
        assert!(download_persistent_with_attempts(
            &test_client(),
            &cache,
            &url,
            "Author-Mod-2.0.0",
            &paused,
            1,
            |_| {},
        )
        .await
        .is_err());
        let completed = download_persistent_with_attempts(
            &test_client(),
            &cache,
            &url,
            "Author-Mod-2.0.0",
            &paused,
            1,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&completed.payload_path).unwrap(), payload);
        server.join().unwrap();
        let _ = fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn recovers_file_completed_before_final_metadata_commit() {
        let payload = b"download-finished-before-crash";
        let cache = temp_cache("metadata-recovery");
        let url = "http://127.0.0.1:9/should-not-be-requested.zip";
        let mod_name = "Author-Recovered-1.0.0";
        let (payload_path, metadata_path, _) = cache_paths(&cache, url, mod_name);
        fs::create_dir_all(payload_path.parent().unwrap()).unwrap();
        fs::write(&payload_path, payload).unwrap();
        save_metadata(
            &metadata_path,
            &DownloadMetadata {
                schema_version: METADATA_VERSION,
                generation: 0,
                url: url.to_string(),
                mod_name: mod_name.to_string(),
                downloaded_bytes: payload.len() as u64,
                total_bytes: Some(payload.len() as u64),
                etag: Some("\"immutable\"".to_string()),
                last_modified: None,
                complete: false,
                sha256: None,
            },
        )
        .unwrap();

        let completed = download_persistent_with_attempts(
            &test_client(),
            &cache,
            url,
            mod_name,
            &AtomicBool::new(false),
            1,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(completed.payload_path).unwrap(), payload);
        let recovered = load_metadata(&metadata_path).unwrap();
        assert!(recovered.complete);
        assert_eq!(recovered.sha256, Some(hash_file(&payload_path).unwrap()));
        let _ = fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn stop_interrupts_stalled_stream_and_keeps_partial_file() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 20\r\nETag: \"stalled\"\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            stream.write_all(b"first").unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(400));
        });

        let cache = temp_cache("stop");
        let url = format!("http://{}/asset.zip", address);
        let mod_name = "Author-Stopped-1.0.0";
        let cancelled = AtomicBool::new(false);
        let client = test_client();
        let download = download_persistent_with_attempts(
            &client,
            &cache,
            &url,
            mod_name,
            &cancelled,
            1,
            |_| {},
        );
        let stop = async {
            tokio::time::sleep(Duration::from_millis(80)).await;
            cancelled.store(true, Ordering::Release);
        };
        let (result, _) = tokio::join!(download, stop);
        assert_eq!(result.unwrap_err(), DOWNLOAD_CANCELLED);
        let (payload_path, metadata_path, _) = cache_paths(&cache, &url, mod_name);
        assert_eq!(fs::read(&payload_path).unwrap(), b"first");
        assert_eq!(load_metadata(&metadata_path).unwrap().downloaded_bytes, 5);
        server.join().unwrap();
        let _ = fs::remove_dir_all(cache);
    }
}
