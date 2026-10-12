use std::time::Duration;

use opencv::{
    core::{Mat, Rect, Scalar},
    imgcodecs,
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
    let target_mat = imgcodecs::imread(test_image_path, imgcodecs::IMREAD_COLOR)
        .expect("Failed to load static test image");

    let target_width = target_mat.cols();
    let target_height = target_mat.rows();

    let canvas_width = 1280;
    let canvas_height = 720;

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33));

        let mut x = 0;
        let mut y = 0;
        let mut dx = 8;
        let mut dy = 8;

        loop {
            interval.tick().await;

            x += dx;
            y += dy;

            if x <= 0 || x + target_width >= canvas_width {
                dx = -dx;
                x = x.clamp(0, canvas_width - target_width);
            }
            if y <= 0 || y + target_height >= canvas_height {
                dy = -dy;
                y = y.clamp(0, canvas_height - target_height);
            }

            let mut canvas = Mat::new_rows_cols_with_default(
                canvas_height,
                canvas_width,
                opencv::core::CV_8UC3,
                Scalar::all(255.0)
            ).unwrap();

            let roi = Rect::new(x, y, target_width, target_height);
            if let Ok(mut canvas_roi) = canvas.roi_mut(roi) {
                let _ = target_mat.copy_to(&mut canvas_roi);
            }

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
