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
    println!("connecting to server at {connect_addr}");

    let (ws_stream, _) = connect_async(connect_addr).await.unwrap();

    println!("connected");

    let (mut sender, mut receiver) = ws_stream.split();

    let watchdog_duration = Duration::from_secs(10);
    let mut watchdog = Box::pin(sleep(watchdog_duration));

    // Handle recieving commands and pings, as well as sending telemetry.
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
                        println!("Server disconnected");
                        break;
                    }
                    _ => {}
                }
            }

            // Forward internal broadcast telemetry to the Server
            Some(telemetry) = telemetry_rx.recv() => {
                let json = serde_json::to_string(&telemetry).unwrap();
                if sender.send(Message::Text(json.into())).await.is_err() {
                    println!("Failed to send telemetry");
                    break;
                }
            }

            // If the watchdog fails
            () = &mut watchdog => {
                println!("No ping received from GCS in 10 seconds!");

                watchdog = Box::pin(sleep(watchdog_duration));
            }
        }
    }
}
