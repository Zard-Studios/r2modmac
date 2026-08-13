//! Demo: Perfetto in-process tracing on macOS.
//!
//! Run with:  cargo run --example perfetto_demo -- [output.pftrace]
//!
//! This records a short trace while simulating typical r2modmac work
//! (hashing, archive inflate, network fetch) and writes it to a file you
//! can open at https://ui.perfetto.dev

use perfetto_sdk::{
    heap_buffer::HeapBuffer,
    pb_msg::{PbMsg, PbMsgWriter},
    producer::*,
    protos::config::{
        data_source_config::DataSourceConfig,
        trace_config::{TraceConfig, TraceConfigBufferConfig, TraceConfigDataSource},
        track_event::track_event_config::TrackEventConfig,
    },
    scoped_track_event,
    tracing_session::TracingSession,
    track_event::*,
    track_event_categories, track_event_instant,
};
use std::{
    error::Error,
    fs::OpenOptions,
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};

track_event_categories! {
    pub mod r2modmac_te_ns {
        ( "hash", "SHA-256 and checksum work", [] ),
        ( "install", "Install / inflate / apply operations", [] ),
        ( "network", "Thunderstore HTTP requests", [] ),
        ( "fs", "Filesystem and archive IO", [] ),
    }
}

use r2modmac_te_ns as perfetto_te_ns;

fn simulate_hash(megabytes: u64) {
    scoped_track_event!("hash", "sha256", |ctx: &mut EventContext| {
        ctx.add_debug_arg("size_mb", TrackEventDebugArg::Int64(megabytes as i64));
    });
    std::thread::sleep(Duration::from_millis(megabytes * 2));
}

fn simulate_inflate(megabytes: u64) {
    {
        scoped_track_event!("install", "inflate_zip", |ctx: &mut EventContext| {
            ctx.add_debug_arg("size_mb", TrackEventDebugArg::Int64(megabytes as i64));
        });
        std::thread::sleep(Duration::from_millis(megabytes * 3));
    }
    {
        scoped_track_event!("fs", "copy_to_game_dir");
        std::thread::sleep(Duration::from_millis(megabytes));
    }
}

fn simulate_fetch() {
    scoped_track_event!(
        "network",
        "fetch_package_index",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg(
                "endpoint",
                TrackEventDebugArg::String("thunderstore.io/api"),
            );
        }
    );
    std::thread::sleep(Duration::from_millis(400));
}

fn build_session_config() -> Vec<u8> {
    let writer = PbMsgWriter::new();
    let hb = HeapBuffer::new(writer.stream_writer());
    let mut msg = PbMsg::new(&writer).unwrap();
    {
        let mut cfg = TraceConfig { msg: &mut msg };
        cfg.set_buffers(|buf_cfg: &mut TraceConfigBufferConfig| {
            buf_cfg.set_size_kb(16 * 1024);
        });
        cfg.set_data_sources(|data_sources: &mut TraceConfigDataSource| {
            data_sources.set_config(|ds_cfg: &mut DataSourceConfig| {
                ds_cfg.set_name("track_event");
                ds_cfg.set_track_event_config(|te_cfg: &mut TrackEventConfig| {
                    te_cfg.set_enabled_categories("hash,install,network,fs");
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

fn main() -> Result<(), Box<dyn Error>> {
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "perfetto_demo.pftrace".to_string());
    let file = Arc::new(Mutex::new(
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&output_path)?,
    ));

    let producer_args = ProducerInitArgsBuilder::new().backends(Backends::IN_PROCESS);
    Producer::init(producer_args.build());
    TrackEvent::init();
    perfetto_te_ns::register()?;

    let mut session = TracingSession::in_process()?;
    session.setup(&build_session_config());
    session.start_blocking();
    log::info!("tracing: do some work...");

    simulate_fetch();
    simulate_hash(64);
    simulate_inflate(32);
    track_event_instant!(
        "install",
        "mod_install_complete",
        |ctx: &mut EventContext| {
            ctx.add_debug_arg("mods", TrackEventDebugArg::Int64(3));
        }
    );

    session.stop_blocking();

    let file_for_write = Arc::clone(&file);
    session.read_trace_blocking(move |data, _end| {
        let mut file = file_for_write.lock().unwrap();
        file.write_all(data).expect("failed to write trace");
    });
    file.lock().unwrap().flush()?;
    println!("Trace written to {output_path}");
    println!("Open it at https://ui.perfetto.dev");
    Ok(())
}
