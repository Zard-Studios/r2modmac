//! End-to-end check of the in-app tracing module.
//!
//! Run with:  cargo run --example trace_verify
//! Starts a 3s recording through the real `tracing` module and asserts a
//! .pftrace file lands in the temp output dir.

use std::time::Duration;

fn main() {
    let dir = std::env::temp_dir().join(format!("r2modmac-trace-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    app_lib::tracing::init(dir.clone());
    assert!(
        app_lib::tracing::start(Some(3)),
        "start() should be accepted"
    );

    std::thread::sleep(Duration::from_millis(2500));
    std::thread::sleep(Duration::from_millis(1500));

    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".pftrace"))
        .collect();
    for f in &files {
        println!("trace file: {}", f.file_name().to_string_lossy());
    }
    assert!(
        !files.is_empty(),
        "expected at least one .pftrace after auto-stop"
    );
    println!(
        "OK: {} trace file(s) written to {}",
        files.len(),
        dir.display()
    );
}
