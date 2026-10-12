#![allow(clippy::unwrap_used)] // Just for preliminary testing
#![allow(clippy::too_many_lines)]

use futures_util::{SinkExt, StreamExt};
use rand::{RngExt, rand_core::UnwrapErr, rngs::SysRng};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::{self};
use tokio::time::{self, sleep};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Serialize, Deserialize, Debug, Clone)] // Telemetry to send
struct TelemetryEnvelope {
    uptime_ms: u128,
    #[serde(flatten)]
    data: TelemetryData,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Data to send
#[serde(tag = "dest", content = "payload", rename_all = "snake_case")]
enum TelemetryData {
    Sensor(SensorData),
    Camera(CameraData),
    Tele(TargetingTele),
    System(SystemData),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
struct SensorData {
    cpu_temp_c: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
struct SystemData {
    status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
struct TargetingTele {
    target: Option<Point>,
    laser: Option<Point>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
struct CameraData {
    target: Option<Point>,
    laser: Option<Point>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
struct CVOutput {
    target: Option<Point>,
    laser: Option<Point>,
}
impl From<CameraData> for CVOutput {
    fn from(value: CameraData) -> Self {
        Self {
            target: value.target,
            laser: value.laser,
        }
    }
}

// Fake sensors for testing
#[allow(clippy::unused_async)] // For testing
async fn temperature_sensor() -> f32 {
    UnwrapErr(SysRng).random()
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct Point {
    x: f32,
    y: f32,
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
    TrimTilt(f32),
    SetPan(f32),
    SetTilt(f32),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "dest", content = "cmd", rename_all = "snake_case")]
enum CommandEnvelope {
    System(SystemCommands),
    Flight(FlightComputerCommands),
    Gimbal(GimbalCommands),
}

// Servos

pub struct MockServo {
    name: String,
    channel: u8,
    current_angle: f32,
    max_angle: f32,
}

impl MockServo {
    #[must_use]
    pub fn new(name: &str, channel: u8, max_angle: f32) -> Self {
        Self {
            name: name.to_string(),
            channel,
            current_angle: 90.0,
            max_angle,
        }
    }

    pub fn apply_trim(&mut self, trim_val: f32) {
        let new_angle = self.current_angle + trim_val;
        self.set_absolute(new_angle);
    }

    pub fn set_absolute(&mut self, angle: f32) {
        self.current_angle = angle.clamp(0.0, self.max_angle);
        self.update_hardware();
    }

    fn update_hardware(&self) {
        let pulse_width_ms = 1.0 + (self.current_angle / self.max_angle);
        let duty_cycle = pulse_width_ms / 20.0;

        println!(
            "Servo: {} (CH {}) | Angle: {:.1}° | Pulse: {:.3}ms | Duty: {:.2}%",
            self.name,
            self.channel,
            self.current_angle,
            pulse_width_ms,
            duty_cycle * 100.0
        );
    }
}

#[tokio::main]
async fn main() {
    let start_time = Instant::now();
    let (telemetry_tx, mut telemetry_rx) = sync::mpsc::channel::<TelemetryEnvelope>(16);
    let (targeting_tx, mut targeting_rx) = sync::watch::channel(CVOutput {
        target: None,
        laser: None,
    });

    let tx_sensor = telemetry_tx.clone();
    let state_rx = targeting_rx.clone();

    // Random temperature telemetry
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let temp = temperature_sensor().await;

            let msg = TelemetryEnvelope {
                uptime_ms: start_time.elapsed().as_millis(),
                data: TelemetryData::Sensor(SensorData { cpu_temp_c: temp }),
            };

            let _ = tx_sensor.send(msg).await;
        }
    });

    let (camera_tx, mut cv_rx) = tokio::sync::watch::channel(CameraData {
        target: None,
        laser: None,
    });

    // Push "frames" at 30 FPS to channel. Currently just random laser and target data, but in future will have fake or real camera feed.
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33));

        loop {
            interval.tick().await;

            let target = if UnwrapErr(SysRng).random() {
                Some(Point {
                    x: UnwrapErr(SysRng).random(),
                    y: UnwrapErr(SysRng).random(),
                })
            } else {
                None
            };

            let laser = if UnwrapErr(SysRng).random() {
                Some(Point {
                    x: UnwrapErr(SysRng).random(),
                    y: UnwrapErr(SysRng).random(),
                })
            } else {
                None
            };

            let frame = CameraData { target, laser };

            let _ = camera_tx.send(frame);
        }
    });

    // Pretend to run CV on the camera frame, but really just forwards the data.
    tokio::spawn(async move {
        loop {
            if cv_rx.changed().await.is_err() {
                break;
            }

            let latest_frame = cv_rx.borrow().clone();

            sleep(Duration::from_secs(1)).await;

            let _ = targeting_tx.send(latest_frame.into());
        }
    });

    // Targeting computer. Currently just prints the currect coordinates.
    tokio::spawn(async move {
        loop {
            println!("{:?}", *targeting_rx.borrow_and_update());
            if targeting_rx.changed().await.is_err() {
                println!("Error in targeting computer input channel");
                break;
            }
        }
    });

    let mut pan_servo = MockServo::new("Pan Servo", 0, 180.0);
    let mut tilt_servo = MockServo::new("Tilt Servo", 1, 180.0);

    // telemtry and command handling
    let connect_addr = "ws://127.0.0.1:8080";
    println!("connecting to server at {connect_addr}");

    let (ws_stream, _) = connect_async(connect_addr).await.unwrap();

    println!("connected");

    let (mut sender, mut receiver) = ws_stream.split();

    let mut watch_ticker = tokio::time::interval(std::time::Duration::from_millis(1_000));
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
                                match cmd {
                                    CommandEnvelope::Flight(FlightComputerCommands::YawRight) => println!("Forwarded YawRight to Flight Computer"),
                                    CommandEnvelope::Gimbal(GimbalCommands::TrimPan(val)) => {
                                        println!("Adjusting Gimbal Pan trim by {val} degrees");
                                        pan_servo.apply_trim(val);
                                    },
                                    CommandEnvelope::Gimbal(GimbalCommands::TrimTilt(val)) => {
                                        println!("Adjusting Gimbal Tilt trim by {val} degrees");
                                        tilt_servo.apply_trim(val);
                                    }
                                    CommandEnvelope::Gimbal(GimbalCommands::SetPan(val)) => {
                                        println!("Setting Gimbal Pan trim to {val} degrees");
                                        pan_servo.set_absolute(val);
                                    },
                                    CommandEnvelope::Gimbal(GimbalCommands::SetTilt(val)) => {
                                        println!("Setting Gimbal Tilt trim to {val} degrees");
                                        tilt_servo.set_absolute(val);
                                    },
                                }
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

            // Peak at the targeting computer and send the data off.
            _ = watch_ticker.tick() => {
                let latest_cv_state = *state_rx.borrow();

                let msg = TelemetryEnvelope {
                    uptime_ms: start_time.elapsed().as_millis(),
                    data: TelemetryData::Tele( TargetingTele { target: latest_cv_state.target, laser: latest_cv_state.laser }),
                };

                let json = serde_json::to_string(&msg).unwrap();

                if sender.send(Message::Text(json.into())).await.is_err() {
                    println!("Failed to send watch telemetry");
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
