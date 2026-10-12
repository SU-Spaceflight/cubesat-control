use std::time::Duration;

use opencv::imgcodecs;
use tokio::{sync::watch, task::JoinHandle, time};
use std::sync::{Arc, Mutex};

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

pub fn spawn_camera_feed(tx: watch::Sender<RawFrame>, test_image_path: &str) -> JoinHandle<()> {
    let mat = imgcodecs::imread(test_image_path, imgcodecs::IMREAD_COLOR)
        .expect("Failed to load static test image");

    let shared_frame = Arc::new(Mutex::new(mat));

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33));

        loop {
            interval.tick().await;

            let frame = RawFrame {
                image: Arc::clone(&shared_frame),
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
