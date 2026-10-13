#![allow(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::cast_possible_wrap,
    clippy::arithmetic_side_effects
)]
use std::time::{Duration, Instant};

use opencv::{
    core::{Mat, Rect, Scalar, Size},
    imgcodecs, imgproc,
    prelude::*,
};
use rand::{RngExt, rand_core::UnwrapErr, rngs::SysRng};
use std::sync::{Arc, Mutex};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time,
};
use tracing::{debug, error, trace, warn};

use crate::{
    protocol::{SensorData, TelemetryData, TelemetryEnvelope},
    vision::RawFrame,
};

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

            if temp < 80.0 {
                debug!("CPU Temp: {temp:.1}°C")
            } else {
                warn!("CPU temp high: {temp:.1}°C")
            }

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

        trace!(
            "Servo: {} (CH {}) | Angle: {:.1}° | Pulse: {:.3}ms | Duty: {:.2}%",
            self.name,
            self.channel,
            self.current_angle,
            pulse_width_ms,
            duty_cycle * 100.0
        );
    }
}

// Bounce the target around a screen like a DVD logo
pub fn spawn_camera_feed(tx: watch::Sender<RawFrame>, test_image_path: &str) -> JoinHandle<()> {
    let Ok(target_mat) = imgcodecs::imread(test_image_path, imgcodecs::IMREAD_COLOR) else {
        error!("Failed to load static test image");
        return tokio::spawn(async {});
    };

    let base_width = f64::from(target_mat.cols());
    let base_height = f64::from(target_mat.rows());

    let canvas_width = 1280;
    let canvas_height = 720;

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33));

        let mut x = 0;
        let mut y = 0;
        let mut dx = 8;
        let mut dy = 8;
        let mut tick_counter: f64 = 0.0;
        let mut frame_count: u64 = 0;

        loop {
            interval.tick().await;
            tick_counter += 0.05;
            frame_count += 1;

            let scale_x = 0.5f64.mul_add(tick_counter.sin().abs(), 0.5);
            let scale_y = 0.5f64.mul_add(tick_counter.cos().abs(), 0.5);

            let current_w = (base_width * scale_x) as i32;
            let current_h = (base_height * scale_y) as i32;

            x += dx;
            y += dy;

            if x <= 0 || x + current_w >= canvas_width {
                dx = -dx;
                x = x.clamp(0, canvas_width - current_w);
            }
            if y <= 0 || y + current_h >= canvas_height {
                dy = -dy;
                y = y.clamp(0, canvas_height - current_h);
            }

            let Ok(mut canvas) = Mat::new_rows_cols_with_default(
                canvas_height,
                canvas_width,
                opencv::core::CV_8UC3,
                Scalar::all(255.0),
            ) else {
                error!("Failed to allocate canvas");
                break;
            };

            let mut squashed_mat = Mat::default();
            let _ = imgproc::resize(
                &target_mat,
                &mut squashed_mat,
                Size::new(current_w, current_h),
                0.0,
                0.0,
                imgproc::INTER_LINEAR,
            );

            let roi = Rect::new(x, y, current_w, current_h);
            if let Ok(mut canvas_roi) = canvas.roi_mut(roi) {
                let _ = squashed_mat.copy_to(&mut canvas_roi);
            }

            // Move a simulated laser point around in a circle on the canvas
            let cycle = frame_count % 300;
            #[allow(clippy::match_same_arms)]
            let laser_visible = match cycle {
                50 | 51 => false,           // 2 frame drop
                80 => false,                // 1 frame drop
                110..=140 => cycle % 4 < 2, // 2 frames on, 2 frames off
                180..=220 => false,         // 40-frame drop
                _ => true,                  // Normal visibility
            };

            if laser_visible {
                let laser_x = 150.0f64
                    .mul_add((tick_counter * 0.8).cos(), f64::from(canvas_width) / 2.0)
                    as i32;
                let laser_y = 150.0f64
                    .mul_add((tick_counter * 0.8).sin(), f64::from(canvas_height) / 2.0)
                    as i32;

                let _ = imgproc::circle(
                    &mut canvas,
                    opencv::core::Point2i::new(laser_x, laser_y),
                    4,
                    opencv::core::Scalar::new(0.0, 0.0, 255.0, 0.0),
                    -1,
                    imgproc::LINE_8,
                    0,
                );
            }

            let roi = Rect::new(x, y, current_w, current_h);
            if let Ok(mut canvas_roi) = canvas.roi_mut(roi) {
                let _ = squashed_mat.copy_to(&mut canvas_roi);
            }

            let frame = RawFrame {
                image: Arc::new(Mutex::new(canvas)),
            };

            if tx.send(frame).is_err() {
                warn!("CV Pipeline disconnected, shutting down camera feed.");
                break;
            }
        }
    })
}
