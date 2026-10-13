use crate::hardware::MockServo;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info};

#[derive(Debug, Clone, Copy)]
pub enum LocalCommand {
    ManualOverride { pan: f32, tilt: f32 },
    AutoTrim { pan_delta: f32, tilt_delta: f32 },
    SetTrackingMode(bool),
}

pub fn spawn_command_arbiter(
    mut rx: mpsc::Receiver<LocalCommand>,
    pan_servo: Arc<Mutex<MockServo>>,
    tilt_servo: Arc<Mutex<MockServo>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        const MAX_ANGLE: f32 = 90.0;
        const MIN_ANGLE: f32 = -90.0;

        let mut tracking_enabled = true;
        let mut current_pan = 0.0_f32;
        let mut current_tilt = 0.0_f32;

        while let Some(cmd) = rx.recv().await {
            match cmd {
                LocalCommand::ManualOverride { pan, tilt } => {
                    tracking_enabled = false;

                    current_pan = pan.clamp(MIN_ANGLE, MAX_ANGLE);
                    current_tilt = tilt.clamp(MIN_ANGLE, MAX_ANGLE);

                    debug!("Manual override: Pan: {current_pan:.2}°, Tilt: {current_tilt:.2}°");

                    if let Ok(mut p) = pan_servo.lock() {
                        p.set_absolute(current_pan + 90.0);
                    }
                    if let Ok(mut t) = tilt_servo.lock() {
                        t.set_absolute(current_tilt + 90.0);
                    }
                }
                LocalCommand::AutoTrim {
                    pan_delta,
                    tilt_delta,
                } => {
                    if tracking_enabled {
                        current_pan = (current_pan + pan_delta).clamp(MIN_ANGLE, MAX_ANGLE);
                        current_tilt = (current_tilt + tilt_delta).clamp(MIN_ANGLE, MAX_ANGLE);

                        tracing::trace!(
                            "AutoTrim applied. New Pan: {current_pan:.2}, New Tilt: {current_tilt:.2}"
                        );

                        if let Ok(mut p) = pan_servo.lock() {
                            p.set_absolute(current_pan + 90.0);
                        }
                        if let Ok(mut t) = tilt_servo.lock() {
                            t.set_absolute(current_tilt + 90.0);
                        }
                    }
                }
                LocalCommand::SetTrackingMode(enabled) => {
                    tracking_enabled = enabled;
                    info!(
                        "Tracking mode: {}",
                        if enabled { "ENGAGED" } else { "DISENGAGED" }
                    );
                }
            }
        }
    })
}
