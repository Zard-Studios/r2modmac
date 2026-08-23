//! A local socket that drives the app the way the buttons do, for development.
//!
//! Compiled only into debug builds and started only when
//! `R2MODMAC_DEV_BRIDGE=1` is set, so a released binary has no listener in it
//! at all. It exists so a change can be exercised end to end — apply, launch,
//! read the runtime verdict — without a person clicking through the interface.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use tauri::AppHandle;

pub const DEFAULT_PORT: u16 = 47_836;

#[derive(Debug, PartialEq, Eq)]
pub enum DevCommand {
    Health { profile_id: String },
    Apply { profile_id: String },
    Launch { profile_id: String },
    Profiles,
}

/// Reads one request line. Kept apart from the socket so it can be tested.
pub fn parse_command(line: &str) -> Result<DevCommand, String> {
    let mut parts = line.split_whitespace();
    let verb = parts.next().unwrap_or("").to_ascii_lowercase();
    let argument = parts.next().map(|value| value.to_string());
    match (verb.as_str(), argument) {
        ("profiles", _) => Ok(DevCommand::Profiles),
        ("health", Some(profile_id)) => Ok(DevCommand::Health { profile_id }),
        ("apply", Some(profile_id)) => Ok(DevCommand::Apply { profile_id }),
        ("launch", Some(profile_id)) => Ok(DevCommand::Launch { profile_id }),
        ("health" | "apply" | "launch", None) => Err(format!("{verb} needs a profile id")),
        _ => Err(format!("unknown command: {line}")),
    }
}

fn profiles(app: &AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let path = crate::utils::paths::app_data_dir(app)
        .map_err(|error| error.to_string())?
        .join("profiles.json");
    let raw = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

fn find_profile(app: &AppHandle, profile_id: &str) -> Result<serde_json::Value, String> {
    profiles(app)?
        .into_iter()
        .find(|profile| profile["id"].as_str() == Some(profile_id))
        .ok_or_else(|| format!("no profile with id {profile_id}"))
}

async fn run(app: AppHandle, command: DevCommand) -> Result<serde_json::Value, String> {
    match command {
        DevCommand::Profiles => Ok(serde_json::Value::Array(
            profiles(&app)?
                .into_iter()
                .map(|profile| {
                    serde_json::json!({
                        "id": profile["id"],
                        "name": profile["name"],
                        "game": profile["gameIdentifier"],
                        "platform": profile["platform"],
                        "mods": profile["mods"].as_array().map(|mods| mods.len()).unwrap_or(0),
                    })
                })
                .collect(),
        )),
        DevCommand::Health { profile_id } => {
            let profile = find_profile(&app, &profile_id)?;
            let game = profile["gameIdentifier"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let platform = profile["platform"].as_str().map(|value| value.to_string());
            let health = crate::commands::game_commands::check_profile_runtime_health(
                app.clone(),
                profile_id,
                game,
                platform,
            )
            .await?;
            serde_json::to_value(health).map_err(|error| error.to_string())
        }
        DevCommand::Apply { profile_id } => {
            let profile = find_profile(&app, &profile_id)?;
            let game = profile["gameIdentifier"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            // The interface runs the analysis pass and then the finalising one.
            let analysis = crate::commands::game_commands::sync_profile_to_game(
                app.clone(),
                profile_id.clone(),
                game.clone(),
                Some(false),
                Some(false),
            )
            .await?;
            let finalized = crate::commands::game_commands::sync_profile_to_game(
                app.clone(),
                profile_id,
                game,
                Some(false),
                Some(true),
            )
            .await?;
            Ok(serde_json::json!({ "analysis": analysis, "finalized": finalized }))
        }
        DevCommand::Launch { profile_id } => {
            let profile = find_profile(&app, &profile_id)?;
            let game = profile["gameIdentifier"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let platform = profile["platform"].as_str().map(|value| value.to_string());
            crate::commands::game_commands::launch_game_with_mods(
                app.clone(),
                game,
                profile_id,
                platform,
            )
            .await?;
            Ok(serde_json::json!({ "launched": true }))
        }
    }
}

fn serve(app: AppHandle, stream: TcpStream) {
    let peer = stream.peer_addr().ok();
    let mut writer = match stream.try_clone() {
        Ok(clone) => clone,
        Err(error) => {
            log::warn!("[dev_bridge] could not answer {peer:?}: {error}");
            return;
        }
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }

    let answer = match parse_command(line.trim()) {
        Ok(command) => {
            log::info!("[dev_bridge] {command:?}");
            match tauri::async_runtime::block_on(run(app, command)) {
                Ok(value) => serde_json::json!({ "ok": true, "result": value }),
                Err(error) => serde_json::json!({ "ok": false, "error": error }),
            }
        }
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    };

    let _ = writeln!(writer, "{answer}");
}

/// Listen on localhost, one request per connection.
pub fn start(app: AppHandle) {
    if std::env::var("R2MODMAC_DEV_BRIDGE").as_deref() != Ok("1") {
        return;
    }
    let port = std::env::var("R2MODMAC_DEV_BRIDGE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);

    std::thread::Builder::new()
        .name("dev-bridge".to_string())
        .spawn(move || {
            let listener = match TcpListener::bind(address) {
                Ok(listener) => listener,
                Err(error) => {
                    log::warn!("[dev_bridge] could not listen on {address}: {error}");
                    return;
                }
            };
            log::info!("[dev_bridge] listening on {address}");
            for stream in listener.incoming().flatten() {
                serve(app.clone(), stream);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_the_bridge_answers_is_parsed() {
        assert_eq!(parse_command("profiles"), Ok(DevCommand::Profiles));
        assert_eq!(
            parse_command("health abc"),
            Ok(DevCommand::Health {
                profile_id: "abc".into()
            })
        );
        assert_eq!(
            parse_command("apply abc"),
            Ok(DevCommand::Apply {
                profile_id: "abc".into()
            })
        );
        assert_eq!(
            parse_command("LAUNCH abc"),
            Ok(DevCommand::Launch {
                profile_id: "abc".into()
            })
        );
    }

    #[test]
    fn a_verb_without_a_profile_is_refused_rather_than_guessed() {
        assert!(parse_command("apply").is_err());
        assert!(parse_command("launch").is_err());
        assert!(parse_command("health").is_err());
    }

    #[test]
    fn anything_else_is_refused() {
        for line in ["", "delete abc", "apply; rm -rf /", "profiles extra"] {
            let parsed = parse_command(line);
            assert!(
                parsed.is_err() || parsed == Ok(DevCommand::Profiles),
                "{line} was accepted as {parsed:?}"
            );
        }
    }
}
