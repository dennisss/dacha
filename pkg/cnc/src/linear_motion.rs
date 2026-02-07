use math::matrix::VectorXd;

/// A single fully defined motion in a straight line with constant acceleration
#[derive(Debug, PartialEq, Clone)]
pub struct LinearMotion {
    pub start_position: VectorXd,
    pub start_velocity: VectorXd,

    pub end_position: VectorXd,
    pub end_velocity: VectorXd,

    pub acceleration: VectorXd,

    pub duration: f64,
}

impl LinearMotion {
    pub fn split_at(self, time: f64) -> (Self, Self) {
        assert!(time <= self.duration);

        let mid_point = &self.start_position + (self.start_velocity.clone() * time)
            + self.acceleration.clone() * (0.5 * time * time);

        let mid_velocity = &self.start_velocity + self.acceleration.clone() * time;

        let a = Self {
            start_position: self.start_position,
            start_velocity: self.start_velocity,
            end_position: mid_point.clone(),
            end_velocity: mid_velocity.clone(),
            acceleration: self.acceleration.clone(),            
            duration: time
        };

        let b = Self {
            start_position: mid_point,
            start_velocity: mid_velocity,
            end_position: self.end_position,
            end_velocity: self.end_velocity,
            acceleration: self.acceleration,
            duration: self.duration - time
        };

        (a, b)
    }
}

