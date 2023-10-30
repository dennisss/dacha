extern crate cnc;
extern crate math;

use cnc::linear_motion::LinearMotion;
use math::matrix::Vector3f;

fn main() {
    let motions = [
        LinearMotion {
            start_position: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            start_velocity: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            end_position: Vector3f::from_slice(&[10240.0, 0.0, 0.0]),
            end_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            acceleration: Vector3f::from_slice(&[500.0, 0.0, 0.0]),
            duration: 6.4,
        },
        LinearMotion {
            start_position: Vector3f::from_slice(&[10240.0, 0.0, 0.0]),
            start_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            end_position: Vector3f::from_slice(&[53760.0, 0.0, 0.0]),
            end_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            acceleration: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            duration: 13.6,
        },
        LinearMotion {
            start_position: Vector3f::from_slice(&[53760.0, 0.0, 0.0]),
            start_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            end_position: Vector3f::from_slice(&[64000.0, 0.0, 0.0]),
            end_velocity: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            acceleration: Vector3f::from_slice(&[-500.0, 0.0, 0.0]),
            duration: 6.4,
        },
    ];

    for motion in &motions {
        let start_position = motion.start_position[0] as i32;
        println!("{}", start_position);
    }
}
