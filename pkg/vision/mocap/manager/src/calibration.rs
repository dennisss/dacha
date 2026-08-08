
use std::io::Read;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;
use std::collections::HashMap;

use common::errors::*;
use common::io::{Readable, Writeable};
use common::hash::FastHasherBuilder;
use executor::bundle::TaskResultBundle;
use file::LocalPathBuf;
use math::array::Array;
use image::Colorspace;
use file::{project_path, LocalPath};
use mocap_proto::mocap::*;
use sstable::record_log::RecordReader;
use protobuf::{StaticMessage, Message};
use protobuf_json::MessageJsonSerialize;
use math::matrix::axis_angle::*;
use math::matrix::{vec2d, vec3d, Vector2d, Matrix3d, Vector3d, Matrix4d};
use vision::{CameraIntrinsicsModel, CameraExtrinsics, BundleAdjustmentSolver};
use cluster_client::id::{entity_id_from_string, entity_id_to_string};

use crate::wand::*;
use crate::matching::CameraParameters;
use crate::proto_utils::*;
use crate::config::*;


/*
TODO: Eventually do full spatial dedupping instead of just looking at the previous matched frame.

TODO: Need to support calibration using a subset of cameras if we need to disable some (or just all that have data).

*/


pub struct WandingCalibrationSolver {
    config: MocapManagerConfig,
    camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
    
    stats: WandingCalibrationStats,

    /// Data after deduplication.
    frames: Vec<FrameData>,

    // TODO: Only need to preserve the extrinsic parameters in this.
    last_match_per_camera: HashMap<u64, BlobPatternMatch, FastHasherBuilder>,

    initial_system_state: Option<MocapManagerStatus>,
}

#[derive(Clone)]
pub struct WandingCalibrationSolution {
    pub error: f64,
    pub params: Vec<CameraParameters>
}


impl WandingCalibrationSolver {

    /// Initializes a calibrator with configs and initial intrinsics pulled from the given config.
    pub fn new(
        config: MocapManagerConfig,
        camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
    ) -> Self {
        Self {
            config,
            camera_intrinsics,
            frames: vec![],
            last_match_per_camera: Default::default(),
            initial_system_state: None,
            stats: WandingCalibrationStats::default(),
        }
    }

    pub fn set_initial_status(&mut self, status: MocapManagerStatus) {
        self.initial_system_state = Some(status);
    }

    pub fn stats(&self) -> WandingCalibrationStats {
        self.stats.clone()
    }

    /// TODO: If doing this offline, the wand finding is parallelizable across
    /// all the frames.
    pub fn add_frame(&mut self, entry: &MocapLogEntry) -> Result<()> {
        *self.stats.raw_num_frames_mut() += 1;

        // At least two cameras need to be involved for correlating relative
        // positions between cameras.
        if entry.blobs().cameras().len() < 2 {
            return Ok(());
        }

        let mut cameras_data = HashMap::default();

        let mut found_wand = false;

        for cam in entry.blobs().cameras() {
            let intrinsics = self.camera_intrinsics.get(&cam.camera_id())
                .ok_or_else(|| format_err!(
                    "Missing intrinsics for camera: {}",
                    entity_id_to_string(cam.camera_id()).unwrap()
                ))?;

            let m = match BlobPatternFinder::find_t_wand(self.config.wand(), intrinsics, cam.results()) {
                Some(v) => v,
                None => continue
            };

            if m.error > self.config.wanding().max_wand_frame_error() {
                continue;
            }

            if !found_wand {
                found_wand = true;
                *self.stats.num_valid_frames_mut() += 1;
            }

            // TODO: If there are multiple frames nearby, pick the one with the lowest reprojection error.
            if let Some(last_match) = self.last_match_per_camera.get(&cam.camera_id()) {
                if !pattern_positions_distinct(&m, last_match) {
                    return Ok(());
                }
            }

            let mut points_2d = vec![];
            let mut points_3d = vec![];
            for p in &m.points {
                points_3d.push(p.reference_point.clone());

                let blob = &cam.results().blobs()[p.blob_index];
                points_2d.push(vec2d(blob.x() as f64, blob.y() as f64));
            }

            cameras_data.insert(cam.camera_id(), FrameCameraData {
                points_2d,
                points_3d,
                pattern: m
            });
        }

        if cameras_data.len() < 2 {
            return Ok(());
        }

        // Update last position per camera for future dedupping.
        for (cam_id, data) in &cameras_data {
            self.last_match_per_camera.insert(*cam_id, data.pattern.clone());
        }

        self.frames.push(FrameData {
            cameras: cameras_data
        });

        *self.stats.num_deduped_frames_mut() += 1;

        Ok(())
    }

    pub fn solve(&mut self) -> Result<WandingCalibrationSolution> {

        let mut data = vec![];
        core::mem::swap(&mut data, &mut self.frames);

        // println!("Subsetting wands...");
        // let mut data = self.select_frame_subset(&mut data)?;

        // println!("Num cams: {}", num_cameras);

        println!("Remaining entries: {}", data.len());

        if data.len() == 0 {
            return Err(err_msg("No data with good enough matches"));
        }


        // Initialize rough camera extrinsics guesses.
        let mut camera_initial_extrinsics = self.extract_initial_extrinsics(&mut data)?;

        let mut solution = self.optimize(&data, &camera_initial_extrinsics)?;

        self.align_extrinsics(&mut solution.params)?;

        Ok(solution)
    }

    fn select_frame_subset(&self, frames: &mut Vec<FrameData>) -> Result<Vec<FrameData>> {
        const CROSS_CAMERA_WEIGHT: f64 = 0.1;

        const SINGLE_CAMERA_WEIGHT: f64 = 10.0;

        const PAIR_WEIGHT: f64 = 20.0;

        // Target number of frames to select in total.
        const TARGET_SUBSET_SIZE: usize = 80;

        // Rough number of times that we want each camera to be observed.
        const SINGLE_CAMERA_TARGET_OBSERVATIONS: usize = 10;

        // Target number of observations of each pair of cameras.
        const PAIR_TARGET_COINCIDENCES: usize = 5;

        // Selected frames.
        let mut out = vec![];

        // How many times each camera has appeared so far in the selected data.
        let mut camera_counts = HashMap::<u64, usize, FastHasherBuilder>::default();

        let mut camera_coincidences = HashMap::<(u64, u64), usize, FastHasherBuilder>::default();

        // TODO: Need some randomization for similarly weighted pairs.

        while frames.len() > 0 && out.len() < TARGET_SUBSET_SIZE {

            let mut best_score = 0.0;
            let mut best_i = 0;

            for i in 0..frames.len() {

                let frame = &frames[i];

                let score = {
                    
                    let mut score = 0.0;
                
                    score += CROSS_CAMERA_WEIGHT * (frame.cameras.len() as f64);

                    for cam_id in frame.cameras.keys() {
                        let c = camera_counts.get(cam_id).cloned().unwrap_or(0);
                        score += SINGLE_CAMERA_WEIGHT * ((SINGLE_CAMERA_TARGET_OBSERVATIONS as f64) - (c as f64)).max(0.0);
                    }

                    for cam_id1 in frame.cameras.keys() {
                        for cam_id2 in frame.cameras.keys() {
                            if cam_id1 >= cam_id2 {
                                continue;
                            }

                            let c = camera_coincidences.get(&(*cam_id1, *cam_id2)).cloned().unwrap_or(0);
                            score += PAIR_WEIGHT * ((PAIR_TARGET_COINCIDENCES as f64) - (c as f64)).max(0.0);
                        }
                    }

                    score
                };

                if score >= best_score {
                    best_score = score;
                    best_i = i;
                }
            }

            let frame = frames.swap_remove(best_i);

            {
                for cam_id in frame.cameras.keys() {
                    *camera_counts.entry(*cam_id).or_default() += 1;
                }

                for cam_id1 in frame.cameras.keys() {
                    for cam_id2 in frame.cameras.keys() {
                        if cam_id1 >= cam_id2 {
                            continue;
                        }

                        *camera_coincidences.entry((*cam_id1, *cam_id2)).or_default() += 1;
                    }
                }
            }

            out.push(frame);
        }

        Ok(out)
    }

    fn extract_initial_extrinsics(
        &self, data: &mut Vec<FrameData>
    ) -> Result<HashMap<u64, CameraExtrinsics, FastHasherBuilder>> {
        let mut camera_initial_extrinsics = HashMap::<u64, CameraExtrinsics, FastHasherBuilder>::default();

        data.sort_by_key(|v| v.cameras.len());
        data.reverse();

        // Init with extrinsics from first entry (the one)
        println!("Best frame spans {} cameras", data[0].cameras.len());
        for (id, cam) in &data[0].cameras {
            let extrinsics = CameraExtrinsics {
                rotation: cam.pattern.rotation.clone(),
                translation: cam.pattern.translation.clone(),
            };

            camera_initial_extrinsics.insert(*id, extrinsics);
        }

        // TODO: Validate that we have enough high quality linkages between camera subsets and we 
        // enforce at most 2 hops from any camera to another camera.


        // Greedily attempt to get extrinsics for all other cameras relative to existing cameras.
        // TODO: Implement a proper minimum spanning tree.
        let mut changed = true;

        while changed {
            changed = false;

            for entry in &data[1..] {
                //

                let mut known_cam_id = None;

                for (cam_id, _) in &entry.cameras {
                    if camera_initial_extrinsics.contains_key(cam_id) {
                        known_cam_id = Some(*cam_id);
                        break;
                    }
                }

                // TODO: Skip below stuff if there are no unknown cameras in the current entry.

                let known_cam_id = match known_cam_id {
                    Some(v) => v,
                    None => continue
                };

                let known_cam_extrinsics = camera_initial_extrinsics.get(&known_cam_id).unwrap();
                let known_cam_global_mat = known_cam_extrinsics.to_mat4x4();

                let known_cam_data = entry.cameras.get(&known_cam_id).unwrap();
                let known_cam_local_mat = CameraExtrinsics {
                    rotation: known_cam_data.pattern.rotation.clone(),
                    translation: known_cam_data.pattern.translation.clone(),
                }.to_mat4x4();

                for (cam_id, cam_data) in &entry.cameras {
                    if camera_initial_extrinsics.contains_key(cam_id) {
                        continue;
                    }

                    let cam_local_mat = CameraExtrinsics {
                        rotation: cam_data.pattern.rotation.clone(),
                        translation: cam_data.pattern.translation.clone(),
                    }.to_mat4x4();

                    // TODO: Cache the inverse.
                    let rel_mat = cam_local_mat * known_cam_local_mat.inverse().unwrap();

                    let global_mat = rel_mat * &known_cam_global_mat;

                    camera_initial_extrinsics.insert(
                        *cam_id,
                        CameraExtrinsics::from_mat4x4(&global_mat)
                    );
                    changed = true;
                }
            }
        }

        println!("Num initial extrinsics: {}", camera_initial_extrinsics.len());

        let initial_system_state = self.initial_system_state.as_ref()
            .ok_or_else(|| err_msg("Missing initial system status"))?;

        for cam in initial_system_state.cameras() {
            if !camera_initial_extrinsics.contains_key(&cam.id()) {
                eprintln!("Missing camera: {} ({:?})", cam.id(), entity_id_to_string(cam.id()));
            }
        }

        let num_cameras = initial_system_state.cameras().len();

        // TODO: Also verify no unknown cameras are present in the data but not in the initial set.
        if camera_initial_extrinsics.len() != num_cameras {
            return Err(err_msg("Not all cameras are linked by some frame chain."));
        }

        Ok(camera_initial_extrinsics)
    }


    fn optimize(
        &self,
        data: &[FrameData],
        camera_initial_extrinsics: &HashMap<u64, CameraExtrinsics, FastHasherBuilder>,
    ) -> Result<WandingCalibrationSolution> {
        // TODO: Use something like a Huber/Cauchy loss to reduce sensitivity to outliers if we
        // messed up want detection.
        let mut solver = BundleAdjustmentSolver::new();

        let mut camera_id_to_index = HashMap::<u64, usize>::default();

        for (id, extrinsics) in camera_initial_extrinsics {
            // TODO: Chose whichever camera has the most coincidences with other cameras.
            let fixed = camera_id_to_index.is_empty();

            let idx = solver.add_camera(
                self.camera_intrinsics.get(id).unwrap(),
                &extrinsics.rotation,
                &extrinsics.translation,
                fixed,
            );

            camera_id_to_index.insert(*id, idx);
        }

        for entry in data {

            // Init object based on first camera
            let object_idx = {
                let (cam_id, cam_data) = entry.cameras.iter().next().unwrap();
                let cam_idx = camera_id_to_index.get(cam_id).unwrap();

                // estimated_transform = initial_camera_transform * object_transform
                // ^ Need to solve for object_transform;

                let initial_camera_transform = camera_initial_extrinsics.get(cam_id).unwrap()
                    .to_mat4x4();

                // println!("AA: {:?}", initial_camera_transform);


                let estimated_transform = CameraExtrinsics {
                    rotation: cam_data.pattern.rotation.clone(),
                    translation: cam_data.pattern.translation.clone(),
                }.to_mat4x4();

                // println!("BBB: {:?}", estimated_transform);
                
                let object_transform = initial_camera_transform.inverse().unwrap() * estimated_transform;

                // println!("OBJ TRANSFORM: {:?}", object_transform);

                let (object_rotation, object_translation) = extrinsics_from_mat4x4(&object_transform);

                solver.add_object(
                    &object_rotation,
                    &object_translation
                )
            };

            for (cam_id, cam_data) in &entry.cameras {
                let cam_idx = *camera_id_to_index.get(cam_id).unwrap();

                for i in 0..cam_data.points_2d.len() {
                    let point_2d = &cam_data.points_2d[i];
                    let point_3d = &cam_data.points_3d[i];

                    solver.add_object_point_view(
                        object_idx,
                        cam_idx,
                        point_2d,
                        point_3d
                    );
                }
            }

        }

        println!("Solving...");

        solver.enable_logging();

        let solution = solver.solve();

        println!("Solved!");

        let mut out = WandingCalibrationSolution {
            error: solution.error(),
            params: vec![]
        };

        for (cam_id, cam_idx) in camera_id_to_index {
            out.params.push(CameraParameters {
                id: cam_id,
                intrinsics: solution.camera_intrinsics(cam_idx),
                extrinsics: solution.camera_extrinsics(cam_idx)
            });
        }

        // TODO: Ideally compute RMS over all data and not just the selected subset.
        println!("RMS Error: {}", out.error);

        Ok(out)
    }

    // MAkes 
    fn align_extrinsics(
        &self,
        camera_params: &mut [CameraParameters]
    ) -> Result<()> {
        let mut up_vector = Vector3d::zero();
        let mut num_cams = 0;

        let initial_system_state = self.initial_system_state.as_ref()
            .ok_or_else(|| err_msg("Missing initial system status"))?;

        for cam in initial_system_state.cameras() {
            let cam_id = cam.id();

            let extrinsics = &camera_params.iter().find(|p| p.id == cam_id)
                .ok_or_else(|| err_msg("Camera not declared in system_state"))?
                .extrinsics;

            let r = from_axis_angle(&extrinsics.rotation);

            if !cam.camera_status().accelerometer().has_value() {
                return Err(err_msg("Camera missing accelerometer vector"));
            }

            let proto = cam.camera_status().accelerometer().value();

            let v: Vector3d = r.transpose() * vec3d(proto.x() as f64, proto.y() as f64, proto.z() as f64).normalized();
            up_vector += v;
            num_cams += 1;
        }

        up_vector /= (num_cams as f64);
        up_vector.normalize();

        
        let r_align = {
            let z = vec3d(0., 0., 1.);
            let mut axis = up_vector.cross(&z).normalized();
            let angle = up_vector.dot(&z).acos();
            axis *= angle;
            from_axis_angle(&axis)
        };

        let t_align = {

            let mut cam_center_pos = vec3d(0., 0., 0.);
            let mut num_cams = 0;
            let mut z_max = -1000000.0f64;
            let mut z_min = 1000000.0f64;

            for p in camera_params.iter() {
                let pos = &r_align * p.extrinsics.position();
                z_max = z_max.max(pos.z());
                z_min = z_min.min(pos.z());
                cam_center_pos += pos;
                num_cams += 1;
            }

            cam_center_pos /= (num_cams as f64);

            let t_z = self.config.wanding().camera_height_guess() - z_max;

            vec3d(
                -cam_center_pos.x(),
                -cam_center_pos.y(),
                t_z
            )
        };

        let align_mat = {
            let mut out = Matrix4d::zero();
            out.block_mut(0, 0).copy_from(&r_align.transpose());
            out.block_mut(0, 3).copy_from(&(
                r_align.transpose() * t_align * -1.0   
            ));
            out[(3, 3)] = 1.0;
            out
        };

        for p in camera_params.iter_mut() {
            p.extrinsics = CameraExtrinsics::from_mat4x4(&(
                p.extrinsics.to_mat4x4() * &align_mat
            ));
        }

        // TODO: Only do this if we have an accelerometer lock.

        // Correcting the yaw (rotation around Z) such that the cameras align
        // with orthogonal directions.
        if camera_params.len() > 1 {
            // Order the cameras based on their angle from the center.
            let mut camera_order = vec![];
            for p in camera_params.iter() {
                let pos = p.extrinsics.position();
                camera_order.push((pos.y().atan2(pos.x()), p.id));
            }

            // TODO: Make more resilient to cameras that are stacked vertically.
            camera_order.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let mut dir_sum = vec2d(0., 0.);

            for i in 0..camera_order.len() {
                let j = (i + 1) % camera_order.len();
                
                let i_id = camera_order[i].1;
                let j_id = camera_order[j].1;

                let p_i = camera_params.iter().find(|p| p.id == i_id).unwrap();
                let p_j = camera_params.iter().find(|p| p.id == j_id).unwrap();

                let dir = p_i.extrinsics.position() - p_j.extrinsics.position();
                let angle = dir.y().atan2(dir.x());

                dir_sum[0] += (4. * angle).cos();
                dir_sum[1] += (4. * angle).sin();
            }

            let yaw = dir_sum[1].atan2(dir_sum[0]) / 4.0;

            {
                let align_mat = {
                    let mut out = Matrix4d::identity();
                    // TODO: Use a helper for this.
                    out[(0, 0)] = (-yaw).cos();
                    out[(0, 1)] = -(-yaw).sin();
                    out[(1, 0)] = (-yaw).sin();
                    out[(1, 1)] = (-yaw).cos();
                    out
                };

                for p in camera_params.iter_mut() {
                    p.extrinsics = CameraExtrinsics::from_mat4x4(&(
                        p.extrinsics.to_mat4x4() * &align_mat
                    ));
                }
            }
        }

        Ok(())
    }

}

fn pattern_positions_distinct(a: &BlobPatternMatch, b: &BlobPatternMatch) -> bool {
    let dt = (&a.translation - &b.translation).norm();
    if dt > 0.05 { // 50mm
        return true;
    }

    let a_r = from_axis_angle(&a.rotation);
    let b_r = from_axis_angle(&b.rotation);

    let dr = to_axis_angle(
        &(a_r.inverse().unwrap() * b_r)
    );

    let angle = dr.norm().abs() * 180.0 / std::f64::consts::PI;

    if angle > 20.0 {
        return true;
    }


    false
}

struct FrameData {
    cameras: HashMap<u64, FrameCameraData, FastHasherBuilder>

}

#[derive(Debug)]
struct FrameCameraData {
    pattern: BlobPatternMatch,
    points_2d: Vec<Vector2d>,
    points_3d: Vec<Vector3d>,
}


