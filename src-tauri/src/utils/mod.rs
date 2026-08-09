pub mod file_ops;
#[path = "persistent_download.rs"]
pub mod legacy_persistent_download;
pub mod manifest_json;
pub mod mod_manifest;
pub mod paths;
#[path = "secure_persistent_download.rs"]
pub mod persistent_download;
pub mod volume_watcher;

/// Run a long synchronous section without starving the async runtime.
///
/// Tokio's multi-thread runtime can hand the current worker's queued tasks to
/// another thread for the duration, which is exactly what is wanted around code
/// that unpacks archives and writes files: no `'static` bound, no restructuring,
/// no moving borrowed data — the section stays where it is and simply stops
/// holding a worker hostage.
///
/// Falls back to running inline when there is no multi-thread runtime, because
/// `block_in_place` panics on a single-threaded one and a performance helper
/// must never be the thing that brings the app down.
pub fn without_starving_runtime<T>(work: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(work)
        }
        _ => work(),
    }
}

#[cfg(test)]
mod without_starving_runtime_tests {
    use super::without_starving_runtime;

    #[test]
    fn work_runs_and_its_value_comes_back_outside_any_runtime() {
        // The fallback path. `block_in_place` panics when there is no
        // multi-thread runtime, so this must never reach it.
        assert_eq!(without_starving_runtime(|| 2 + 2), 4);
    }

    #[test]
    fn work_runs_and_its_value_comes_back_on_the_multi_thread_runtime() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();
        let answer = runtime.block_on(async { without_starving_runtime(|| 2 + 2) });
        assert_eq!(answer, 4);
    }

    #[test]
    fn the_runtime_keeps_serving_other_tasks_while_the_work_runs() {
        // The whole point: a long synchronous section must not stop the tasks
        // queued beside it, which is what starved the app during installs.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();

        let served = Arc::new(AtomicBool::new(false));
        let flag = served.clone();

        runtime.block_on(async move {
            let other = tokio::spawn(async move {
                flag.store(true, Ordering::SeqCst);
            });

            without_starving_runtime(|| {
                // Long enough that a starved runtime would still be stuck here.
                std::thread::sleep(std::time::Duration::from_millis(150));
            });

            other.await.unwrap();
        });

        assert!(served.load(Ordering::SeqCst), "the queued task never ran");
    }
}
