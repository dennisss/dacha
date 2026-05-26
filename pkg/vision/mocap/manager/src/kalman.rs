use math::matrix::{vec2f, Vector2f, Matrix2f, Vector2ft, Vector3f};

const PROCESS_NOISE: f32 = 0.1; // 100mm

const OBSERVATION_NOISE: f32 = 0.0005; // 0.5mm


pub struct KalmanPointFilter3D {
    inner: [KalmanPointFilter1D; 3]
}

impl KalmanPointFilter3D {
    pub fn new(initial_position: &Vector3f) -> Self {
        Self {
            inner: [
                KalmanPointFilter1D::new(initial_position[0]),
                KalmanPointFilter1D::new(initial_position[1]),
                KalmanPointFilter1D::new(initial_position[2]),
            ]
        }
    }

    pub fn predict(&mut self, dt: f32) -> Vector3f {
        let mut out = Vector3f::zero();
        for i in 0..3 {
            out[i] = self.inner[i].predict(dt);
        }
        out
    }

    pub fn update(&mut self, observed_position: &Vector3f) {
        for i in 0..3 {
            self.inner[i].update(observed_position[i]);
        }
    }
}


struct KalmanPointFilter1D {
    /// Position and velocity
    /// Aka 'x'
    state: Vector2f,

    /// Aka 'P'
    covariance: Matrix2f,
}

impl KalmanPointFilter1D {

    pub fn new(initial_position: f32) -> Self {
        Self {
            state: vec2f(initial_position, 0.0),
            // TODO: Make velocity uncertainty much higher
            covariance: Matrix2f::identity() * (OBSERVATION_NOISE * OBSERVATION_NOISE),
        }
    }

    pub fn predict(&mut self, dt: f32) -> f32 {
        let f = self.state_transition_mat(dt);
        self.state = &f * &self.state;
        self.covariance = &f * &self.covariance * f.transpose() + self.process_noise_mat(dt);
        self.state[0]
    }

    pub fn update(&mut self, observed_position: f32) {
        let y = observed_position - self.state[0];

        // TODO: Base noise on the triangulation error.
        let s = self.covariance[0] + (OBSERVATION_NOISE * OBSERVATION_NOISE);

        // Kalman gain
        let k = &self.covariance * self.observation_mat().transpose() * (1.0 / s);

        self.state += k.clone() * y;

        self.covariance = (Matrix2f::identity() - k * self.observation_mat()) * &self.covariance;
    }

    /// Aka 'H'
    fn observation_mat(&self) -> Vector2ft {
        Vector2ft::from_slice(&[ 1.0, 0.0 ])
    }

    /// Aka 'Q'
    fn process_noise_mat(&self, dt: f32) -> Matrix2f {
        let dt2 = dt*dt;
        let dt3 = dt2*dt;
        let dt4 = dt3*dt;
        
        Matrix2f::from_slice(&[
            0.25 * dt4, 0.5 * dt3,
            0.5 * dt3, dt2
        ]) * (PROCESS_NOISE * PROCESS_NOISE)
    }

    // 'F_k'
    fn state_transition_mat(&mut self, dt: f32) -> Matrix2f {
        Matrix2f::from_slice(&[
            1.0, dt,
            0.0, 1.0
        ])
    }
}


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn works() {

        let mut filter = KalmanPointFilter1D::new(0.0);
        let dt = 0.1;

        for i in 0..20 {
            let pred = filter.predict(dt);
            println!("{} => {:?}", i, pred);

            filter.update(i as f32);
        }
        


    }

}
