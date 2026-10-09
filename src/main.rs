#![allow(clippy::unwrap_used)] // Just for preliminary testing

use futures_util::{SinkExt, StreamExt};
use rand::{RngExt, rand_core::UnwrapErr, rngs::SysRng};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Serialize, Deserialize, Debug, Clone)] // Telemetry to "send"
struct TelemetryEnvelope {
    dest: String,
    uptime_ms: u128,
    payload: PayloadData,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Data to send
#[serde(untagged)]
enum PayloadData {
    Sensor(SensorData),
    Camera(CameraData),
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Sample sensor data for testing
struct SensorData {
    cpu_temp_c: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Pretend we've already found the target, and might know the laser position
struct CameraData {
    target_x: f32,
    target_y: f32,
    laser_x: Option<f32>,
    laser_y: Option<f32>,
}

// Fake sensors for testing
async fn temperature_sensor() -> f32 {
    UnwrapErr(SysRng).random()
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
}

async fn get_target_location() -> Option<Point> {
    if UnwrapErr(SysRng).random() {
        return Some(Point {
            x: UnwrapErr(SysRng).random(),
            y: UnwrapErr(SysRng).random(),
        });
    }
    None
}

async fn get_laser_location() -> Option<Point> {
    if UnwrapErr(SysRng).random() {
        return Some(Point {
            x: UnwrapErr(SysRng).random(),
            y: UnwrapErr(SysRng).random(),
        });
    }
    None
}

// Command chain
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum SystemCommands {}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum FlightComputerCommands {
    YawRight,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
enum GimbalCommands {
    TrimPan(f32),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "dest", content = "cmd", rename_all = "snake_case")]
enum CommandEnvelope {
    System(SystemCommands),
    Flight(FlightComputerCommands),
    Gimbal(GimbalCommands),
}

#[tokio::main]
async fn main() {
    let start_time = Instant::now();
    let (tx, mut rx) = mpsc::channel::<TelemetryEnvelope>(16);

    let tx_sensor = tx.clone();
    let tx_camera = tx.clone();

    tokio::spawn(async move {
        // Tokio task to send a random temperature on tx_sensor every second
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let temp = temperature_sensor().await;

            let msg = TelemetryEnvelope {
                dest: "telemetry_client".to_string(),
                uptime_ms: start_time.elapsed().as_millis(),
                payload: PayloadData::Sensor(SensorData { cpu_temp_c: temp }),
            };

            let _ = tx_sensor.send(msg).await;
        }
    });

    tokio::spawn(async move {
        // Send some random target info
        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            interval.tick().await;

            if let Some(target_pos) = get_target_location().await {
                let laser_pos = get_laser_location().await;

                let msg = TelemetryEnvelope {
                    dest: "targeting computer".to_string(),
                    uptime_ms: start_time.elapsed().as_millis(),
                    payload: PayloadData::Camera(CameraData {
                        target_x: target_pos.x,
                        target_y: target_pos.y,
                        laser_x: laser_pos.map(|p| p.x),
                        laser_y: laser_pos.map(|p| p.y)
                    }),
                };

                let _ = tx_camera.send(msg).await;
            }
        }
    });

    let connect_addr = "ws://127.0.0.1:8080";
    println!("connecting to server at {connect_addr}");

    let (ws_stream, _) = connect_async(connect_addr).await.unwrap();

    println!("connected");

    let (mut sender, mut receiver) = ws_stream.split();

    loop {
        tokio::select! {
            // Read incoming routed commands and keep-alives from Python
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<CommandEnvelope>(&text) {
                            Ok(cmd) => {
                                match cmd {
                                    CommandEnvelope::Flight(FlightComputerCommands::YawRight) => println!("Forwarded YawRight to Flight Computer"),
                                    CommandEnvelope::Gimbal(GimbalCommands::TrimPan(val)) => println!("Adjusting Gimbal Pan trim by {val} degrees"),
                                }
                            }
                            Err(e) => {
                                println!("Error: {e}, command: {text}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) => {
                        println!("Keep Alive");
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        println!("Server disconnected");
                        break;
                    }
                    _ => {}
                }
            }

            // Forward internal broadcast telemetry to the Server
            Some(telemetry) = rx.recv() => {
                let json = serde_json::to_string(&telemetry).unwrap();
                if sender.send(Message::Text(json.into())).await.is_err() {
                    println!("Failed to send telemetry");
                    break;
                }
            }
        }
    }
}
