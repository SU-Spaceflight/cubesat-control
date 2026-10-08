use std::time::{Duration, Instant};

use rand::RngExt;
use serde::{Deserialize, Serialize};
use tokio::{sync::broadcast, time};

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

#[tokio::main]
async fn main() {
    let start_time = Instant::now();
    let (tx, mut rx) = broadcast::channel::<TelemetryEnvelope>(16);

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

    loop {
        if let Ok(msg) = rx.recv().await {
            let json_out = serde_json::to_string(&msg).unwrap();
            println!("{}", json_out)
        }
    }
}
