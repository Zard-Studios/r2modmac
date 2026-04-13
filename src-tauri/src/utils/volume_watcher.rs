use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use notify::{Event, EventKind, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

const VOLUME_EVENT_NAME: &str = "storage-volumes-changed";
const EVENT_DEBOUNCE: Duration = Duration::from_millis(500);
const WINDOWS_DRIVE_RESCAN_INTERVAL: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct WatchRoot {
    path: PathBuf,
    mode: RecursiveMode,
}

pub fn start_volume_watcher(app: AppHandle) {
    let app_handle = app.clone();

    thread::Builder::new()
        .name("storage-volume-watcher".to_string())
        .spawn(move || {
            if let Err(error) = run_volume_watcher(app_handle) {
                eprintln!("[volume_watcher] stopped: {}", error);
            }
        })
        .ok();
}

fn run_volume_watcher(app: AppHandle) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .map_err(|error| format!("Failed to start filesystem watcher: {}", error))?;

    let mut watched_roots = HashSet::new();
    refresh_watch_roots(&mut watcher, &mut watched_roots);

    if watched_roots.is_empty() {
        eprintln!("[volume_watcher] no mount roots available to watch");
    } else {
        eprintln!(
            "[volume_watcher] watching {} mount root(s)",
            watched_roots.len()
        );
    }

    loop {
        match rx.recv_timeout(WINDOWS_DRIVE_RESCAN_INTERVAL) {
            Ok(Ok(event)) => {
                if should_emit_volume_event(&event) {
                    drain_pending_volume_events(&rx);
                    let _ = app.emit(VOLUME_EVENT_NAME, ());
                    refresh_watch_roots(&mut watcher, &mut watched_roots);
                }
            }
            Ok(Err(error)) => {
                eprintln!("[volume_watcher] watch error: {}", error);
                refresh_watch_roots(&mut watcher, &mut watched_roots);
            }
            Err(RecvTimeoutError::Timeout) => {
                #[cfg(target_os = "windows")]
                {
                    let previous_roots = watched_roots.clone();
                    refresh_watch_roots(&mut watcher, &mut watched_roots);
                    if watched_roots != previous_roots {
                        let _ = app.emit(VOLUME_EVENT_NAME, ());
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Filesystem watcher channel disconnected".to_string());
            }
        }
    }
}

fn should_emit_volume_event(event: &Event) -> bool {
    !event.paths.is_empty() && !matches!(event.kind, EventKind::Access(_))
}

fn drain_pending_volume_events(rx: &mpsc::Receiver<notify::Result<Event>>) {
    loop {
        match rx.recv_timeout(EVENT_DEBOUNCE) {
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn refresh_watch_roots(
    watcher: &mut notify::RecommendedWatcher,
    watched_roots: &mut HashSet<WatchRoot>,
) {
    let current_roots = collect_watch_roots();

    let roots_to_watch = current_roots
        .difference(watched_roots)
        .cloned()
        .collect::<Vec<_>>();

    for root in roots_to_watch {
        if watcher.watch(&root.path, root.mode).is_ok() {
            watched_roots.insert(root);
        }
    }

    let stale_roots = watched_roots
        .iter()
        .filter(|root| !root.path.exists())
        .cloned()
        .collect::<Vec<_>>();

    for root in stale_roots {
        let _ = watcher.unwatch(&root.path);
        watched_roots.remove(&root);
    }
}

fn collect_watch_roots() -> HashSet<WatchRoot> {
    let mut roots = HashSet::new();

    #[cfg(target_os = "macos")]
    {
        push_root(
            &mut roots,
            PathBuf::from("/Volumes"),
            RecursiveMode::NonRecursive,
        );
    }

    #[cfg(target_os = "linux")]
    {
        push_root(
            &mut roots,
            PathBuf::from("/media"),
            RecursiveMode::Recursive,
        );
        push_root(&mut roots, PathBuf::from("/mnt"), RecursiveMode::Recursive);
        push_root(
            &mut roots,
            PathBuf::from("/run/media"),
            RecursiveMode::Recursive,
        );
    }

    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            push_root(&mut roots, root, RecursiveMode::NonRecursive);
        }
    }

    roots
}

fn push_root(roots: &mut HashSet<WatchRoot>, path: PathBuf, mode: RecursiveMode) {
    if path.is_dir() {
        roots.insert(WatchRoot { path, mode });
    }
}
