#![allow(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    clippy::cast_possible_wrap,
    clippy::arithmetic_side_effects
)]
use std::time::Duration;

use opencv::{
    core::{Mat, Rect, Scalar, Size},
    imgcodecs, imgproc,
    prelude::*,
};
use std::sync::{Arc, Mutex};
use tokio::{sync::watch, task::JoinHandle, time};

use crate::{
    commands::{CommandEnvelope, FlightComputerCommands, GimbalCommands},
    vision::RawFrame,
};

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

// Bounce the target around a screen like a DVD logo
pub fn spawn_camera_feed(tx: watch::Sender<RawFrame>, test_image_path: &str) -> JoinHandle<()> {
    let Ok(target_mat) = imgcodecs::imread(test_image_path, imgcodecs::IMREAD_COLOR) else {
        println!("Failed to load static test image");
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

        loop {
            interval.tick().await;
            tick_counter += 0.05;

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
                println!("Failed to allocate canvas");
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

            // Move a simulate laser point around in a circle on the canvas
            let laser_x =
                150.0f64.mul_add((tick_counter * 0.8).cos(), f64::from(canvas_width) / 2.0) as i32;
            let laser_y =
                150.0f64.mul_add((tick_counter * 0.8).sin(), f64::from(canvas_height) / 2.0) as i32;

            let _ = imgproc::circle(
                &mut canvas,
                opencv::core::Point2i::new(laser_x, laser_y),
                4,
                opencv::core::Scalar::new(0.0, 0.0, 255.0, 0.0),
                -1,
                imgproc::LINE_8,
                0,
            );

            let frame = RawFrame {
                image: Arc::new(Mutex::new(canvas)),
            };

            if tx.send(frame).is_err() {
                println!("CV Pipeline disconnected, shutting down camera feed.");
                break;
            }
        }
    })
}

pub fn handle_command(
    cmd: &CommandEnvelope,
    pan_servo: &mut MockServo,
    tilt_servo: &mut MockServo,
) {
    match cmd {
        CommandEnvelope::Flight(FlightComputerCommands::YawRight) => {
            println!("Forwarded YawRight to Flight Computer");
        }
        CommandEnvelope::Gimbal(GimbalCommands::TrimPan(val)) => {
            println!("Adjusting Gimbal Pan trim by {val} degrees");
            pan_servo.apply_trim(*val);
        }
        CommandEnvelope::Gimbal(GimbalCommands::TrimTilt(val)) => {
            println!("Adjusting Gimbal Tilt trim by {val} degrees");
            tilt_servo.apply_trim(*val);
        }
        CommandEnvelope::Gimbal(GimbalCommands::SetPan(val)) => {
            println!("Setting Gimbal Pan trim to {val} degrees");
            pan_servo.set_absolute(*val);
        }
        CommandEnvelope::Gimbal(GimbalCommands::SetTilt(val)) => {
            println!("Setting Gimbal Tilt trim to {val} degrees");
            tilt_servo.set_absolute(*val);
        }
        CommandEnvelope::System(_) => {
            println!("Forwarded System command");
        }
    }
}
