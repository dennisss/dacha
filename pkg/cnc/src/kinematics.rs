/// Calculates how long it would take to travel a given displacement.
///
///
/// Returns the smallest time value >= 0.
pub fn time_to_travel(displacement: f32, start_velocity: f32, acceleration: f32) -> f32 {
    if displacement == 0.0 {
        return 0.0;
    }

    let a = acceleration / 2.0;
    let b = start_velocity;
    let c = -displacement;

    let (t1, t2) = math::find_quadratic_roots(a, b, c);

    if t2 >= 0.0 {
        t2
    } else {
        t1
    }
}

pub fn displacement_traveled(start_velocity: f32, acceleration: f32, duration: f32) -> f32 {
    ((acceleration / 2.0) * duration * duration) + (start_velocity * duration)
}
