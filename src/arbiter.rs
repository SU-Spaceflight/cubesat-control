//! Manages internal command routing. For example, when sent a command to control the gimbal, it will forward that command to the relevant gimbal control system.

use crate::hardware::MockServo;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info};

/// Commands issued locally within the satellite
///
/// These commands are either sent from automated systems within the satellite, or are forwarded from ground control.
#[derive(Debug, Clone, Copy)]
pub enum LocalCommand {
    /// Disables automated tracking and forces the servos to point at specific angles.
    ///
    /// # Fields
    /// * `pan` - The desired pan angle in degrees.
    /// * `tilt` - The desired tilt angle in degrees.
    ManualOverride { pan: f32, tilt: f32 },

    /// Incrementally adjusts the gimbal position.
    ///
    /// This command is ignored if the system is not currently in tracking mode.
    AutoTrim { pan_delta: f32, tilt_delta: f32 },

    /// If disabled, drops all `AutoTrim` commands, effectively removing the automatic targeting of the satellite.
    SetTrackingMode(bool),
}

/// Spawns the command arbiter as a background task. The arbiter handles all internal commands sent to it via its `mpsc` channel.
///
/// # Arguments
/// * `rx` - An `mpsc` receiver that contains the `LocalCommands` sent by the program.
/// * `pan_servo` - Thread safe `MockServo` that is being used for testing purposes. In future servo control will be abstracted out.
/// * `tilt_servo` - See `pan_servo`.
///
/// # Returns
/// A `JoinHandle` to the spawned Tokio task.
pub fn spawn_command_arbiter(
    mut rx: mpsc::Receiver<LocalCommand>,
    pan_servo: Arc<Mutex<MockServo>>,
    tilt_servo: Arc<Mutex<MockServo>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // TODO Use config file
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
                        // Converting servo values from -90..90 to 0..180
                        // TODO Need to revisit once servos are picked out
                        p.set_absolute(current_pan + 90.0);
                    }
                    if let Ok(mut t) = tilt_servo.lock() {
                        // See above
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
