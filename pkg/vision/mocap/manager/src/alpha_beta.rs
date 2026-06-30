use math::matrix::Vector3d;


// TODO: Ideally we should ensure that the 'dt' passed to this never changes.
pub struct AlphaBetaEstimator3D {
    x: Vector3d,
    v: Vector3d,
    alpha: f64,
    beta: f64,
}

impl AlphaBetaEstimator3D {
    pub fn new(initial_position: &Vector3d, alpha: f64, beta: f64) -> Self {
        Self {
            x: initial_position.clone(),
            v: Vector3d::zero(),
            alpha,
            beta
        }
    }

    pub fn x(&self) -> &Vector3d {
        &self.x
    }

    /// Updates the state of the estimator to the predicted position at time 't + dt'.
    /// (assuming the estimator was previously at time 't')
    ///
    /// Returns the predicted position.
    pub fn predict(&mut self, dt: f64) -> Vector3d {
        self.x += self.v.clone() * dt;
        self.x.clone()
    }

    pub fn update(&mut self, dt: f64, observed_position: &Vector3d) {
        let r = observed_position - &self.x;

        self.x += r.clone() * self.alpha;
        self.v += r * (self.beta / dt);
    }
}