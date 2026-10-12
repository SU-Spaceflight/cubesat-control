use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use opencv::{core::Mat, imgproc, prelude::*};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time,
};

use crate::protocol::{TargetingTele, TelemetryData, TelemetryEnvelope};

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

#[derive(Clone)]
pub struct RawFrame {
    pub image: Arc<Mutex<Mat>>,
}

pub fn spawn_cv_pipeline(
    mut rx: watch::Receiver<RawFrame>,
    tx: watch::Sender<CvOutput>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if rx.changed().await.is_err() {
                println!("Camera feed disconnected, shutting down CV pipeline.");
                break;
            }

            let frame_wrapper = rx.borrow().clone();

            let target_point = tokio::task::spawn_blocking(move || {
                let mat = frame_wrapper.image.lock().unwrap();
                if mat.empty() {
                    return None;
                }

                let mut gray = Mat::default();
                let mut edges = Mat::default();

                let _ = imgproc::cvt_color(
                    &*mat,
                    &mut gray,
                    imgproc::COLOR_BGR2GRAY,
                    0,
                    opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
                );
                let _ = imgproc::canny(&gray, &mut edges, 50.0, 150.0, 3, false);

                let mut contours =
                    opencv::core::Vector::<opencv::core::Vector<opencv::core::Point2i>>::new();
                let _ = imgproc::find_contours(
                    &edges,
                    &mut contours,
                    imgproc::RETR_LIST,
                    imgproc::CHAIN_APPROX_SIMPLE,
                    opencv::core::Point2i::new(0, 0),
                );

                let mut valid_ellipses = Vec::new();
                for i in 0..contours.len() {
                    if let Ok(contour) = contours.get(i) {
                        if contour.len() >= 5 {
                            if let Ok(ellipse) = imgproc::fit_ellipse(&contour) {
                                let area =
                                    ellipse.size.width * ellipse.size.height * std::f32::consts::PI
                                        / 4.0;
                                if (100.0..100_000.0).contains(&area) {
                                    valid_ellipses.push((ellipse, area));
                                }
                            }
                        }
                    }
                }

                let center_tolerance = 5.0;

                for i in 0..valid_ellipses.len() {
                    for j in (i + 1)..valid_ellipses.len() {
                        let (e1, area1) = valid_ellipses[i];
                        let (e2, area2) = valid_ellipses[j];

                        let dx = e1.center.x - e2.center.x;
                        let dy = e1.center.y - e2.center.y;
                        let distance = (dx.powi(2) + dy.powi(2)).sqrt();

                        if distance < center_tolerance && (area1 - area2).abs() > 500.0 {
                            return Some(Point {
                                x: (e1.center.x + e2.center.x) / 2.0,
                                y: (e1.center.y + e2.center.y) / 2.0,
                            });
                        }
                    }
                }

                None
            })
            .await
            .unwrap_or(None);

            // No laser detection yet
            let output = CvOutput {
                target: target_point,
                laser: None,
            };

            let _ = tx.send(output);
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
