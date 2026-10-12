#![allow(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::cast_possible_wrap,
    clippy::arithmetic_side_effects
)]
use std::fs;
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

#[allow(clippy::too_many_lines)]
pub fn spawn_cv_pipeline(
    mut rx: watch::Receiver<RawFrame>,
    tx: watch::Sender<CvOutput>,
) -> JoinHandle<()> {
    if cfg!(feature = "save_debug_frames") {
        let _ = fs::create_dir_all("debug_frames");
    }

    tokio::spawn(async move {
        let mut frame_counter = 0;

        let mut last_target: Option<Point> = None;
        let alpha = 0.8; // Smoothing factor
        let jump_threshold = 100.0;

        loop {
            if rx.changed().await.is_err() {
                println!("Camera feed disconnected, shutting down CV pipeline.");
                break;
            }

            let frame_wrapper = rx.borrow().clone();
            frame_counter += 1;

            let compute_frame = frame_wrapper.clone();

            let (raw_target_point, raw_laser_point) = tokio::task::spawn_blocking(move || {
                let Ok(mat) = compute_frame.image.lock() else {
                    return (None, None);
                };
                if mat.empty() {
                    return (None, None);
                }

                let mut gray = opencv::core::Mat::default();
                let mut edges = opencv::core::Mat::default();

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
                    if let Ok(contour) = contours.get(i)
                        && contour.len() >= 5
                        && let Ok(ellipse) = imgproc::fit_ellipse(&contour)
                    {
                        let area =
                            ellipse.size.width * ellipse.size.height * std::f32::consts::PI / 4.0;
                        if (100.0..100_000.0).contains(&area) {
                            valid_ellipses.push((ellipse, area));
                        }
                    }
                }

                let center_tolerance = 5.0;
                let mut found_point = None;

                for i in 0..valid_ellipses.len() {
                    for j in (i + 1)..valid_ellipses.len() {
                        let Some(&(e1, area1)) = valid_ellipses.get(i) else {
                            continue;
                        };
                        let Some(&(e2, area2)) = valid_ellipses.get(j) else {
                            continue;
                        };

                        let dx = e1.center.x - e2.center.x;
                        let dy = e1.center.y - e2.center.y;
                        let distance = dx.hypot(dy);

                        if distance < center_tolerance && (area1 - area2).abs() > 500.0 {
                            found_point = Some(Point {
                                x: f32::midpoint(e1.center.x, e2.center.x),
                                y: f32::midpoint(e1.center.y, e2.center.y),
                            });
                            break;
                        }
                    }
                    if found_point.is_some() {
                        break;
                    }
                }

                let mut hsv = opencv::core::Mat::default();
                let _ = imgproc::cvt_color(
                    &*mat,
                    &mut hsv,
                    imgproc::COLOR_BGR2HSV,
                    0,
                    opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT,
                );

                let mut mask1 = opencv::core::Mat::default();
                let mut mask2 = opencv::core::Mat::default();
                let mut laser_mask = opencv::core::Mat::default();

                let lower_red1 = opencv::core::Scalar::new(0.0, 150.0, 150.0, 0.0);
                let upper_red1 = opencv::core::Scalar::new(10.0, 255.0, 255.0, 0.0);
                let _ = opencv::core::in_range(&hsv, &lower_red1, &upper_red1, &mut mask1);

                let lower_red2 = opencv::core::Scalar::new(160.0, 150.0, 150.0, 0.0);
                let upper_red2 = opencv::core::Scalar::new(180.0, 255.0, 255.0, 0.0);
                let _ = opencv::core::in_range(&hsv, &lower_red2, &upper_red2, &mut mask2);

                let _ = opencv::core::bitwise_or(
                    &mask1,
                    &mask2,
                    &mut laser_mask,
                    &opencv::core::no_array(),
                );

                let laser_point = imgproc::moments(&laser_mask, true).map_or(None, |moments| if moments.m00 > 0.0 {
                    Some(Point {
                        x: (moments.m10 / moments.m00) as f32,
                        y: (moments.m01 / moments.m00) as f32,
                    })
                } else {
                    None
                });

                drop(mat);

                (found_point, laser_point)
            })
            .await
            .unwrap_or((None, None));

            let filtered_point = match (raw_target_point, last_target) {
                (Some(new_pt), Some(last_pt)) => {
                    let dist = (new_pt.x - last_pt.x).hypot(new_pt.y - last_pt.y);

                    if dist > jump_threshold {
                        Some(new_pt)
                    } else {
                        Some(Point {
                            x: f32::mul_add(alpha, new_pt.x - last_pt.x, last_pt.x),
                            y: f32::mul_add(alpha, new_pt.y - last_pt.y, last_pt.y),
                        })
                    }
                }
                (Some(new_pt), None) => Some(new_pt),
                (None, _) => None,
            };

            last_target = filtered_point;

            if cfg!(feature = "save_debug_frames")
                && let Ok(mut mat) = frame_wrapper.image.lock() {
                    if let Some(pt) = filtered_point {
                        let _ = imgproc::circle(
                            &mut *mat,
                            opencv::core::Point2i::new(pt.x as i32, pt.y as i32),
                            5,
                            opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0),
                            -1,
                            imgproc::LINE_8,
                            0,
                        );
                    }

                    let filename = format!("debug_frames/frame_{frame_counter:05}.jpg");
                    let _ =
                        opencv::imgcodecs::imwrite(&filename, &*mat, &opencv::core::Vector::new());
                }

            let output = CvOutput {
                target: raw_target_point,
                laser: raw_laser_point,
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
