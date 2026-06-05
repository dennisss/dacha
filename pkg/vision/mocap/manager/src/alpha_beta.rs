use math::matrix::Vector3f;


// TODO: Ideally we should ensure that the 'dt' passed to this never changes.
pub struct AlphaBetaEstimator3D {
    x: Vector3f,
    v: Vector3f,
    alpha: f32,
    beta: f32,
}

impl AlphaBetaEstimator3D {
    pub fn new(initial_position: &Vector3f, alpha: f32, beta: f32) -> Self {
        Self {
            x: initial_position.clone(),
            v: Vector3f::zero(),
            alpha,
            beta
        }
    }

    pub fn x(&self) -> &Vector3f {
        &self.x
    }

    /// Updates the state of the estimator to the predicted position at time 't + dt'.
    /// (assuming the estimator was previously at time 't')
    ///
    /// Returns the predicted position.
    pub fn predict(&mut self, dt: f32) -> Vector3f {
        self.x += self.v.clone() * dt;
        self.x.clone()
    }

    pub fn update(&mut self, dt: f32, observed_position: &Vector3f) {
        let r = observed_position - &self.x;

        self.x += r.clone() * self.alpha;
        self.v += r * (self.beta / dt);
    }
}