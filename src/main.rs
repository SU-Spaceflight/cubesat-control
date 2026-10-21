//! The main entry point for the `CubeSat` satellite controller.
//!
//! This application is built on the Tokio asynchronous runtime. The `main` module
//! is responsible for initializing system-wide configurations, instantiating shared
//! state, and orchestrating the various background tasks that make up the
//! satellite's operating system.
//!
//! # System Initialization
//! 1. **Logging setup**: Initializes the `tracing` subscriber to output logs to both
//!    the standard console and rolling daily log files.
//! 2. **Hardware state**: Creates thread-safe references (`Arc<Mutex<...>>`) for
//!    hardware components like the pan and tilt servos.
//! 3. **Channel wiring**: Sets up `mpsc` (multi-producer, single-consumer) and
//!    `watch` channels to facilitate non-blocking communication between the independent
//!    subsystems (e.g., routing camera frames to the CV pipeline, or routing target
//!    data to the telemetry sender).
//! 4. **Task spawning**: Launches all primary operational loops in the background:
//!    - Hardware monitors (temperature).
//!    - The camera feed and computer vision pipeline.
//!    - The targeting computer and command arbiter.
//! 5. **Network loop**: Finally, blocks on the WebSocket client loop to maintain
//!    communication with Ground Control.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use ::config::{Config, Environment, File};
use opencv::core::Mat;
use tokio::sync;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::arbiter::{LocalCommand, spawn_command_arbiter};
use crate::config::SatelliteConfig;
use crate::control::spawn_targeting_computer;
use crate::hardware::{MockServo, spawn_camera_feed, spawn_temperature_monitor};
use crate::protocol::{TelemetryEnvelope, run_websocket_client};
use crate::vision::{CvOutput, RawFrame, spawn_cv_pipeline, spawn_targeting_telemetry_watcher};

mod arbiter;
mod commands;
mod config;
mod control;
mod hardware;
mod protocol;
mod vision;

#[tokio::main]
async fn main() {
    // Logging setup
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

    // Config setup
    // config options are defined in config.rs. Default config options are defined in config.toml. To override config values at runtime, environment variables can be used. E.g. `SAT__NETWORK__GROUND_STATION_IP="192.168.1.100" cargo run -r`
    #[allow(clippy::expect_used)]
    let settings = Config::builder()
        .add_source(File::with_name("config.toml"))
        .add_source(Environment::with_prefix("SAT").separator("__"))
        .build()
        .expect("Failed to build config");

    #[allow(clippy::expect_used)]
    let app_config: SatelliteConfig = settings
        .try_deserialize()
        .expect("Failed to deserialize config");

    // Starting
    info!("Starting mainboard");

    let start_time = Instant::now();

    let pan_servo = Arc::new(Mutex::new(MockServo::new("Pan Servo", 0, 180.0)));
    let tilt_servo = Arc::new(Mutex::new(MockServo::new("Tilt Servo", 1, 180.0)));

    // Channel setup
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

    // Task spawning

    spawn_temperature_monitor(telemetry_tx.clone(), start_time);
    spawn_targeting_telemetry_watcher(targeting_rx.clone(), telemetry_tx, start_time);

    spawn_camera_feed(camera_tx, "target.png");
    spawn_cv_pipeline(cv_rx, targeting_tx);
    spawn_targeting_computer(targeting_rx.clone(), command_tx.clone());

    spawn_command_arbiter(command_rx, pan_servo.clone(), tilt_servo.clone());

    run_websocket_client(telemetry_rx, command_tx.clone(), app_config.network).await;
}
