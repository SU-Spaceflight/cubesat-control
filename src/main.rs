// #![allow(clippy::unwrap_used)] // Just for preliminary testing
// #![allow(clippy::expect_used)]

use std::sync::{Arc, Mutex};
use std::time::Instant;

use opencv::core::Mat;
use tokio::sync;

use crate::hardware::{MockServo, spawn_camera_feed};
use crate::protocol::{TelemetryEnvelope, run_websocket_client};
use crate::sensors::spawn_temperature_monitor;
use crate::vision::{
    CvOutput, RawFrame, spawn_cv_pipeline, spawn_targeting_computer,
    spawn_targeting_telemetry_watcher,
};

mod commands;
mod control;
mod hardware;
mod protocol;
mod sensors;
mod vision;

#[tokio::main]
async fn main() {
    let start_time = Instant::now();

    let pan_servo = MockServo::new("Pan Servo", 0, 180.0);
    let tilt_servo = MockServo::new("Tilt Servo", 1, 180.0);

    let (telemetry_tx, telemetry_rx) = sync::mpsc::channel::<TelemetryEnvelope>(16);

    let initial_mat = Mat::default();
    let initial_frame = RawFrame {
        image: Arc::new(Mutex::new(initial_mat)),
    };

    let (camera_tx, cv_rx) = sync::watch::channel::<RawFrame>(initial_frame);
    let (targeting_tx, targeting_rx) = sync::watch::channel(CvOutput {
        target: None,
        laser: None,
    });

    spawn_temperature_monitor(telemetry_tx.clone(), start_time);

    spawn_camera_feed(camera_tx, "target.png");
    spawn_cv_pipeline(cv_rx, targeting_tx);
    spawn_targeting_computer(targeting_rx.clone());

    spawn_targeting_telemetry_watcher(targeting_rx, telemetry_tx, start_time);

    run_websocket_client("ws://127.0.0.1:8080", telemetry_rx, pan_servo, tilt_servo).await;
}
