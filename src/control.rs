use std::time::{Duration, Instant};

use tokio::{sync::watch, task::JoinHandle};

use crate::vision::CvOutput;

pub struct PidController {
    kp: f32,
    ki: f32,
    kd: f32,
    integral: f32,
    prev_error: f32,
    integral_limit: f32,
}
impl PidController {
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

    pub const fn reset_integral(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
    }
}

pub fn spawn_targeting_computer(mut rx: watch::Receiver<CvOutput>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut pan_pid = PidController::new(0.02, 0.001, 0.005, 5.0);
        let mut tilt_pid = PidController::new(0.02, 0.001, 0.005, 5.0);

        let mut last_time = Instant::now();
        let mut last_laser_seen = Instant::now();

        let laser_ttl = Duration::from_millis(500);

        let optical_center_x = 640.0;
        let optical_center_y = 360.0;
        let degrees_per_pixel_x = 0.05;
        let degrees_per_pixel_y = 0.05;

        loop {
            if rx.changed().await.is_err() {
                println!("Error in targeting computer input channel");
                break;
            }

            let cv_state = *rx.borrow_and_update();
            let now = Instant::now();
            let dt = now.duration_since(last_time).as_secs_f32();
            last_time = now;

            match (cv_state.target, cv_state.laser) {
                (Some(target), Some(laser)) => {
                    last_laser_seen = now;
                    let pan_trim = pan_pid.calculate(target.x, laser.x, dt);
                    let tilt_trim = tilt_pid.calculate(target.y, laser.y, dt);

                    println!(
                        "Target ({:.1}, {:.1}) -> Cmd: Pan {:+.2}°, Tilt {:+.2}°",
                        target.x, target.y, pan_trim, tilt_trim
                    );
                }

                (Some(target), None) => {
                    if now.duration_since(last_laser_seen) < laser_ttl {
                        println!("Laser lost, continuing motion");
                    } else {
                        pan_pid.reset_integral();
                        tilt_pid.reset_integral();

                        let pan_error = (target.x - optical_center_x) * degrees_per_pixel_x;
                        let tilt_error = (target.y - optical_center_y) * degrees_per_pixel_y;

                        println!(
                            "[Blind] Target ({:.1}, {:.1}) -> Cmd: Pan {:+.2}°, Tilt {:+.2}°",
                            target.x, target.y, pan_error, tilt_error
                        );
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
