#![allow(clippy::unwrap_used)] // Just for preliminary testing

use std::time::{Duration, Instant};

use futures_util::{StreamExt, SinkExt};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use tokio::{net::{TcpListener, TcpStream}, sync::broadcast, time};
use tokio_tungstenite::tungstenite::Message;

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

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CommandEnvelope {
    dest: String,
    cmd: String,
    val: Option<f32>,
}

#[tokio::main]
async fn main() {
    let start_time = Instant::now();
    let (tx, _) = broadcast::channel::<TelemetryEnvelope>(16);

    let tx_sensor = tx.clone();
    let tx_camera = tx.clone();

    tokio::spawn(async move {
        // Tokio task to send a random temperature on tx_sensor every second
        let mut interval = time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;
            let mut rng = rand::rng();

            let temp = rng.random_range(30.0..100.0);

            let msg = TelemetryEnvelope {
                dest: "telemetry_client".to_string(),
                uptime_ms: start_time.elapsed().as_millis(),
                payload: PayloadData::Sensor(SensorData { cpu_temp_c: temp }),
            };

            let _ = tx_sensor.send(msg);
        }
    });

    tokio::spawn(async move {
        // Send some random target info
        let mut interval = time::interval(Duration::from_millis(33));

        loop {
            interval.tick().await;
            let mut rng = rand::rng();

            let laser_found: bool = rng.random();

            let msg = TelemetryEnvelope {
                dest: "targeting computer".to_string(),
                uptime_ms: start_time.elapsed().as_millis(),
                payload: PayloadData::Camera(CameraData {
                    target_x: rng.random_range(0.0..40.0),
                    target_y: rng.random_range(0.0..40.0),
                    laser_x: if laser_found {
                        Some(rng.random_range(0.0..40.0))
                    } else {
                        None
                    },
                    laser_y: if laser_found {
                        Some(rng.random_range(0.0..40.0))
                    } else {
                        None
                    },
                }),
            };

            let _ = tx_camera.send(msg);
        }
    });

    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap(); // Listen to ip and port
    println!("listening at ws://127.0.0.1:8080");
    while let Ok((stream, addr)) = listener.accept().await { // wait for a connection
        println!("new connection from {addr}");
        let tx_clone = tx.clone();

        tokio::spawn(handle_client(stream, tx_clone)); // start the handler
    }
}

async fn handle_client(stream: TcpStream, tx: broadcast::Sender<TelemetryEnvelope>) {
    let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

    let (mut sender, mut reciever) = ws_stream.split();
    let mut rx = tx.subscribe();

    loop {
        tokio::select! {
            msg = reciever.next() => {
                match msg { // Check the different message types
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<CommandEnvelope>(&text) {
                            Ok(cmd) => println!("{cmd:#?}"),
                            Err(e) => {
                                println!("failed to parse, error: {e}, data: {text}")
                            }
                        }
                    }
                    Some(Ok(Message::Ping(_))) => {
                        println!("Keep Alive");
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("Client Disconnected");
                        break
                    }
                    _ => {}
                }
            }

            Ok(telemetry) = rx.recv() => { // forward internal telemetry over the websocket
                let json = serde_json::to_string(&telemetry).unwrap();
                if sender.send(Message::Text(json.into())).await.is_err() {
                    eprintln!("Failed to print telemetry");
                }
            }
        }
    }
}
