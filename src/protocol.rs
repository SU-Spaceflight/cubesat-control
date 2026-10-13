use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc,
    time::{interval, sleep},
};
use tokio_tungstenite::{connect_async, tungstenite::{Message, Bytes}};
use tracing::{debug, error, info, warn};

use crate::{
    arbiter::LocalCommand,
    commands::{CommandEnvelope, GimbalCommands},
    vision::Point,
};

#[derive(Serialize, Deserialize, Debug, Clone)] // Telemetry to send
pub struct TelemetryEnvelope {
    pub uptime_ms: u128,
    #[serde(flatten)]
    pub data: TelemetryData,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Data to send
#[serde(tag = "dest", content = "payload", rename_all = "snake_case")]
pub enum TelemetryData {
    Sensor(SensorData),
    Camera(CameraData),
    Tele(TargetingTele),
    System(SystemData),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct SensorData {
    pub cpu_temp_c: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct SystemData {
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct TargetingTele {
    pub target: Option<Point>,
    pub laser: Option<Point>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct CameraData {
    pub target: Option<Point>,
    pub laser: Option<Point>,
}

pub async fn run_websocket_client(
    connect_addr: &str,
    mut telemetry_rx: mpsc::Receiver<TelemetryEnvelope>,
    command_tx: mpsc::Sender<LocalCommand>,
) {
    let reconnect_delay = Duration::from_secs(3);
    let watchdog_duration = Duration::from_secs(10);
    let mut ping_interval = interval(Duration::from_secs(3));

    loop {
        info!("Attempting to connect to GCS server at {connect_addr}");

        // Attempt to connect
        let ws_stream = match connect_async(connect_addr).await {
            Ok((stream, _)) => stream,
            Err(e) => {
                warn!("Connection failed: {e}. Retrying in {reconnect_delay:?}");
                sleep(reconnect_delay).await;
                continue;
            }
        };

        info!("Connected to GCS!");
        let (mut sender, mut receiver) = ws_stream.split();
        let mut watchdog = Box::pin(sleep(watchdog_duration));

        loop {
            tokio::select! {
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<CommandEnvelope>(&text) {
                                Ok(cmd) => {
                                    handle_command(cmd, &command_tx).await;
                                }
                                Err(e) => {
                                    error!("Error: {e}, command: {text}");
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            warn!("Server disconnected.");
                            break; // Break the inner loop to trigger a reconnect
                        }
                        Some(Ok(Message::Pong(_))) => {
                            info!("Pong recieved, watchdog reset");
                            watchdog = Box::pin(sleep(watchdog_duration));
                        }
                        _ => {}
                    }
                }

                // Forward internal broadcast telemetry to the Server
                telemetry_opt = telemetry_rx.recv() => {
                    if let Some(telemetry) = telemetry_opt {
                        let json = serde_json::to_string(&telemetry).unwrap();
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            warn!("Failed to send telemetry. Connection likely lost.");
                            break; // Break the inner loop to trigger a reconnect
                        }
                    } else {
                        // If the internal mpsc channel closes, the main app is shutting down
                        warn!("Internal telemetry channel closed. Shutting down client.");
                        return;
                    }
                }

                _ = ping_interval.tick() => {
                    if let Err(e) = sender.send(Message::Ping(Bytes::from(""))).await {
                        println!("Failed to send ping: {}", e);
                        break;
                    }
                }


                // If the watchdog fails
                () = &mut watchdog => {
                    warn!("No ping received from GCS in 10 seconds!");
                    // Reset watchdog so it doesn't spam, or break here to force a TCP reconnect
                    watchdog = Box::pin(sleep(watchdog_duration));
                }
            }
        }

        // Wait briefly before attempting to reconnect
        sleep(reconnect_delay).await;
    }
}

pub async fn handle_command(command: CommandEnvelope, arbiter_tx: &mpsc::Sender<LocalCommand>) {
    match command {
        CommandEnvelope::Gimbal(GimbalCommands::ManualOverride { pan, tilt }) => {
            if let Err(e) = arbiter_tx
                .send(LocalCommand::ManualOverride { pan, tilt })
                .await
            {
                error!("Failed to forward manual override to Arbiter: {e}");
            } else {
                debug!("Forwarded Manual Override ({pan:.1}, {tilt:.1}) to Arbiter");
            }
        }

        CommandEnvelope::Gimbal(GimbalCommands::SetTrackingMode(enabled)) => {
            if let Err(e) = arbiter_tx
                .send(LocalCommand::SetTrackingMode(enabled))
                .await
            {
                error!("Failed to forward tracking mode to Arbiter: {e}");
            } else {
                info!("Forwarded Tracking Mode toggle ({enabled}) to Arbiter");
            }
        }

        CommandEnvelope::Gimbal(_) => {
            warn!("Received unhandled Gimbal command");
        }

        _ => {
            debug!("Handled non-gimbal command");
        }
    }
}
