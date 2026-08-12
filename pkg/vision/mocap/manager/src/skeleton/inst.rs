use std::f64::consts::PI;

use math::matrix::Vector3d;
use mocap_proto::mocap::*;
use math_proto_util::VectorProtoExt;

/// This is a model of a multi-joint/limb skeleton.
///
/// - This just contains the bone data in a normalized orientation with
///   rotation constraints.
/// - The 'root' node of the skeleton is located at (0,0,0) and has 6 DOF.
/// - Bones start at the position of their parent and end at their 'end_position'
/// - Bones rotate around their 'end_position'
/// - When creating bones, they by default have 
#[derive(Default, Clone)]
pub struct Skeleton {
    pub id: u32,

    /// List of top level bones.
    /// The start_position of each of these bones will be (0,0,0)
    pub bones: Vec<Bone>,

    pub markers: Vec<BoneMarker>,

    /// TODO: Serialize me.
    pub default_translation: Vector3d,
}

impl Skeleton {
    pub fn add_bone<S: AsRef<str>>(&mut self, name: S, end_position: Vector3d) -> &mut Bone {
        let index = self.bones.len();
        self.bones.push_mut(Bone::new(index, name.as_ref().to_string(), end_position))
    }

    pub fn add_marker<S: AsRef<str>>(&mut self, name: S, bone_index: usize, position: Vector3d) {
        let index = self.markers.len();
        self.markers.push(BoneMarker {
            index,
            name: name.as_ref().to_string(),
            bone_index,
            position
        });
    }

    pub fn marker_index(&self, name: &str) -> usize {
        self.markers.iter().find(|m| m.name == name).unwrap().index
    }

    pub fn bone_start_position(&self, index: usize) -> Vector3d {
        if let Some(parent) = self.bones[index].parent {
            self.bones[index].end_position.clone()
        } else {
            Vector3d::zero()
        }
    }

    pub fn bone_end_position(&self, index: usize) -> Vector3d {
        self.bones[index].end_position.clone()
    }

    pub fn to_proto(&self) -> SkeletonProto {
        let mut proto = SkeletonProto::default();
        proto.set_id(self.id);
        for bone in &self.bones {
            proto.add_bones(bone.to_proto());
        }
        proto
    }
}


#[derive(Clone)]
pub struct Bone {
    pub index: usize,

    pub name: String,
    
    pub end_position: Vector3d,

    pub parent: Option<usize>,

    /// Constraints for each of the x,y,z axes.
    /// Default to unconstrained [-180, 180] degrees.
    pub rotation_constraints: Vec<AngleConstraint>,
}

impl Bone {

    fn new(index: usize, name: String, end_position: Vector3d) -> Self {
        Self {
            index,
            name,
            end_position,
            parent: None,
            rotation_constraints: vec![
                AngleConstraint::unconstrained(); 3
            ]
        }
    }

    pub fn to_proto(&self) -> BoneProto {
        let mut proto = BoneProto::default();
        proto.set_name(&self.name);
        if let Some(parent) = &self.parent {
            proto.set_parent(*parent as u32);   
        }
        proto.set_end_position(self.end_position.to_proto());
        proto
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn set_parent(&mut self, parent_index: usize) -> &mut Self { 
        self.parent = Some(parent_index);
        self
    }

    pub fn disable_rotation(&mut self) -> &mut Self {
        for c in &mut self.rotation_constraints {
            c.min = 0.0;
            c.max = 0.0;
        }

        self
    }

    pub fn disable_axis_rotation(&mut self, index: usize) -> &mut Self {
        let c = &mut self.rotation_constraints[index];
        c.min = 0.0;
        c.max = 0.0;
        self
    }

    pub fn set_axis_rotation_limits(&mut self, index: usize, min: f64, max: f64) -> &mut Self {
        let c = &mut self.rotation_constraints[index];
        c.min = min;
        c.max = max;
        self
    }
}

#[derive(Clone)]
pub struct AngleConstraint {
    pub min: f64,
    pub max: f64
}

impl AngleConstraint {
    pub fn unconstrained() -> Self {
        Self { min: -PI, max: PI }
    }

    pub fn is_fixed(&self) -> bool {
        self.min == self.max
    }
}


#[derive(Clone)]
pub struct BoneMarker {
    pub index: usize,
    pub name: String,
    pub bone_index: usize,
    pub position: Vector3d,
}

#[derive(Clone)]
pub struct SkeletonJointsState {
    pub translation: Vector3d,
    pub rotation: Vector3d,
    pub bone_rotations: Vec<Vector3d>,
}

impl SkeletonJointsState {
    pub fn zero(skeleton: &Skeleton) -> Self {
        Self {
            translation: skeleton.default_translation.clone(),
            rotation: Vector3d::zero(),
            bone_rotations: vec![ Vector3d::zero(); skeleton.bones.len() ]
        }
    }

    pub fn to_proto(&self) -> SkeletonJointsStateProto {
        let mut proto = SkeletonJointsStateProto::default();
        proto.set_translation(self.translation.to_proto());
        proto.set_rotation(self.rotation.to_proto());
        for r in &self.bone_rotations {
            proto.add_bone_rotations(r.to_proto());
        }
        proto
    }

    pub fn serialize(&self, skeleton: &Skeleton) -> Vec<f64> {
        let mut out = vec![];
        out.extend_from_slice(self.translation.as_ref());
        out.extend_from_slice(self.rotation.as_ref());

        for (bone_i, rot) in self.bone_rotations.iter().enumerate() {
            for i in 0..3 {
                if skeleton.bones[bone_i].rotation_constraints[i].is_fixed() {
                    continue;
                }

                out.push(rot[i]);
            }
        }

        out
    }

    pub fn parse(values: &[f64], skeleton: &Skeleton) -> Self {
        let translation = Vector3d::from_slice(&values[0..3]);
        let rotation = Vector3d::from_slice(&values[3..6]);

        let mut bone_rotations = vec![];
        bone_rotations.reserve_exact(skeleton.bones.len());
        
        let mut input_i = 6;

        for (bone_i, bone) in skeleton.bones.iter().enumerate() {
            let mut rot = Vector3d::zero();

            for i in 0..3 {
                if bone.rotation_constraints[i].is_fixed() {
                    continue;
                }

                rot[i] = values[input_i];
                input_i += 1;
            }

            bone_rotations.push(rot);
        }

        assert_eq!(input_i, values.len());

        Self {
            translation,
            rotation,
            bone_rotations
        }
    }

    pub fn clamp(&mut self, skeleton: &Skeleton) {
        for i in 0..self.bone_rotations.len() {
            let rot = &mut self.bone_rotations[i];

            for j in 0..3 {
                let limits = &skeleton.bones[i].rotation_constraints[j];
                rot[j] = rot[j].max(limits.min).min(limits.max)
            }
        }
    }
}
