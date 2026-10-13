use std::sync::{Arc, Mutex};
use std::time::Instant;

use opencv::core::Mat;
use tokio::sync;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::arbiter::{LocalCommand, spawn_command_arbiter};
use crate::control::spawn_targeting_computer;
use crate::hardware::{MockServo, spawn_camera_feed, spawn_temperature_monitor};
use crate::protocol::{TelemetryEnvelope, run_websocket_client};
use crate::vision::{CvOutput, RawFrame, spawn_cv_pipeline, spawn_targeting_telemetry_watcher};

mod arbiter;
mod commands;
mod control;
mod hardware;
mod protocol;
mod vision;

#[tokio::main]
async fn main() {
    #[allow(clippy::expect_used)]
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .filename_suffix("flight_system.log")
        .build("log")
        .expect("Failed to initialize rolling file appender");

    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    let console_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_filter(console_filter);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_writer)
        .with_ansi(false)
        .with_filter(LevelFilter::DEBUG);

    let subscriber = tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer);

    #[allow(clippy::expect_used)]
    tracing::subscriber::set_global_default(subscriber)
        .expect("Unable to create tracing subscriber");

    info!("Starting mainboard");

    let start_time = Instant::now();

    let pan_servo = Arc::new(Mutex::new(MockServo::new("Pan Servo", 0, 180.0)));
    let tilt_servo = Arc::new(Mutex::new(MockServo::new("Tilt Servo", 1, 180.0)));

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

    let (command_tx, command_rx) = sync::mpsc::channel::<LocalCommand>(16);

    spawn_temperature_monitor(telemetry_tx.clone(), start_time);
    spawn_targeting_telemetry_watcher(targeting_rx.clone(), telemetry_tx, start_time);

    spawn_camera_feed(camera_tx, "target.png");
    spawn_cv_pipeline(cv_rx, targeting_tx);
    spawn_targeting_computer(targeting_rx.clone(), command_tx.clone());

    spawn_command_arbiter(command_rx, pan_servo.clone(), tilt_servo.clone());

    run_websocket_client("ws://127.0.0.1:8080", telemetry_rx, command_tx.clone()).await;
}
