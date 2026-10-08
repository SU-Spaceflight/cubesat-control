use serde::{Deserialize, Serialize};

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

}
