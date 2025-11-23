use std::time::Instant;

pub struct PIDController {
    k_p: f32,
    k_i: f32,
    k_d: f32,
    last_error: Option<(f32, Instant)>,
    error_integral: f32,
}

impl PIDController {
    pub fn new() -> Self {
        Self {
            k_p: 0.1,
            k_i: 0.01,
            k_d: 0.001,
            last_error: None,
            error_integral: 0.0,
        }

    }

    pub fn next(&mut self, error: f32, time: Instant) -> f32 {
        let mut d_error = 0.0;
        if let Some((last_error, last_time)) = self.last_error.take() {
            let dt = (time - last_time).as_secs_f32();
            d_error = (error - last_error) / dt;
            self.error_integral += error * dt; // TODO: Trapezoidal error?
        }
        self.last_error = Some((error, time));

        // Limit the contribution of the integral filter to the [0, 1] range in the final output.
        self.error_integral = self.error_integral.min(1.0 / self.k_i).max(0.0 / self.k_i);



        let v = (error * self.k_p) + (d_error * self.k_d) + (self.error_integral * self.k_i);

        v.min(1.0).max(0.0)
    }
}
