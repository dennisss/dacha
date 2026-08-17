use std::collections::{HashMap, HashSet};
use std::time::Duration;

use common::hash::*;
use common::errors::*;
use math::matrix::{Vector3d, VectorXd, MatrixXd};
use math_proto_util::VectorProtoExt;
use mocap_proto::mocap::*;
use math::geometry::bounding_box::*;
use math::matrix::cwise_binary_ops::CwiseDiv;
use math::matrix::cwise_binary_ops::{CwiseMulAssign, CwiseDivAssign};
use math::matrix::cwise_binary_ops::CwiseMul;

use crate::skeleton::inst::*;
use crate::skeleton::solver::*;
use crate::skeleton::tree::*;
use crate::skeleton::model::*;
use crate::matching::*;
use crate::rigid_transform::*;

/*
Matching to skeletons:

TODO: Need to fast snap the translation vector.

TODO: Need to support saving / restoring skeleton data from the config file.

TODO: Implement re-matching lost points.

TODO: Need to bound the amount we are allowed to move between frames (especially if part of the skeleton temporarily becomes underconstrained) and then re-match occluded points when they become available again
    - Except we can ignore this check for the first frame when transitioning to live.

*/

#[derive(Default)]
pub struct SkeletonTracker {
    config: SkeletonTrackerConfig,
    skeletons: Vec<SkeletonState>,
}

struct SkeletonState {
    skeleton: Skeleton,
    skeleton_tree: SkeletonTree,
    mode: Mode,
}

enum Mode {
    Lost,
    Searching(SearchingMode),
    Live(LiveMode)
}

struct SearchingMode {
    last_scan_time: u64,
    default_joints: SkeletonJointsState,
    candidate: Option<SearchCandidate>,
}

struct SearchCandidate {
    start_time: u64,
    end_time: u64,
    marker_ids: Vec<u64>,
    marker_positions: Vec<Vector3d>,
}

struct LiveMode {
    marker_ids: Vec<Option<u64>>,
    joints: SkeletonJointsState
}


impl SkeletonTracker {

    pub fn set_config(&mut self, config: SkeletonTrackerConfig) -> Result<()> {
        self.config = config;

        self.skeletons.clear();

        for proto in self.config.skeletons() {
            // TODO: Need to implement full saving/restoring of skeleton states from config.
            let mut skeleton = standard_skeleton();
            skeleton.id = proto.id();

            let skeleton_tree = SkeletonTree::new(&skeleton);

            self.skeletons.push(SkeletonState {
                skeleton,
                skeleton_tree,
                mode: Mode::Lost,
            });
        }

        Ok(())
    }

    // NOTE: Currently this will just contain skeletons.
    pub fn config_patch(&self) -> SkeletonTrackerConfig {
        let mut out = SkeletonTrackerConfig::default();

        for skel in &self.skeletons {
            out.add_skeletons(skel.skeleton.to_proto());
        }
        
        out
    }


    pub fn set_skeleton_searching(&mut self, id: u32, searching: bool) {

        for state in self.skeletons.iter_mut() {
            if state.skeleton.id != id {
                continue;
            }

            if searching {
                match &state.mode {
                    Mode::Searching(_) => {
                        // Nothing to do.
                        return;
                    },
                    _ => {}
                }

                state.mode = Mode::Searching(SearchingMode {
                    last_scan_time: 0,
                    default_joints: SkeletonJointsState::zero(&state.skeleton),
                    candidate: None
                });

            } else {
                match &state.mode {
                    Mode::Searching(_) => {
                        state.mode = Mode::Lost;
                    }
                    _ => {}
                }
            }

            break;
        }
    }

    pub fn to_state_protos(&self) -> Vec<SkeletonStateProto> {
        let mut out = vec![];

        for state in &self.skeletons {
            let mut proto = SkeletonStateProto::default();
            proto.set_id(state.skeleton.id);

            match &state.mode {
                Mode::Lost => {
                    proto.set_mode(SkeletonStateProto_Mode::LOST);
                }
                Mode::Searching(searching) => {
                    proto.set_mode(SkeletonStateProto_Mode::SEARCHING);
                    proto.set_joints(searching.default_joints.to_proto());

                    proto.set_calculated_positions(
                        Self::calculate_skeleton_positions(state, &searching.default_joints)
                    );
                }

                Mode::Live(live) => {
                    proto.set_mode(SkeletonStateProto_Mode::LIVE);

                    proto.marker_ids_mut().extend(
                        live.marker_ids.iter().clone().map(|i| i.unwrap_or(0)));

                    proto.set_joints(live.joints.to_proto());

                    proto.set_calculated_positions(
                        Self::calculate_skeleton_positions(state, &live.joints)
                    );
                }
            }

            out.push(proto);
        }

        out
    }

    fn calculate_skeleton_positions(
        state: &SkeletonState, joints: &SkeletonJointsState
    ) -> SkeletonCalculatedPositions {
        let mut proto = SkeletonCalculatedPositions::default();

        let mut bone_positions = vec![Vector3d::zero(); state.skeleton.bones.len()];
        let mut marker_positions = vec![Vector3d::zero(); state.skeleton.markers.len()];
        state.skeleton_tree.forward_kinematics(
            joints,
            &mut bone_positions,
            &mut marker_positions
        );

        for bone_i in 0..state.skeleton.bones.len() {
            let end_pos = bone_positions[bone_i].clone();

            let start_pos = match state.skeleton.bones[bone_i].parent.clone() {
                Some(parent_i) => {
                    bone_positions[parent_i].clone()
                } 
                None => joints.translation.clone()
            };

            proto.add_start(start_pos.to_proto());
            proto.add_end(end_pos.to_proto());
        }

        for m in marker_positions {
            proto.add_markers(m.to_proto());
        }

        proto
    }

    pub fn run(&mut self, timestamp: u64, points: &[TrackedPoint]) {
        // TODO: Also exit early if all skeletons are lost.
        if self.skeletons.is_empty() {
            return;
        }

        let mut points_by_id = HashMap::<u64, usize, FastHasherBuilder>::default();
        for (i, point) in points.iter().enumerate() {
            points_by_id.insert(point.id, i);
        }

        let mut used_point_ids = HashSet::<u64, FastHasherBuilder>::default();
        for state in &self.skeletons {
            match &state.mode {
                Mode::Live(live) => {
                    for id in &live.marker_ids {
                        if let Some(id) = id {
                            used_point_ids.insert(*id);
                        }
                    }
                }
                _ => {}
            }
        }

        for state in &mut self.skeletons {

            match &mut state.mode {
                Mode::Lost => {},
                Mode::Searching(searching) => {

                    let min_scan_period = Duration::from_secs_f32(
                        1.0 / (self.config.search().max_skeleton_scan_fps() as f32)
                    ).as_nanos() as u64;

                    if timestamp - searching.last_scan_time < min_scan_period {
                        continue;
                    }

                    searching.last_scan_time = timestamp;

                    let expected_marker_positions = {
                        let mut bone_positions = vec![Vector3d::zero(); state.skeleton.bones.len()];
                        let mut marker_positions = vec![Vector3d::zero(); state.skeleton.markers.len()];
                        state.skeleton_tree.forward_kinematics(
                            &searching.default_joints,
                            &mut bone_positions,
                            &mut marker_positions
                        );

                        marker_positions
                    };

                    // Find all markers reasonably close to the model marker points.
                    let mut found_marker_ids = HashSet::<u64, FastHasherBuilder>::default();
                    let max_model_point_distance_squared = squared(self.config.search().max_model_point_distance());
                    for expected_pos in &expected_marker_positions {
                        for pt in points {
                            if (&pt.position - expected_pos).norm_squared() <= max_model_point_distance_squared {
                                found_marker_ids.insert(pt.id);
                            }
                        }
                    }

                    if found_marker_ids.len() != expected_marker_positions.len() {
                        searching.candidate = None;
                        continue;
                    }

                    let mut marker_ids = found_marker_ids.iter().cloned().collect::<Vec<u64>>();
                    marker_ids.sort();

                    let mut marker_positions = vec![];
                    for id in &marker_ids {
                        marker_positions.push(points[*points_by_id.get(id).unwrap()].position.clone());
                    }

                    if let Some(candidate) = &mut searching.candidate {
                        if candidate.marker_ids == marker_ids {

                            let mut all_close = true;
                            for i in 0..marker_ids.len() {
                                if (&marker_positions[i] - &candidate.marker_positions[i]).norm() > self.config.search().max_standstill_distance() {
                                    all_close = false;
                                    break;
                                }
                            }

                            if all_close {
                                candidate.end_time = timestamp;

                                let min_standstill = Duration::from_secs_f64(
                                    self.config.search().min_standstill_time_secs()
                                ).as_nanos() as u64;

                                if candidate.end_time - candidate.start_time >= min_standstill {
                                    let (fit_skeleton, fit_marker_ids) = Self::fit_matched_skeleton(
                                        marker_ids,
                                        marker_positions,
                                        expected_marker_positions,
                                        &state.skeleton
                                    );

                                    state.mode = Mode::Live(LiveMode {
                                        // TODO: Use the best fit from find_rigid_transform.
                                        joints: searching.default_joints.clone(),
                                        marker_ids: fit_marker_ids.into_iter().map(|i| Some(i)).collect()
                                    });

                                    // TODO: These need to be pushed back into the config.
                                    state.skeleton = fit_skeleton;
                                    state.skeleton_tree = SkeletonTree::new(&state.skeleton);
                                    println!("MATCH SKEL");
                                }

                                continue;
                            }
                        }
                    }


                    searching.candidate = Some(SearchCandidate {
                        start_time: timestamp,
                        end_time: timestamp,
                        marker_ids,
                        marker_positions
                    });
                }

                Mode::Live(live) => {
                    // Gather all known points.

                    let mut markers = vec![];

                    // TODO: Weight the error in the solver by the tracking confidence (mainly whether or not the marker is a ghost).
                    for (i, id) in live.marker_ids.iter().cloned().enumerate() {
                        let id = match id {
                            Some(v) => v,
                            None => continue
                        };

                        let point_idx = match points_by_id.get(&id) {
                            Some(v) => *v,
                            None => continue
                        };

                        let point = &points[point_idx];

                        // TODO: Decrease the weight of these in the solver.
                        if point.camera_ids.is_empty() {
                            continue;
                        }

                        markers.push((i, point.position.clone()));
                    }

                    // TODO: Exit if not enough points are visible
                    // (at some point, kill the skeleton)

                    if markers.len() < live.marker_ids.len() - 4 {
                        println!("NOT ENOUGH MARKERS. ABORT.");
                        state.mode = Mode::Lost;
                        continue; 
                    }

                    // Update 

                    let new_joints = solve_skeleton_joints_state(
                        &state.skeleton, &state.skeleton_tree, &live.joints, &markers
                    );
                    
                    live.joints = new_joints;

                    for i in 0..live.marker_ids.len() {
                        if let Some(id) = live.marker_ids[i] {
                            if let Some(_) = points_by_id.get(&id) {
                                continue;
                            }
                        }

                        // Clearing if it has an id but the point is missing.
                        live.marker_ids[i] = None;
                    }



                    /*
                    if markers.len() < live.marker_ids.len() {
                        // let mut bone_positions = vec![Vector3d::zero(); state.skeleton.bones.len()];
                        // let mut marker_positions = vec![Vector3d::zero(); state.skeleton.markers.len()];
                        // state.skeleton_tree.forward_kinematics(
                        //     &live.joints,
                        //     &mut bone_positions,
                        //     &mut marker_positions
                        // );

                        for i in 0..live.marker_ids.len() {
                            if let Some(id) = live.marker_ids[i] {
                                if let Some(_) = points_by_id.get(&id) {
                                    continue;
                                }
                            }

                            // Clearing if it has an id but the point is missing.
                            // live.marker_ids[i] = None;

                            /*
                            // TODO: Verify there is no match ambiguity (must be no second best that is also good)
                            let expected_position = &marker_positions[i];

                            for pt in points {
                                if used_point_ids.contains(&pt.id) {
                                    continue;
                                }

                                if (&pt.position - expected_position).norm() < 0.1 {
                                    live.marker_ids[i] = Some(pt.id);
                                    break;
                                }
                            }
                            */
                        }
                    }
                    */

                }
            }
        }
    }

    pub fn backpropagate_predicted_points(&mut self, matcher: &mut BlobMatcher) {
        for state in &mut self.skeletons {
            match &mut state.mode {
                Mode::Live(live) => {
                    let mut bone_positions = vec![Vector3d::zero(); state.skeleton.bones.len()];
                    let mut marker_positions = vec![Vector3d::zero(); state.skeleton.markers.len()];
                    state.skeleton_tree.forward_kinematics(
                        &live.joints,
                        &mut bone_positions,
                        &mut marker_positions
                    );

                    for i in 0..live.marker_ids.len() {
                        if let Some(id) = live.marker_ids[i] {
                            matcher.add_position_observation(id, marker_positions[i].clone());
                        } else {
                            live.marker_ids[i] = Some(matcher.add_ghost(marker_positions[i].clone()));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn fit_matched_skeleton(
        marker_ids: Vec<u64>,
        marker_positions: Vec<Vector3d>,
        expected_marker_positions: Vec<Vector3d>,
        skeleton: &Skeleton,
    ) -> (Skeleton, Vec<u64>) {
        // Optimize the assignment of real markers to markers
        // in the skeleton model. 
        let (marker_ids, marker_positions) = {
            let mut weights = MatrixXd::zero_with_shape(marker_ids.len(), marker_ids.len());

            for i in 0..weights.rows() {
                for j in 0..weights.cols() {
                    weights[(j, i)] = (&marker_positions[i] - &expected_marker_positions[j]).norm();
                }
            }

            let mut assignments = vec![];

            let mut solver = math::assignment_solver::AssignmentSolver::new();
            let _ = solver.solve(&weights, &mut assignments);


            let mut new_marker_ids = vec![];
            let mut new_marker_positions = vec![];
            for i in 0..marker_ids.len() {
                let idx = assignments[i].unwrap();
                new_marker_ids.push(marker_ids[idx]);
                new_marker_positions.push(marker_positions[idx].clone());
            }

            (new_marker_ids, new_marker_positions)
        };

        let mut model_marker_positions = vec![];
        for marker in &skeleton.markers {
            model_marker_positions.push(marker.position.clone());
        }

        // Find the best transform that transforms the real markers back into the skeleton
        // model's coordinate system. 
        let (r, t) = find_rigid_transform(&marker_positions, &model_marker_positions, &[]);


        let marker_positions = {
            let mut new_points = vec![];

            for pt in marker_positions {
                new_points.push(&r * pt + &t);
            }

            new_points
        };

        let mut scale = Vector3d::zero();
        let mut scale_count = Vector3d::zero();

        for i in 0..marker_positions.len() {
            let a = &marker_positions[i];
            let b = &model_marker_positions[i];

            for j in 0..3 {
                if b[j].abs() > 0.0001 {
                    scale[j] += (a[j] / b[j]) * b[j].abs();
                    scale_count[j] += b[j].abs();
                }
            }
        }

        scale.cwise_div_assign(&scale_count);


        // for i in 0..3 {
        //     scale[i] = (scale[i] - 1.0) * 0.1 + 1.0;
        // }



        /*
        let data_size = {
            // TODO: Make this support f64 directly without casts.
            let mut b = BoundingBoxBuilder::new();
            for p in &marker_positions {
                b.update(&p.cast());
            }

            let b = b.build();

            b.max - b.min 
        };

        let model_size = {
            let mut b = BoundingBoxBuilder::new();
            for p in &model_marker_positions {
                b.update(&p.cast());
            }

            let b = b.build();

            b.max - b.min
        };

        let scale: Vector3d = data_size.cwise_div(&model_size).cast();
        */

        let mut new_skeleton = skeleton.clone();
        
        // TODO: Also update the default translation?

        for bone in &mut new_skeleton.bones {
            bone.end_position.cwise_mul_assign(&scale);
        }

        for i in 0..marker_positions.len() {
            if skeleton.markers[i].name.contains("TORSO") {
                new_skeleton.markers[i].position = marker_positions[i].clone();
            } else {
                new_skeleton.markers[i].position.cwise_mul_assign(&scale);
            }


            // 
        }

        (new_skeleton, marker_ids)
    }
}

fn squared(v: f64) -> f64 {
    v * v
}

