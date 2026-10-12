#![allow(clippy::unwrap_used)] // Just for preliminary testing
#![allow(clippy::expect_used)]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    commands::CommandEnvelope,
    hardware::{MockServo, handle_command},
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
    mut pan_servo: MockServo,
    mut tilt_servo: MockServo,
) {
    let reconnect_delay = Duration::from_secs(3);
    let watchdog_duration = Duration::from_secs(10);

    loop {
        println!("Attempting to connect to GCS server at {connect_addr}");

        // Attempt to connect
        let ws_stream = match connect_async(connect_addr).await {
            Ok((stream, _)) => stream,
            Err(e) => {
                println!("Connection failed: {e}. Retrying in {reconnect_delay:?}");
                sleep(reconnect_delay).await;
                continue;
            }
        };

        println!("Connected to GCS!");
        let (mut sender, mut receiver) = ws_stream.split();
        let mut watchdog = Box::pin(sleep(watchdog_duration));

        loop {
            tokio::select! {
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<CommandEnvelope>(&text) {
                                Ok(cmd) => {
                                    handle_command(&cmd, &mut pan_servo, &mut tilt_servo);
                                }
                                Err(e) => {
                                    println!("Error: {e}, command: {text}");
                                }
                            }
                        }
                        Some(Ok(Message::Ping(_))) => {
                            watchdog = Box::pin(sleep(watchdog_duration));
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            println!("Server disconnected.");
                            break; // Break the inner loop to trigger a reconnect
                        }
                        _ => {}
                    }
                }

                // Forward internal broadcast telemetry to the Server
                telemetry_opt = telemetry_rx.recv() => {
                    match telemetry_opt {
                        Some(telemetry) => {
                            let json = serde_json::to_string(&telemetry).unwrap();
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                println!("Failed to send telemetry. Connection likely lost.");
                                break; // Break the inner loop to trigger a reconnect
                            }
                        }
                        None => {
                            // If the internal mpsc channel closes, the main app is shutting down
                            println!("Internal telemetry channel closed. Shutting down client.");
                            return;
                        }
                    }
                }

                // If the watchdog fails
                () = &mut watchdog => {
                    println!("No ping received from GCS in 10 seconds!");
                    // Reset watchdog so it doesn't spam, or break here to force a TCP reconnect
                    watchdog = Box::pin(sleep(watchdog_duration));
                }
            }
        }

        // Wait briefly before attempting to reconnect
        sleep(reconnect_delay).await;
    }
}
