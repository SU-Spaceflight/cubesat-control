//! Manages internal command routing. For example, when sent a command to control the gimbal, it will forward that command to the relevant gimbal control system.

use crate::hardware::Servo;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

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
    mut pan_servo: Box<dyn Servo>,
    mut tilt_servo: Box<dyn Servo>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tracking_enabled = true;

        while let Some(cmd) = rx.recv().await {
            match cmd {
                LocalCommand::ManualOverride { pan, tilt } => {
                    tracking_enabled = false;
                    debug!("Manual override: Pan: {pan:.2}°, Tilt: {tilt:.2}°");

                    if let Err(e) = pan_servo.set_absolute(pan) {
                        error!("Pan servo hardware fault: {e}");
                    }
                    if let Err(e) = tilt_servo.set_absolute(tilt) {
                        error!("Tilt servo hardware fault: {e}");
                    }
                }
                LocalCommand::AutoTrim {
                    pan_delta,
                    tilt_delta,
                } => {
                    if tracking_enabled {
                        let new_pan = pan_servo.current_angle() + pan_delta;
                        let new_tilt = tilt_servo.current_angle() + tilt_delta;

                        if let Err(e) = pan_servo.set_absolute(new_pan) {
                            error!("Pan servo hardware fault: {e}");
                        }
                        if let Err(e) = tilt_servo.set_absolute(new_tilt) {
                            error!("Tilt servo hardware fault: {e}");
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
