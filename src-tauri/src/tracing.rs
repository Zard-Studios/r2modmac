//! Perfetto in-process tracing for the app.
//!
//! Producer, track-event categories and a single tracing-session manager
//! thread. `TracingSession` is `!Send`, so the session lives on its own
//! thread and is driven through a channel. All the `scoped_track_event!`
//! calls sprinkled through the app are no-ops while no session is recording.
//!
//! Two ways to record a trace:
//!   - Set `R2MODMAC_TRACE_SECS=N` at launch: recording starts on startup and
//!     is saved to `<app data>/traces/trace-<unix>.pftrace` after N seconds.
//!   - Call `start_perfetto_trace` / `stop_perfetto_trace` from the frontend.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::time::Duration;

use perfetto_sdk::heap_buffer::HeapBuffer;
use perfetto_sdk::pb_msg::{PbMsg, PbMsgWriter};
use perfetto_sdk::producer::*;
use perfetto_sdk::protos::config::data_source_config::DataSourceConfig;
use perfetto_sdk::protos::config::trace_config::{
    TraceConfig, TraceConfigBufferConfig, TraceConfigDataSource,
};
use perfetto_sdk::protos::config::track_event::track_event_config::TrackEventConfig;
use perfetto_sdk::tracing_session::TracingSession;
use perfetto_sdk::track_event::*;
use perfetto_sdk::track_event_categories;

track_event_categories! {
    pub mod r2modmac_te_categories {
        ( "hash", "SHA-256 checksumming", [] ),
        ( "install", "Install / apply / sync / inflate", [] ),
        ( "network", "Downloads and API requests", [] ),
        ( "fs", "Filesystem and archive IO", [] ),
        ( "config", "Config and manifest parsing", [] ),
    }
}

pub use perfetto_sdk::track_event::{EventContext, TrackEventDebugArg};
pub use perfetto_sdk::{scoped_track_event, track_event_instant};
pub use r2modmac_te_categories as perfetto_te_ns;

static TRACE_DIR: OnceLock<PathBuf> = OnceLock::new();
static COMMAND_TX: OnceLock<Sender<TraceCommand>> = OnceLock::new();
static INITIALIZED: AtomicBool = AtomicBool::new(false);

const ENABLED_CATEGORIES: &str = "hash,install,network,fs,config";

enum TraceCommand {
    Start(Option<u32>),
    Stop,
}

/// Initialize the in-process producer, track events and the session manager.
/// Safe to call more than once; only the first call does anything.
pub fn init(trace_dir: PathBuf) {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    let producer_args = ProducerInitArgsBuilder::new().backends(Backends::IN_PROCESS);
    Producer::init(producer_args.build());
    TrackEvent::init();
    let _ = r2modmac_te_categories::register();

    let (tx, rx) = channel::<TraceCommand>();
    let _ = TRACE_DIR.set(trace_dir);
    let _ = COMMAND_TX.set(tx.clone());
    let _ = std::thread::Builder::new()
        .name("perfetto-trace".to_string())
        .spawn(move || manager(rx, tx));
    log::info!("[perfetto] in-process tracing ready");
}

fn manager(rx: Receiver<TraceCommand>, tx: Sender<TraceCommand>) {
    let mut session: Option<TracingSession> = None;
    while let Ok(command) = rx.recv() {
        match command {
            TraceCommand::Start(duration_secs) => {
                if session.is_some() {
                    continue;
                }
                let mut new_session = match TracingSession::in_process() {
                    Ok(s) => s,
                    Err(error) => {
                        log::warn!("[perfetto] failed to create session: {error}");
                        continue;
                    }
                };
                new_session.setup(&build_session_config());
                new_session.start_blocking();
                log::info!("[perfetto] tracing started");
                session = Some(new_session);
                if let Some(seconds) = duration_secs {
                    let timer_tx = tx.clone();
                    let _ = std::thread::Builder::new()
                        .name("perfetto-trace-stop".to_string())
                        .spawn(move || {
                            std::thread::sleep(Duration::from_secs(seconds as u64));
                            let _ = timer_tx.send(TraceCommand::Stop);
                        });
                }
            }
            TraceCommand::Stop => {
                if let Some(mut active) = session.take() {
                    active.stop_blocking();
                    let Some(dir) = TRACE_DIR.get() else {
                        continue;
                    };
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let path = dir.join(format!("trace-{stamp}.pftrace"));
                    match std::fs::File::create(&path) {
                        Ok(file) => {
                            let file = std::sync::Arc::new(std::sync::Mutex::new(file));
                            active.read_trace_blocking(move |data, _end| {
                                if let Ok(mut guard) = file.lock() {
                                    let _ = std::io::Write::write_all(&mut *guard, data);
                                }
                            });
                            log::info!("[perfetto] trace saved to {}", path.display());
                        }
                        Err(error) => log::warn!(
                            "[perfetto] could not write trace to {}: {error}",
                            path.display()
                        ),
                    }
                }
            }
        }
    }
}

fn build_session_config() -> Vec<u8> {
    let writer = PbMsgWriter::new();
    let hb = HeapBuffer::new(writer.stream_writer());
    let mut msg = PbMsg::new(&writer).unwrap();
    {
        let mut cfg = TraceConfig { msg: &mut msg };
        cfg.set_buffers(|buf_cfg: &mut TraceConfigBufferConfig| {
            buf_cfg.set_size_kb(32 * 1024);
        });
        cfg.set_data_sources(|data_sources: &mut TraceConfigDataSource| {
            data_sources.set_config(|ds_cfg: &mut DataSourceConfig| {
                ds_cfg.set_name("track_event");
                ds_cfg.set_track_event_config(|te_cfg: &mut TrackEventConfig| {
                    te_cfg.set_enabled_categories(ENABLED_CATEGORIES);
                });
            });
        });
    }
    msg.finalize();
    let cfg_size = writer.stream_writer().get_written_size();
    let mut cfg_buffer = vec![0u8; cfg_size];
    hb.copy_into(&mut cfg_buffer);
    cfg_buffer
}

/// Ask the manager to start recording. `duration_secs` auto-stops the session.
/// Returns false if tracing was never initialized.
pub fn start(duration_secs: Option<u32>) -> bool {
    COMMAND_TX
        .get()
        .map(|tx| tx.send(TraceCommand::Start(duration_secs)).is_ok())
        .unwrap_or(false)
}

/// Ask the manager to stop recording and save the trace file.
pub fn stop() -> bool {
    COMMAND_TX
        .get()
        .map(|tx| tx.send(TraceCommand::Stop).is_ok())
        .unwrap_or(false)
}

/// Start a self-terminating recording from the `R2MODMAC_TRACE_SECS` env var.
pub fn auto_record_if_requested() {
    let seconds = std::env::var("R2MODMAC_TRACE_SECS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|&seconds| seconds > 0);
    if let Some(seconds) = seconds {
        log::info!("[perfetto] R2MODMAC_TRACE_SECS set: recording for {seconds}s");
        start(Some(seconds));
    }
}
