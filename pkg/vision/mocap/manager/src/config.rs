use std::ops::Deref;
use std::collections::HashMap;

use common::hash::FastHasherBuilder;
use common::errors::*;
use mocap_proto::mocap::*;
use cluster_client::id::{entity_id_to_string, entity_id_from_string};
use vision::{CameraIntrinsicsModel, CameraExtrinsics};
use protobuf::Message;


pub struct ManagerConfigContainer {
    merged: MocapManagerConfig,
    diff: MocapManagerConfig,

    revision: u64,

    // There are extracted from the merged config.
    camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
    camera_extrinsics: HashMap<u64, CameraExtrinsics, FastHasherBuilder>,
}

impl ManagerConfigContainer {
    pub fn create(base: &MocapManagerConfig) -> Result<Self> {

        let mut merged = base.clone();

        for per_cam in merged.per_camera_mut() {
            if !per_cam.camera_id_str().is_empty() {
                let camera_id = entity_id_from_string(per_cam.camera_id_str()).unwrap();
                per_cam.clear_camera_id_str();
                per_cam.set_camera_id(camera_id);
            }
        }

        let mut inst = Self {
            merged,
            diff: MocapManagerConfig::default(),
            revision: 1,
            camera_intrinsics: Default::default(),
            camera_extrinsics: Default::default(),
        };

        inst.extract_camera_params();

        Ok(inst)
    }

    // Re-computes the camera_intrinsics/camera_extrinsics maps.
    fn extract_camera_params(&mut self) {
        self.camera_intrinsics.clear();
        self.camera_extrinsics.clear();

        for per_cam in self.merged.per_camera() {
            let camera_id = per_cam.camera_id();
            
            if per_cam.has_intrinsics() {
                self.camera_intrinsics.insert(camera_id, CameraIntrinsicsModel::from_proto(per_cam.intrinsics()));
            }

            if per_cam.has_extrinsics() {
                self.camera_extrinsics.insert(camera_id, CameraExtrinsics::from_proto(per_cam.extrinsics()));
            }
        }
    }

    pub fn value(&self) -> &MocapManagerConfig {
        &self.merged
    }

    pub fn diff(&self) -> &MocapManagerConfig {
        &self.diff
    }

    /// Starts at 1 and increments for each update.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn camera_enabled(&self, camera_id: u64) -> bool {
        let config = match self.merged.per_camera().iter().find(|c| c.camera_id() == camera_id) {
            Some(v) => v,
            None => return false
        };

        config.enabled()
    }

    pub fn num_enabled_cameras(&self) -> usize {
        let mut n = 0;

        for c in self.merged.per_camera() {
            if c.enabled() {
                n += 1;
            }
        }

        n
    }

    pub fn camera_intrinsics(&self) -> &HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder> {
        &self.camera_intrinsics
    }

    pub fn camera_extrinsics(&self) -> &HashMap<u64, CameraExtrinsics, FastHasherBuilder> {
        &self.camera_extrinsics
    }

    // TODO: Ideal semantics are that all Matrix/Vector protos completely override old ones.
    pub fn merge_from(&mut self, other: &MocapManagerConfig) -> Result<()> {

        self.revision += 1;

        // Camera data merging.
        for new_cam in other.per_camera() {
            // TODO: Check non-zero camera id.

            let merged_cam = Self::existing_or_new_camera_entry(&mut self.merged, new_cam.camera_id());
            Self::merge_camera_entries(merged_cam, new_cam);

            let diff_cam = Self::existing_or_new_camera_entry(&mut self.diff, new_cam.camera_id());
            Self::merge_camera_entries(diff_cam, new_cam);
        }

        if other.has_rigid_body_tracker() {
            // Clear so that the new diff can introduce deletions.
            self.merged.rigid_body_tracker_mut().clear_bodies();
            self.diff.rigid_body_tracker_mut().clear_bodies();
            
            self.merged.rigid_body_tracker_mut().merge_from(other.rigid_body_tracker());
            self.diff.rigid_body_tracker_mut().merge_from(other.rigid_body_tracker());
        }

        // TODO: Regular proto merge for the rest of the stuff.

        self.extract_camera_params();

        Ok(())
    }

    fn existing_or_new_camera_entry(
        config: &mut MocapManagerConfig, camera_id: u64
    ) -> &mut MocapPerCameraConfig {
        for i in 0..config.per_camera().len() {
            if config.per_camera()[i].camera_id() == camera_id {
                return &mut config.per_camera_mut()[i];
            }
        }

        let c = config.new_per_camera();
        c.set_camera_id(camera_id);
        c
    }

    fn merge_camera_entries(base: &mut MocapPerCameraConfig, diff: &MocapPerCameraConfig) {
        if diff.has_intrinsics() {
            base.set_intrinsics(diff.intrinsics().clone());
        }
        if diff.has_extrinsics() {
            base.set_extrinsics(diff.extrinsics().clone());
        }
        if diff.has_enabled() {
            base.set_enabled(diff.enabled());
        }
    }
}

impl Deref for ManagerConfigContainer {
    type Target = MocapManagerConfig;

    fn deref(&self) -> &Self::Target {
        &self.merged
    }
}
