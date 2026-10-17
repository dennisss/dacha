use common::errors::*;
use mocap_proto::mocap::*;
use math::matrix::{vec3d, Matrix4d};
use vision::CameraExtrinsics;
use math_proto_util::VectorProtoExt;

use crate::matching::TrackedPoint;
use crate::rigid_body::*;
use crate::config::*;
use crate::wand::wand_reference_points;


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

    // The origin frame is the wand frame rotated by 180 degrees around Z and
    // raised by the marker height (wand laying flat on the ground).
    let w = config.wand();
    let reference_points = wand_reference_points(w)?;
    let pts = vec![
        &reference_points[1],
        &reference_points[0],
        &reference_points[2],
        &reference_points[3],
    ].into_iter().map(|p| vec3d(-p.x(), -p.y(), p.z() + w.height())).collect::<Vec<_>>();

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
