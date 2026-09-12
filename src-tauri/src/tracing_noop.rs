//! No-op tracing backend for Windows.
//!
//! `perfetto-sdk-sys` currently conflicts with the Windows SDK headers.  Keep
//! the tracing call sites and Tauri commands available without linking
//! Perfetto into Windows builds.

use std::path::PathBuf;

#[allow(unused_macros)]
macro_rules! scoped_track_event {
    ($($tokens:tt)*) => {{}};
}

pub(crate) use scoped_track_event;

#[allow(unused_imports)]
pub(crate) use scoped_track_event as track_event_instant;

#[allow(dead_code)]
pub struct EventContext;

#[allow(dead_code)]
pub enum TrackEventDebugArg<'a> {
    String(&'a str),
    Int64(i64),
    Bool(bool),
}

#[allow(dead_code)]
pub mod perfetto_te_ns {}

pub fn init(_trace_dir: PathBuf) {}

pub fn start(_duration_secs: Option<u32>) -> bool {
    false
}

pub fn stop() -> bool {
    false
}

pub fn auto_record_if_requested() {}
