use math::matrix::VectorXd;

/// Gets the largest magnitude vector pointing in the same direction as
/// 'direction' which ensuring that no axis exceeds the component wise
/// magnitude 
pub fn constrained_vector(direction: &VectorXd, axis_limits: &[f64]) -> VectorXd {
    assert_eq!(axis_limits.len(), direction.len());

    let mut direction = direction.clone().normalized();

    let mut limit = None;
    let mut limiting_axis = 0;

    for (axis_i, axis_limit) in axis_limits.iter().cloned().enumerate() {
        if direction[axis_i].abs() < 0.0001 {
            continue;
        }

        let v = (axis_limit / direction[axis_i]).abs();
        if let Some(limit) = limit {
            if v >= limit {
                continue;
            }
        }
        
        limit = Some(v);
        limiting_axis = axis_i;
    }

    // Will crash if direction has zero magnitude in all directions.
    assert!(limit.is_some());

    for i in 0..direction.len() {
        if i != limiting_axis {
            direction[i] = axis_limits[limiting_axis].abs() * (direction[i] / direction[limiting_axis].abs());
        }
    }
    direction[limiting_axis] = axis_limits[limiting_axis].copysign(direction[limiting_axis]);

    direction
}

#[cfg(test)]
mod tests {
    use super::*;

    use math::vecxd;

    #[test]
    fn constrained_vector_test() {
        assert_eq!(constrained_vector(&vecxd!(100.0), &[20.0]), vecxd!(20.0));
        assert_eq!(constrained_vector(&vecxd!(100.0, 0.0), &[20.0, 0.0]), vecxd!(20.0, 0.0));
        assert_eq!(constrained_vector(&vecxd!(100.0, 0.0), &[20.0, 20.0]), vecxd!(20.0, 0.0));
        assert_eq!(constrained_vector(&vecxd!(100.0, 0.0), &[20.0, 40.0]), vecxd!(20.0, 0.0));
        assert_eq!(constrained_vector(&vecxd!(100.0, 0.0), &[40.0, 20.0]), vecxd!(40.0, 0.0));
        assert_eq!(constrained_vector(&vecxd!(0.0, 100.0), &[40.0, 20.0]), vecxd!(0.0, 20.0));
        assert_eq!(constrained_vector(&vecxd!(0.0, 100.0), &[20.0, 40.0]), vecxd!(0.0, 40.0));
        assert_eq!(constrained_vector(&vecxd!(50.0, 50.0), &[20.0, 40.0]), vecxd!(20.0, 20.0));
        assert_eq!(constrained_vector(&vecxd!(40.0, 60.0), &[20.0, 40.0]), vecxd!(20.0, 30.0));
        assert_eq!(constrained_vector(&vecxd!(60.0, 40.0), &[40.0, 20.0]), vecxd!(30.0, 20.0));

        assert_eq!(constrained_vector(&vecxd!(0.0, 40.0, 60.0, 0.0), &[0.0, 20.0, 40.0, 0.0]), vecxd!(0.0, 20.0, 30.0, 0.0));
        
        assert_eq!(constrained_vector(&vecxd!(60.0, -40.0), &[40.0, 20.0]), vecxd!(30.0, -20.0));
        assert_eq!(constrained_vector(&vecxd!(-60.0, 40.0), &[40.0, 20.0]), vecxd!(-30.0, 20.0));

        assert_eq!(constrained_vector(&vecxd!(-60.0, 40.0), &[40.0, 0.0]), vecxd!(0.0, 0.0));
        assert_eq!(constrained_vector(&vecxd!(-60.0, 40.0), &[0.0, 10.0]), vecxd!(0.0, 0.0));
        assert_eq!(constrained_vector(&vecxd!(-60.0, 40.0), &[0.0, 0.0]), vecxd!(0.0, 0.0));

        println!("{:?}", constrained_vector(&vecxd!(2.106, -0.434, 0.0, 0.0), &[ 40.0, 40.0, 12.0, 0.0 ]));

    }

}
