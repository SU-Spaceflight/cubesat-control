//! manages the satellite's control loop and targeting systems.
//!
//! This module translates computer vision outputs into physical hardware movements. It houses the continuous targeting computer task and the PID controllers responsible for smooth gimbal tracking.

use std::time::{Duration, Instant};

use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tracing::{error, info, trace, warn};

use crate::{arbiter::LocalCommand, vision::CvOutput};

/// A standard Proportional-Integral-Derivative (PID) controller.
///
/// Maintains system stability by calculating the error between a desired
/// setpoint and the current value, applying correction based on proportional,
/// integral, and derivative terms.
pub struct PidController {
    kp: f32,
    ki: f32,
    kd: f32,
    integral: f32,
    prev_error: f32,
    integral_limit: f32,
}
impl PidController {
    /// Constructs a new `PidController` from given P, I, and D terms, as well as an integral limit.
    #[must_use]
    pub const fn new(kp: f32, ki: f32, kd: f32, integral_limit: f32) -> Self {
        Self {
            kp,
            ki,
            kd,
            integral: 0.0,
            prev_error: 0.0,
            integral_limit,
        }
    }

    /// Calculates the next control output based on the current system error.
    ///
    /// # Arguments
    /// * `setpoint` - The desired target value (e.g., target pixel coordinate).
    /// * `current_value` - The current measured value (e.g., laser pixel coordinate).
    /// * `dt` - The time delta in seconds since the last calculation.
    ///
    /// # Returns
    /// The calculated correction value to be applied to the system.
    pub fn calculate(&mut self, setpoint: f32, current_value: f32, dt: f32) -> f32 {
        let error = setpoint - current_value;

        let p_out = self.kp * error;

        self.integral = error
            .mul_add(dt, self.integral)
            .clamp(-self.integral_limit, self.integral_limit);

        let i_out = self.ki * self.integral;

        let derivative = (error - self.prev_error) / dt;
        let d_out = self.kd * derivative;

        self.prev_error = error;

        p_out + i_out + d_out
    }

    /// Resets the integral and previous error to 0. This is used to reset the PID if the laser is lost.
    pub const fn reset_integral(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
    }
}

/// Spawns a targeting computer background task. The targeting computer watches a `watch` channel for new information about the laser and target. When it receives a new set of values, it either uses a PID loop or a general purpose gimbal controller based off of the screen position.
///
/// # Arguments
/// * `rx` - `watch` channel that supplies the target and laser coordinates.
/// * `tx` - `mpsc` channel to send gimbal commands on.
///
/// # Returns
/// A `JoinHandle` to the spawned Tokio task.
pub fn spawn_targeting_computer(
    mut rx: watch::Receiver<CvOutput>,
    tx: mpsc::Sender<LocalCommand>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Placeholder PID values
        // TODO read from config file
        let mut pan_pid = PidController::new(0.02, 0.001, 0.005, 5.0);
        let mut tilt_pid = PidController::new(0.02, 0.001, 0.005, 5.0);

        let mut last_time = Instant::now();
        let mut last_laser_seen = Instant::now();

        let laser_ttl = Duration::from_millis(500);

        // Current expected canvas size is 1280x720
        // TODO Read camera size from config file
        let optical_center_x = 640.0;
        let optical_center_y = 360.0;
        // Hardcoded FOV
        // TODO Read camera FOV from config file
        let degrees_per_pixel_x = 0.05;
        let degrees_per_pixel_y = 0.05;

        let mut target_acquired = false;
        let mut laser_acquired = false;

        loop {
            if rx.changed().await.is_err() {
                error!("Error in targeting computer input channel");
                break;
            }

            let cv_state = *rx.borrow_and_update();
            let now = Instant::now();
            let dt = now.duration_since(last_time).as_secs_f32();
            last_time = now;

            let currently_has_target = cv_state.target.is_some();
            if currently_has_target != target_acquired {
                target_acquired = currently_has_target;
                if target_acquired {
                    info!("Target acquired");
                } else {
                    info!("Target lost");
                }
            }

            let currently_has_laser = cv_state.laser.is_some();
            if currently_has_laser != laser_acquired {
                laser_acquired = currently_has_laser;
                if laser_acquired {
                    info!("Laser acquired");
                } else {
                    info!("Laser lost");
                }
            }

            match (cv_state.target, cv_state.laser) {
                (Some(target), Some(laser)) => {
                    last_laser_seen = now;
                    let pan_trim = pan_pid.calculate(target.x, laser.x, dt);
                    let tilt_trim = tilt_pid.calculate(target.y, laser.y, dt);

                    trace!("PID Trim - Pan: {pan_trim:.3}, Tilt: {tilt_trim:.3}");

                    if let Err(e) = tx.try_send(LocalCommand::AutoTrim {
                        pan_delta: pan_trim,
                        tilt_delta: tilt_trim,
                    }) {
                        warn!("Could not add to Arbiter channel {e}");
                    }
                }

                (Some(target), None) => {
                    if now.duration_since(last_laser_seen) < laser_ttl {
                    } else {
                        pan_pid.reset_integral();
                        tilt_pid.reset_integral();

                        let pan_error = (target.x - optical_center_x) * degrees_per_pixel_x;
                        let tilt_error = (target.y - optical_center_y) * degrees_per_pixel_y;

                        let _ = tx.try_send(LocalCommand::AutoTrim {
                            pan_delta: pan_error,
                            tilt_delta: tilt_error,
                        });
                    }
                }

                (None, _) => {
                    pan_pid.reset_integral();
                    tilt_pid.reset_integral();
                }
            }
        }
    })
}
