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