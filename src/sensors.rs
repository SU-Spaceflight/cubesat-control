use std::time::{Duration, Instant};

use rand::{RngExt, rand_core::UnwrapErr, rngs::SysRng};
use tokio::{sync::mpsc, task::JoinHandle, time};
use tracing::{debug, warn};

use crate::protocol::{SensorData, TelemetryData, TelemetryEnvelope};

#[allow(clippy::unused_async)] // For testing
pub async fn temperature_sensor() -> f32 {
    UnwrapErr(SysRng).random_range(20.0..60.0)
}

pub fn spawn_temperature_monitor(
    tx: mpsc::Sender<TelemetryEnvelope>,
    start_time: Instant,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let temp = temperature_sensor().await;

            if temp < 80.0 {debug!("CPU Temp: {temp:.1}°C")} else {warn!("CPU temp high: {temp:.1}°C")}

            let msg = TelemetryEnvelope {
                uptime_ms: start_time.elapsed().as_millis(),
                data: TelemetryData::Sensor(SensorData { cpu_temp_c: temp }),
            };

            if tx.send(msg).await.is_err() {
                warn!("Telemetry channel closed, shutting down temperature monitor.");
                break;
            }
        }
    })
}
