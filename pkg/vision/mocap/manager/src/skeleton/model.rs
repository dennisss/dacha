use std::f64::consts::PI;

use math::matrix::{Vector3d, vec3d, Matrix4d, Vector4d};
use math::matrix::axis_angle::*;
use mocap_proto::mocap::*;

use crate::skeleton::inst::*;

const V2_MODEL: bool = true;


#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Left,
    Right
}

impl Side {
    fn scale_x(&self, x: f64) -> f64 {
        let s = match self {
            Self::Left => -1.0,
            Self::Right => 1.0
        };

        x * s
    }

    fn adjust_name(&self, s: &str) -> String {
        match self {
            Self::Left => s.to_string(),
            Self::Right => s.replace("LEFT", "RIGHT")
        }
    }
}

fn deg2rad(v: f64) -> f64 {
    v * PI / 180.0
}

pub fn standard_skeleton() -> Skeleton {

    // Hip / 'belt buckle' area (but in the middle of the body) will be (0,0,0)
    let mut s = Skeleton::default();

    s.default_translation = vec3d(0., 0., 0.9);

    for side in [Side::Left, Side::Right] {
        let hip = s
            .add_bone(
                side.adjust_name("LEFT_HIP"),
                vec3d(side.scale_x(0.125), 0., 0.)
            )
            .set_axis_rotation_limits(2, deg2rad(-60.), deg2rad(50.))
            .index();

        if side == Side::Left {
            s.add_marker(
                side.adjust_name("HIP_FRONT"),
                hip,
                vec3d(0.0, 0.13, 0.0)
            );
        }

        s.add_marker(
            side.adjust_name("LEFT_HIP_BACK"),
            hip,
            vec3d(side.scale_x(0.11), -0.13, 0.0)
        );

        // 'upper leg'
        let knee = s
            .add_bone(
                side.adjust_name("LEFT_UPPER_LEG"),
                vec3d(side.scale_x(0.125), 0., -0.44)
            )
            .set_parent(hip)
            .set_axis_rotation_limits(0, deg2rad(-160.), deg2rad(5.))
            .disable_axis_rotation(1)
            .disable_axis_rotation(2)
            .index();

        s.add_marker(
            side.adjust_name("LEFT_UPPER_LEG_KNEE"),
            knee,
            vec3d(side.scale_x(0.125), 0.05, -0.40)
        );

        if V2_MODEL {
            s.add_marker(
                side.adjust_name("LEFT_UPPER_LEG_KNEE_SIDE"),
                knee,
                vec3d(side.scale_x(0.125 + 0.1), 0., -0.40)
            );
        }

        
        // 'lower leg'
        let ankle = s
            .add_bone(
                side.adjust_name("LEFT_LOWER_LEG"),
                vec3d(side.scale_x(0.125), 0., -0.84)
            )
            .set_parent(knee)
            // Since this is an endpoint.
            .disable_rotation()
            .index();

        s.add_marker(
            side.adjust_name("LEFT_LOWER_LEG_ANKLE"),
            ankle,
            vec3d(side.scale_x(0.125), 0.05, -0.84)
        );
    }


    let lower_chest = s
        .add_bone(
            "LOWER_TORSO",
            vec3d(0., 0., 0.3)
        )
        .index();

    // This terminates at shoulder level
    let upper_chest = s
        .add_bone(
            "UPPER_TORSO",
            vec3d(0., 0., 0.6)
        )
        .set_parent(lower_chest)
        .disable_rotation()
        .index();

    for side in [Side::Left, Side::Right] {
        if V2_MODEL {
            if side == Side::Left {
                s.add_marker(
                    side.adjust_name("UPPER_TORSO_FRONT"),
                    upper_chest,
                    vec3d(side.scale_x(0.), 0.13, 0.4)
                );                
            }

            s.add_marker(
                side.adjust_name("UPPER_TORSO_LEFT_BACK"),
                upper_chest,
                vec3d(side.scale_x(0.11), -0.13, 0.4)
            );
        } else {
            s.add_marker(
                side.adjust_name("UPPER_TORSO_LEFT_NIPPLE"),
                upper_chest,
                vec3d(side.scale_x(0.11), 0.13, 0.4)
            );
        }

        let shoulder = s
            .add_bone(
                side.adjust_name("LEFT_SHOULDER"),
                vec3d(side.scale_x(0.2), 0., 0.6)
            )
            .set_parent(upper_chest)
            .index();

        let elbow = s
            .add_bone(
                side.adjust_name("LEFT_UPPER_ARM"),
                vec3d(side.scale_x(0.5), 0., 0.6)
            )
            .set_parent(shoulder)
            .disable_axis_rotation(1)
            .index();

        s.add_marker(
            side.adjust_name("LEFT_UPPER_ARM_ELBOW"),
            elbow,
            vec3d(side.scale_x(0.45), -0.05, 0.6)
        );

        let wrist = s
            .add_bone(
                side.adjust_name("LEFT_LOWER_ARM"),
                vec3d(side.scale_x(0.74), 0., 0.6),
            )
            .set_parent(elbow)
            // Since this is an endpoint.
            .disable_rotation()
            .index();

        s.add_marker(
            side.adjust_name("LEFT_LOWER_ARM_WRIST"),
            wrist,
            vec3d(side.scale_x(0.74), 0., 0.62)
        )

    }

    s
}

