use common::errors::*;
use mocap_proto::mocap::*;
use math::matrix::{vec3d, Matrix4d};
use vision::CameraExtrinsics;
use math_proto_util::VectorProtoExt;

use crate::matching::TrackedPoint;
use crate::rigid_body::*;
use crate::config::*;


/// Finds a calibration wand in the given point cloud and creates a config
/// patch that will set the origin position/orientaiton based on that wand.
///
/// TODO: Support optionally ignoring any X/Y axis rotations and just
/// taking Z yaw adjustments given the accelerometer based alignment.
pub fn set_origin_with_wand(
    config: &ManagerConfigContainer,
    points: &[TrackedPoint]
) -> Result<MocapManagerConfig> {
    let mut tracker_config = config.rigid_body_tracker().clone();
    tracker_config.clear_bodies();

    let w = config.wand();
    let pts = vec![
        vec3d(0., 0., w.height()),
        vec3d(w.left_arm_length(), 0., w.height()),
        vec3d(-w.right_arm_length(), 0., w.height()),
        vec3d(0., w.bottom_length(), w.height()),
    ];

    let body = tracker_config.new_bodies();
    body.set_id(1u32);

    for pt in pts {
        body.add_points(pt.to_proto());
    }

    let mut tracker = RigidBodyTracker::default();
    tracker.set_config(tracker_config);
    
    tracker.run(points);

    let bodies = tracker.bodies();

    if bodies.len() != 1 || bodies[0].transform.is_none() {
        return Err(err_msg("Wand not found"));
    }

    let mut transform = {
        let (r, t) = bodies[0].transform.as_ref().unwrap();
        
        let mut out = Matrix4d::zero();
        out.block_mut(0, 0).copy_from(&r);
        out.block_mut(0, 3).copy_from(&t);
        out[(3, 3)] = 1.0;
        out
    };

    let mut patch = MocapManagerConfig::default();

    for cam in config.per_camera() {
        let old_extrinsics = match config.camera_extrinsics().get(&cam.camera_id()) {
            Some(v) => v,
            None => continue
        };

        let new_extrinsics = CameraExtrinsics::from_mat4x4(&(
            old_extrinsics.to_mat4x4() * &transform
        ));

        let mut proto = patch.new_per_camera();
        proto.set_camera_id(cam.camera_id());
        proto.set_extrinsics(new_extrinsics.to_proto());
    }

    Ok(patch)
}
