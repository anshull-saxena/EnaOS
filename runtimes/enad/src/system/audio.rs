/// Audio state monitoring via PulseAudio / PipeWire.
///
/// Monitors:
///   - Default sink (output device) changes
///   - Default source (input device) changes
///   - Volume changes on the default sink
///   - Mute state changes
///   - Media playback state (via MPRIS D-Bus)
///
/// Uses pactl (PulseAudio CLI) for polling-based monitoring.
/// For PipeWire, pactl still works via pipewire-pulse compatibility.
///
/// Future: direct libpulse-binding or PipeWire D-Bus integration.

use std::sync::Arc;

use tracing::{info, warn};

use crate::bus::EventBus;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// Run the audio state watcher.
pub async fn run(bus: Arc<EventBus>) {
    info!("Audio watcher: starting");

    // Check if pactl is available.
    if !command_exists("pactl") {
        warn!("Audio watcher: pactl not found — audio monitoring disabled");
        return;
    }

    let mut last_sink = String::new();
    let mut last_source = String::new();
    let mut last_volume: f64 = -1.0;
    let mut last_muted = false;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Get default sink info.
        if let Ok((sink, volume, muted)) = get_default_sink_info().await {
            let mut changed = false;

            if sink != last_sink {
                info!("Audio: default sink changed to {sink}");
                changed = true;
            }

            if (volume - last_volume).abs() > 0.01 || muted != last_muted {
                info!("Audio: volume = {:.0}%, muted = {muted}", volume * 100.0);
                changed = true;
            }

            if changed {
                bus.publish(SystemEvent::new(
                    "audio-watcher",
                    EventKind::Audio,
                    EventPayload::AudioVolumeChanged {
                        sink_name: sink.clone(),
                        volume,
                        muted,
                    },
                ));

                last_sink = sink;
                last_volume = volume;
                last_muted = muted;
            }
        }

        // Get default source info.
        if let Ok(source) = get_default_source().await {
            if source != last_source {
                info!("Audio: default source changed to {source}");
                last_source = source.clone();

                // Also emit device change if sink changed too.
                if let Ok(sink) = get_default_sink_name().await {
                    bus.publish(SystemEvent::new(
                        "audio-watcher",
                        EventKind::Audio,
                        EventPayload::AudioDeviceChanged {
                            default_sink: sink,
                            default_source: source,
                        },
                    ));
                }
            }
        }
    }
}

/// Also monitor MPRIS media players for playback state.
pub async fn run_mpris(bus: Arc<EventBus>) {
    use zbus::Connection;

    info!("MPRIS watcher: starting");

    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            warn!("MPRIS watcher: failed to connect to session D-Bus: {e}");
            return;
        }
    };

    // List known MPRIS players.
    let mut last_players: Vec<String> = Vec::new();
    let mut last_state: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Get list of MPRIS players.
        let players = get_mpris_players(&conn).await;

        for player_name in &players {
            if let Ok((state, title, artist)) = get_mpris_state(&conn, player_name).await {
                let prev_state = last_state.get(player_name).cloned().unwrap_or_default();

                if state != prev_state || !last_players.contains(player_name) {
                    info!("MPRIS: {player_name} — {state} — {title:?} by {artist:?}");

                    bus.publish(SystemEvent::new(
                        "mpris-watcher",
                        EventKind::Audio,
                        EventPayload::MediaPlayback {
                            player: player_name.clone(),
                            state,
                            title: if title.is_empty() {
                                None
                            } else {
                                Some(title)
                            },
                            artist: if artist.is_empty() {
                                None
                            } else {
                                Some(artist)
                            },
                        },
                    ));

                    last_state.insert(player_name.clone(), state);
                }
            }
        }

        last_players = players;
    }
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn get_default_sink_name() -> Result<String, ()> {
    let output = tokio::process::Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .await
        .map_err(|_| ())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(())
    }
}

async fn get_default_source() -> Result<String, ()> {
    let output = tokio::process::Command::new("pactl")
        .args(["get-default-source"])
        .output()
        .await
        .map_err(|_| ())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(())
    }
}

async fn get_default_sink_info() -> Result<(String, f64, bool), ()> {
    let sink_name = get_default_sink_name().await?;

    let output = tokio::process::Command::new("pactl")
        .args(["list", "sinks"])
        .output()
        .await
        .map_err(|_| ())?;

    if !output.status.success() {
        return Err(());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_sink_info(&output_str, &sink_name)
}

fn parse_sink_info(output: &str, sink_name: &str) -> Result<(String, f64, bool), ()> {
    // Find the section for the default sink.
    let mut in_sink = false;
    let mut volume: f64 = 0.0;
    let mut muted = false;

    for line in output.lines() {
        if line.contains(&format!("Name: {sink_name}")) {
            in_sink = true;
            continue;
        }

        if in_sink {
            if line.starts_with("\tVolume:") {
                // Parse volume from: "Volume: front-left: 65536 / 100%"
                if let Some(pct) = line.split('/').nth(1) {
                    if let Some(num) = pct.trim().strip_suffix('%') {
                        if let Ok(v) = num.trim().parse::<f64>() {
                            volume = v / 100.0;
                        }
                    }
                }
            }

            if line.contains("Mute:") {
                muted = line.contains("yes");
            }

            // End of sink section.
            if line.starts_with("Sink #") || (line.starts_with('\t') == false && line.contains("Name:")) {
                break;
            }
        }
    }

    if in_sink {
        Ok((sink_name.to_string(), volume, muted))
    } else {
        Err(())
    }
}

// ── MPRIS ────────────────────────────────────────────────────────

async fn get_mpris_players(conn: &zbus::Connection) -> Vec<String> {
    use zbus::fdo::DBusProxy;

    let dbus = match DBusProxy::new(conn).await {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let names = match dbus.list_names().await {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };

    names
        .iter()
        .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
        .map(|n| n.strip_prefix("org.mpris.MediaPlayer2.").unwrap_or(n).to_string())
        .collect()
}

async fn get_mpris_state(
    conn: &zbus::Connection,
    player_name: &str,
) -> Result<(String, String, String), ()> {
    use zbus::zvariant::Value;

    let path = format!("/org/mpris/MediaPlayer2");
    let dest = format!("org.mpris.MediaPlayer2.{player_name}");

    // Get PlaybackStatus and Metadata via Get method.
    let msg = conn
        .call_method(
            Some(&dest),
            &path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.mpris.MediaPlayer2.Player", "PlaybackStatus"),
        )
        .await
        .map_err(|_| ())?;

    let state: Value = msg.body().deserialize().map_err(|_| ())?;
    let state_str = if let Value::Str(s) = state {
        s.as_str().to_string()
    } else {
        "unknown".to_string()
    };

    // Get metadata.
    let msg = conn
        .call_method(
            Some(&dest),
            &path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.mpris.MediaPlayer2.Player", "Metadata"),
        )
        .await
        .map_err(|_| ())?;

    let metadata: std::collections::HashMap<String, Value> =
        msg.body().deserialize().map_err(|_| ())?;

    let title = metadata
        .get("xesam:title")
        .and_then(|v| {
            if let Value::Str(s) = v {
                Some(s.as_str().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let artist = metadata
        .get("xesam:artist")
        .and_then(|v| {
            if let Value::Array(arr) = v {
                arr.iter()
                    .next()
                    .and_then(|v| {
                        if let Value::Str(s) = v {
                            Some(s.as_str().to_string())
                        } else {
                            None
                        }
                    })
            } else if let Value::Str(s) = v {
                Some(s.as_str().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    Ok((state_str, title, artist))
}
