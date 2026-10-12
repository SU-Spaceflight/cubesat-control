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
                let mut thresh = Mat::default();
                let mut contours =
                    opencv::core::Vector::<opencv::core::Vector<opencv::core::Point2i>>::new();
                let mut hierarchy = opencv::core::Vector::<opencv::core::Vec4i>::new();

                let _ = imgproc::cvt_color(
                    &*mat,
                    &mut gray,
                    imgproc::COLOR_BGR2GRAY,
                    0,
                    opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
                ); // Convert to grayscale and apply threshold
                let _ =
                    imgproc::threshold(&gray, &mut thresh, 127.0, 255.0, imgproc::THRESH_BINARY);

                // map nested relationships
                let _ = imgproc::find_contours_with_hierarchy(
                    &thresh,
                    &mut contours,
                    &mut hierarchy,
                    imgproc::RETR_TREE,
                    imgproc::CHAIN_APPROX_SIMPLE,
                    opencv::core::Point2i::new(0, 0),
                );

                for i in 0..hierarchy.len() {
                    let Ok(h) = hierarchy.get(i) else { continue };
                    let child_idx = h[2];

                    if child_idx != -1 {
                        let Ok(child_h) = hierarchy.get(child_idx as usize) else {
                            continue;
                        };

                        // Check if the contour has one child with no siblings
                        if child_h[0] == -1 && child_h[1] == -1 {
                            let grandchild_idx = child_h[2];

                            if grandchild_idx != -1 {
                                let Ok(grandchild_h) = hierarchy.get(grandchild_idx as usize)
                                else {
                                    continue;
                                };

                                // check if the child has one child with no siblings
                                if grandchild_h[0] == -1 && grandchild_h[1] == -1 {
                                    // Innermost contour found, find the middle
                                    if let Ok(innermost_contour) =
                                        contours.get(grandchild_idx as usize)
                                    {
                                        // Fit ellipse requires at least three points
                                        if innermost_contour.len() >= 5 {
                                            if let Ok(ellipse) =
                                                imgproc::fit_ellipse(&innermost_contour)
                                            {
                                                return Some(Point {
                                                    x: ellipse.center.x,
                                                    y: ellipse.center.y,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
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
