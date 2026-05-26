
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;
use std::collections::HashMap;

use common::errors::*;
use common::io::{Readable, Writeable};
use common::hash::FastHasherBuilder;
use executor::bundle::TaskResultBundle;
use executor::FileHandle;
use file::LocalPathBuf;
use macros::executor_main;
use peripherals::serial::{SerialPort, SerialOptions};
use mocap_camera::frame_processor::*;
use math::array::Array;
use image::Colorspace;
use file::{project_path, LocalPath};
use mocap_proto::mocap::*;
use sstable::record_log::RecordReader;
use protobuf::{StaticMessage, Message};
use protobuf_json::MessageJsonSerialize;
use math::matrix::axis_angle::*;
use math::matrix::{vec2f, vec3f, Vector2f, Matrix3f, Vector3f, Matrix4f};
use vision::{CameraIntrinsicsModel, CameraExtrinsics, BundleAdjustmentSolver};
use cluster_client::id::{entity_id_from_string, entity_id_to_string};

use crate::wand::*;
use crate::matching::CameraParameters;
use crate::proto_utils::*;


/*
TODO: Eventually do full spatial dedupping instead of just looking at the previous matched frame.
*/


const CAMERAS_HEIGHT: f32 = 2.0;  // meters


pub struct MocapCameraExtrinsicsCalibrator<'a> {
    config: &'a MocapManagerConfig,
    cameras_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
    initial_system_state: &'a MocapManagerStatus,
    num_cameras: usize,
}

impl<'a> MocapCameraExtrinsicsCalibrator<'a> {

    pub fn calibrate(
        config: &'a MocapManagerConfig,
        entries: &'a [MocapLogEntry]
    ) -> Result<HashMap<u64, CameraExtrinsics, FastHasherBuilder>> {

        let mut cameras_intrinsics = HashMap::<u64, CameraIntrinsicsModel, FastHasherBuilder>::default();

        for per_cam in config.per_camera() {
            let camera_id = entity_id_from_string(per_cam.camera_id_str()).unwrap();
            cameras_intrinsics.insert(camera_id, intrinsics_from_proto(per_cam.intrinsics()));
        }


        let initial_system_state = {
            let first_entry = &entries[0];
            if !first_entry.has_system_state() {
                return Err(err_msg("Expected first entry to have system_state"));
            }

            first_entry.system_state()
        };

        let num_cameras = initial_system_state.cameras().len();

        let inst = Self {
            config,
            cameras_intrinsics,
            num_cameras,
            initial_system_state,
        };

        let mut data = inst.find_wands(entries)?;

        let mut data = inst.select_frame_subset(&mut data)?;

        println!("Num cams: {}", num_cameras);

        println!("Remaining entries: {}", data.len());

        if data.len() == 0 {
            return Err(err_msg("No data with good enough matches"));
        }

        // Initialize rough camera extrinsics guesses.
        let mut camera_initial_extrinsics = inst.extract_initial_intrinsics(&mut data)?;

        let mut camera_extrinsics = inst.optimize_extrinsics(&data, &camera_initial_extrinsics)?;

        inst.align_extrinsics(&mut camera_extrinsics)?;

        Ok(camera_extrinsics)
    }

    // 
    fn find_wands(&self, entries: &[MocapLogEntry]) -> Result<Vec<FrameData>> {
        // TODO: Only need to preserve the extrinsic parameters in this.
        let mut last_match_per_camera = HashMap::<u64, BlobPatternMatch, FastHasherBuilder>::default();

        let mut data: Vec<FrameData> = vec![];

        for (i, entry) in entries.iter().enumerate() {
            if i % 1000 == 0 {
                println!("- {} / {}", i, entries.len());
            }

            // At least two cameras need to be involved for correlating relative
            // positions between cameras.
            if entry.blobs().cameras().len() < 2 {
                continue;
            }

            let mut cameras_data = HashMap::default();

            let mut deduped = false;

            for cam in entry.blobs().cameras() {
                let intrinsics = self.cameras_intrinsics.get(&cam.camera_id())
                    .ok_or_else(|| format_err!(
                        "Missing intrinsics for camera: {}",
                        entity_id_to_string(cam.camera_id()).unwrap()
                    ))?;

                let m = match BlobPatternFinder::find_t_wand(self.config.wand(), intrinsics, cam.results()) {
                    Some(v) => v,
                    None => continue
                };

                if m.total_reprojection_error > 2.0 {
                    continue;
                }


                // TODO: If there are multiple frames nearby, pick the one with the lowest reprojection error.
                if let Some(last_match) = last_match_per_camera.get(&cam.camera_id()) {
                    if !pattern_positions_distinct(&m, last_match) {
                        deduped = true;
                        break;
                    }
                }

                let mut points_2d = vec![];
                let mut points_3d = vec![];
                for p in &m.points {
                    points_3d.push(p.reference_point.clone());

                    let blob = &cam.results().blobs()[p.blob_index];
                    points_2d.push(vec2f(blob.x(), blob.y()));
                }

                cameras_data.insert(cam.camera_id(), FrameCameraData {
                    points_2d,
                    points_3d,
                    pattern: m
                });
            }

            if deduped {
                continue;
            }

            if cameras_data.len() < 2 {
                continue;
            }

            // Update last position per camera for future debupping.
            for (cam_id, data) in &cameras_data {
                last_match_per_camera.insert(*cam_id, data.pattern.clone());
            }

            data.push(FrameData {
                cameras: cameras_data
            });
        }

        Ok(data)
    }

    fn select_frame_subset(&self, frames: &mut Vec<FrameData>) -> Result<Vec<FrameData>> {
        const CROSS_CAMERA_WEIGHT: f32 = 0.1;

        const SINGLE_CAMERA_WEIGHT: f32 = 10.0;

        const PAIR_WEIGHT: f32 = 20.0;

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

        while frames.len() > 0 && out.len() < TARGET_SUBSET_SIZE {

            let mut best_score = 0.0;
            let mut best_i = 0;

            for i in 0..frames.len() {

                let frame = &frames[i];

                let score = {
                    
                    let mut score = 0.0;
                
                    score += CROSS_CAMERA_WEIGHT * (frame.cameras.len() as f32);

                    for cam_id in frame.cameras.keys() {
                        let c = camera_counts.get(cam_id).cloned().unwrap_or(0);
                        score += SINGLE_CAMERA_WEIGHT * ((SINGLE_CAMERA_TARGET_OBSERVATIONS as f32) - (c as f32)).max(0.0);
                    }

                    for cam_id1 in frame.cameras.keys() {
                        for cam_id2 in frame.cameras.keys() {
                            if cam_id1 >= cam_id2 {
                                continue;
                            }

                            let c = camera_coincidences.get(&(*cam_id1, *cam_id2)).cloned().unwrap_or(0);
                            score += PAIR_WEIGHT * ((PAIR_TARGET_COINCIDENCES as f32) - (c as f32)).max(0.0);
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

    fn extract_initial_intrinsics(
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
                    let rel_mat = cam_local_mat * known_cam_local_mat.inverse();

                    let global_mat = rel_mat * &known_cam_global_mat;

                    let (r, t) = extrinsics_from_mat4x4(&global_mat);
                    camera_initial_extrinsics.insert(*cam_id, CameraExtrinsics {
                        rotation: r,
                        translation: t
                    });
                    changed = true;
                }
            }
        }

        println!("Num initial extrinsics: {}", camera_initial_extrinsics.len());

        for cam in self.initial_system_state.cameras() {
            if !camera_initial_extrinsics.contains_key(&cam.id()) {
                eprintln!("Missing camera: {} ({:?})", cam.id(), entity_id_to_string(cam.id()));
            }
        }

        // TODO: Also verify no unknown cameras are present in the data but not in the initial set.
        if camera_initial_extrinsics.len() != self.num_cameras {
            return Err(err_msg("Not all cameras are linked by some frame chain."));
        }

        Ok(camera_initial_extrinsics)
    }


    fn optimize_extrinsics(
        &self,
        data: &[FrameData],
        camera_initial_extrinsics: &HashMap<u64, CameraExtrinsics, FastHasherBuilder>,
    ) -> Result<HashMap<u64, CameraExtrinsics, FastHasherBuilder>> {
        // TODO: Use something like a Huber/Cauchy loss to reduce sensitivity to outliers if we
        // messed up want detection.
        let mut solver = BundleAdjustmentSolver::new();

        let mut camera_id_to_index = HashMap::<u64, usize>::default();

        for (id, extrinsics) in camera_initial_extrinsics {
            // TODO: Chose whichever camera has the most coincidences with other cameras.
            let fixed = camera_id_to_index.is_empty();

            let idx = solver.add_camera(
                self.cameras_intrinsics.get(id).unwrap(),
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
                
                let object_transform = initial_camera_transform.inverse() * estimated_transform;

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

        let solution = solver.solve();

        println!("Solved!");


        let mut camera_extrinsics = HashMap::<u64, CameraExtrinsics, FastHasherBuilder>::default();

        for (cam_id, cam_idx) in camera_id_to_index {
            let extrinsics = solution.camera_extrinsics(cam_idx);
            camera_extrinsics.insert(cam_id, extrinsics);
        }

        Ok(camera_extrinsics)
    }

    // MAkes 
    fn align_extrinsics(
        &self,
        camera_extrinsics: &mut HashMap<u64, CameraExtrinsics, FastHasherBuilder>
    ) -> Result<()> {
        let mut up_vector = Vector3f::zero();
        let mut num_cams = 0;

        for cam in self.initial_system_state.cameras() {
            let cam_id = cam.id();

            let extrinsics = camera_extrinsics.get(&cam_id)
                .ok_or_else(|| err_msg("Camera not declared in system_state"))?;
            let r = from_axis_angle(&extrinsics.rotation);

            if !cam.camera_status().accelerometer().has_value() {
                return Err(err_msg("Camera missing accelerometer vector"));
            }

            let proto = cam.camera_status().accelerometer().value();

            let v: Vector3f = r.transpose() * vec3f(proto.x(), proto.y(), proto.z()).normalized();
            up_vector += v;
            num_cams += 1;
        }

        up_vector /= (num_cams as f32);
        up_vector.normalize();

        
        let r_align = {
            let z = vec3f(0., 0., 1.);
            let mut axis = up_vector.cross(&z).normalized();
            let angle = up_vector.dot(&z).acos();
            axis *= angle;
            from_axis_angle(&axis)
        };

        let t_align = {

            let mut cam_center_pos = vec3f(0., 0., 0.);
            let mut num_cams = 0;
            let mut z_max = -1000000.0f32;
            let mut z_min = 1000000.0f32;

            for extrinsics in camera_extrinsics.values() {
                let pos = &r_align * extrinsics.position();
                z_max = z_max.max(pos.z());
                z_min = z_min.min(pos.z());
                cam_center_pos += pos;
                num_cams += 1;
            }

            cam_center_pos /= (num_cams as f32);

            let t_z = 2.0 - z_max;

            vec3f(
                -cam_center_pos.x(),
                -cam_center_pos.y(),
                t_z
            )
        };

        let align_mat = {
            let mut out = Matrix4f::zero();
            out.block_mut(0, 0).copy_from(&r_align.transpose());
            out.block_mut(0, 3).copy_from(&(
                r_align.transpose() * t_align * -1.0   
            ));
            out[(3, 3)] = 1.0;
            out
        };

        for extrinsics in camera_extrinsics.values_mut() {

            let (r, t) = extrinsics_from_mat4x4(&(
                extrinsics.to_mat4x4() * &align_mat
            ));

            extrinsics.rotation = r;
            extrinsics.translation = t;
        }

        Ok(())
    }


}

fn extrinsics_from_mat4x4(mat: &Matrix4f) -> (Vector3f, Vector3f) {
    let rot: Matrix3f = mat.block(0, 0).to_owned();

    let translation = mat.block(0, 3).to_owned();

    (to_axis_angle(&rot), translation)
}


fn pattern_positions_distinct(a: &BlobPatternMatch, b: &BlobPatternMatch) -> bool {
    let dt = (&a.translation - &b.translation).norm();
    if dt > 0.05 { // 50mm
        return true;
    }

    let a_r = from_axis_angle(&a.rotation);
    let b_r = from_axis_angle(&b.rotation);

    let dr = to_axis_angle(
        &(a_r.inverse() * b_r)
    );

    let angle = dr.norm().abs() * 180.0 / std::f32::consts::PI;

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
    points_2d: Vec<Vector2f>,
    points_3d: Vec<Vector3f>,
}


