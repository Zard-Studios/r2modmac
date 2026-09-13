use serde::Serialize;
use std::sync::OnceLock;

static SESSION_ID: OnceLock<String> = OnceLock::new();

pub fn session_id() -> &'static str {
    SESSION_ID
        .get_or_init(|| uuid::Uuid::new_v4().to_string())
        .as_str()
}

pub fn operation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Serialize)]
struct AppLogRecord<'a> {
    schema: u8,
    timestamp: &'a str,
    level: &'a str,
    session_id: &'a str,
    process_id: u32,
    thread: &'a str,
    target: &'a str,
    module: Option<&'a str>,
    file: Option<&'a str>,
    line: Option<u32>,
    message: &'a str,
}

pub fn format_app_log(
    timestamp: &str,
    level: &str,
    target: &str,
    module: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
    message: &str,
) -> String {
    let current_thread = std::thread::current();
    let thread = current_thread.name().unwrap_or("unnamed");
    serde_json::to_string(&AppLogRecord {
        schema: 1,
        timestamp,
        level,
        session_id: session_id(),
        process_id: std::process::id(),
        thread,
        target,
        module,
        file,
        line,
        message,
    })
    .unwrap_or_else(|_| {
        format!("{{\"schema\":1,\"level\":\"ERROR\",\"message\":\"log serialization failed\"}}")
    })
}

#[cfg(test)]
mod tests {
    use super::{format_app_log, session_id};

    #[test]
    fn app_log_is_one_structured_json_record_with_diagnostic_context() {
        let line = format_app_log(
            "2026-09-13T15:04:05.123+02:00",
            "DEBUG",
            "app_lib::commands",
            Some("app_lib::commands"),
            Some("src/commands.rs"),
            Some(42),
            "first line\nsecond line",
        );
        let record: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert_eq!(record["schema"], 1);
        assert_eq!(record["session_id"], session_id());
        assert_eq!(record["level"], "DEBUG");
        assert_eq!(record["file"], "src/commands.rs");
        assert_eq!(record["line"], 42);
        assert_eq!(record["message"], "first line\nsecond line");
        assert!(!line.contains('\n'), "one event must stay on one log line");
    }

    #[test]
    fn operation_ids_are_unique_and_parseable() {
        let first = super::operation_id();
        let second = super::operation_id();

        assert_ne!(first, second);
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        assert!(uuid::Uuid::parse_str(&second).is_ok());
    }
}
