use math::matrix::Vector3f;

/// A single fully defined motion in a straight line with constant acceleration
#[derive(Debug)]
pub struct LinearMotion {
    pub start_position: Vector3f,
    pub start_velocity: Vector3f,

    pub end_position: Vector3f,
    pub end_velocity: Vector3f,

    pub acceleration: Vector3f,

    pub duration: f32,
}

/*
delta_A = delta_X + delta_Y
delta_B = delta_X - delta_Y

Y = Y

*/
