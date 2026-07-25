use std::f64::consts::PI;
use std::collections::HashMap;

use common::hash::FastHasherBuilder;
use math::matrix::{Vector3d, vec3d, Matrix4d, Vector4d};
use math::matrix::axis_angle::*;

use crate::skeleton::inst::*;


/// Tree based representation of a skeleton definition.
/// 
/// This allows for computing bone positions through top down tree
/// traversal. 
pub struct SkeletonTree {
    num_bones: usize,

    num_markers: usize,

    children: Vec<SkeletonNode>,
}

pub struct SkeletonNode {
    bone: Bone,

    children: Vec<SkeletonNode>,

    /// These are markers attached to the current bone.
    markers: Vec<BoneMarker>,
}


impl SkeletonTree {

    pub fn new(skeleton: &Skeleton) -> Self {

        let mut nodes = vec![];
        for bone in &skeleton.bones {
            nodes.push(Some(SkeletonNode {
                bone: bone.clone(),
                children: vec![],
                markers: vec![]
            }));
        }

        for marker in &skeleton.markers {
            nodes[marker.bone_index].as_mut().unwrap().markers.push(marker.clone());
        }

        let mut num_children = vec![0; skeleton.bones.len()];
        for bone in &skeleton.bones {
            if let Some(parent) = bone.parent {
                num_children[parent] += 1;
            }
        }

        let mut root_nodes = vec![];

        let mut changed = true;
        while changed {
            changed = false;

            for i in 0..nodes.len() {
                if nodes[i].is_some() && num_children[i] == 0 {
                    let node = nodes[i].take().unwrap();
                    if let Some(parent) = node.bone.parent {
                        nodes[parent].as_mut().unwrap().children.push(node);
                        num_children[parent] -= 1;
                    } else {
                        root_nodes.push(node);
                    }

                    changed = true;
                }
            }
        }

        Self {
            num_bones: skeleton.bones.len(),
            num_markers: skeleton.markers.len(),
            children: root_nodes
        }
    }

    pub fn forward_kinematics(
        &self,
        state: &SkeletonJointsState,
        bone_positions: &mut [Vector3d],
        marker_positions: &mut [Vector3d]
    ) {
        let base_transform = extrinsics_mat(&state.rotation, &state.translation);
        
        for child in &self.children {
            self.forward_kinematics_inner(child, &base_transform, state, bone_positions, marker_positions);
        }
    }

    // pub fn forward_kinematics_with_gradients(
    //     &self,
    //     state: SkeletonJointsState,
    //     bone_positions: &mut [Vector3d],
    //     marker_positions: &mut [Vector3d]
    // )

    fn forward_kinematics_inner(
        &self,
        node: &SkeletonNode,
        parent_transform: &Matrix4d,
        state: &SkeletonJointsState,
        bone_positions: &mut [Vector3d],
        marker_positions: &mut [Vector3d]
    ) {
        if !bone_positions.is_empty() {
            bone_positions[node.bone.index] = apply_transform(
                parent_transform, &node.bone.end_position
            );
        }

        for marker in &node.markers {
            marker_positions[marker.index] = apply_transform(
                parent_transform, &marker.position
            );
        }

        // Skip calculating the next transform if we don't need it.
        if node.children.is_empty() {
            return;
        }

        /*
        let cur_transform = (
            translate(&node.bone.end_position) *
            rotate(&state.bone_rotations[node.bone.index]) *
            translate(&(node.bone.end_position.clone() * -1.0))
        );
        */
        let cur_transform = {
            let mut t = node.bone.end_position.clone() * -1.0;
            let r = from_axis_angle(&state.bone_rotations[node.bone.index]);
            t = &r * t;
            t += &node.bone.end_position;

            let mut out = Matrix4d::zero();
            out.block_mut(0,0).copy_from(&r);
            out.block_mut(0, 3).copy_from(&t);
            out[(3, 3)] = 1.0;
            out
        };


        let child_transform = parent_transform * cur_transform;

        for child in &node.children {
            self.forward_kinematics_inner(child, &child_transform, state, bone_positions, marker_positions);
        }
    }
}

fn extrinsics_mat(rotation: &Vector3d, translation: &Vector3d) -> Matrix4d {
    let mut out = Matrix4d::zero();
    out.block_mut(0, 0).copy_from(&from_axis_angle(rotation));
    out.block_mut(0, 3).copy_from(translation);
    out[(3, 3)] = 1.0;
    out

}


fn apply_transform(mat: &Matrix4d, v: &Vector3d) -> Vector3d {
    let v = mat * Vector4d::from_slice(&[ v.x(), v.y(), v.z(), 1. ]);
    vec3d(v.x(), v.y(), v.z())
}

fn translate(translation: &Vector3d) -> Matrix4d {
    let mut out = Matrix4d::identity();
    out.block_mut(0, 3).copy_from(translation);
    out
} 

fn rotate(rotation: &Vector3d) -> Matrix4d {
    let mut out = Matrix4d::identity();
    out.block_mut(0, 0).copy_from(&from_axis_angle(rotation));
    out
}