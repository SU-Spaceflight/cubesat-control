use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{self, sleep},
};

use crate::protocol::{CameraData, TargetingTele, TelemetryData, TelemetryEnvelope};

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub struct CvOutput {
    pub target: Option<Point>,
    pub laser: Option<Point>,
}
impl From<CameraData> for CvOutput {
    fn from(value: CameraData) -> Self {
        Self {
            target: value.target,
            laser: value.laser,
        }
    }
}

pub fn spawn_cv_pipeline(
    mut rx: watch::Receiver<CameraData>,
    tx: watch::Sender<CvOutput>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if rx.changed().await.is_err() {
                break;
            }

            let latest_frame = rx.borrow().clone();

            sleep(Duration::from_secs(1)).await;

            let _ = tx.send(latest_frame.into());
        }
    })
}

pub fn spawn_targeting_computer(mut rx: watch::Receiver<CvOutput>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            println!("{:?}", *rx.borrow_and_update());
            if rx.changed().await.is_err() {
                println!("Error in targeting computer input channel");
                break;
            }
        }
    })
}

pub fn spawn_targeting_telemetry_watcher(
    rx: watch::Receiver<CvOutput>,
    tx: mpsc::Sender<TelemetryEnvelope>,
    start_time: Instant,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            let latest_cv_state = *rx.borrow();

            let msg = TelemetryEnvelope {
                uptime_ms: start_time.elapsed().as_millis(),
                data: TelemetryData::Tele(TargetingTele {
                    target: latest_cv_state.target,
                    laser: latest_cv_state.laser,
                }),
            };

            if tx.send(msg).await.is_err() {
                println!("Failed to send watch telemetry");
                break;
            }
        }
    })
}
