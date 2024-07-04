use core::ops::Deref;
use std::collections::HashSet;

use base_error::*;
use cnc_monitor_proto::cnc::MachineConfig;
use crypto::random::RngExt;
use protobuf::{Message, MessageReflection};

use crate::db::ProtobufDB;

/// Stores the configuration for a machine.
///
/// - configs are stored on disk as a diff relative to the base preset.
/// - the diff is merged with the preset
pub struct MachineConfigContainer {
    diff: MachineConfig,
    merged: MachineConfig,
}

impl MachineConfigContainer {
    pub fn create(diff: MachineConfig, preset: &MachineConfig) -> Result<Self> {
        let mut inst = Self {
            diff: MachineConfig::default(),
            merged: preset.clone(),
        };
        inst.merge_from(&diff)?;

        Ok(inst)
    }

    pub fn diff(&self) -> &MachineConfig {
        &self.diff
    }

    pub fn merge_from(&mut self, other: &MachineConfig) -> Result<()> {
        // Make copies of everything. They will only be updated at the end if everything
        // succeeds.
        let mut diff = self.diff.clone();
        let mut merged = self.merged.clone();
        let mut other = other.clone();

        // The path field is propagated to track correspondence of devices to selectors
        // but should otherwise never be used for selection.
        if !other.device().path().is_empty() {
            return Err(err_msg("Should not have a path in selector"));
        }

        for camera in other.cameras() {
            if !camera.device().path().is_empty() {
                return Err(err_msg("Should not have a path in selector"));
            }
        }

        let mut existing_clear_fields = HashSet::<Vec<u8>>::default();
        for field_path in diff.clear_fields() {
            existing_clear_fields.insert(field_path.serialize()?);
        }

        for field_path in other.clear_fields_mut() {
            if field_path.key_len() != 1 {
                return Err(err_msg(
                    "Only clear_field entries with single key paths are supported",
                ));
            }

            if field_path.key()[0].has_field_name() {
                let name = field_path.key()[0].field_name();
                let id = merged
                    .field_number_by_name(name)
                    .ok_or_else(|| format_err!("Unknown MachineConfig field named: {}", name))?;

                field_path.key_mut()[0].set_field_id(id);
            }

            if !field_path.key()[0].has_field_id() {
                return Err(err_msg("Field path key missing field_id"));
            }

            let field_id = field_path.key()[0].field_id();
            if merged.field_by_number(field_id).is_none() {
                return Err(err_msg("No field by given id"));
            }

            merged.clear_field_with_number(field_id);
            diff.clear_field_with_number(field_id);

            let field_path_bytes = field_path.serialize()?;
            if existing_clear_fields.insert(field_path_bytes) {
                diff.add_clear_fields(field_path.as_ref().clone());
            }
        }

        other.clear_clear_fields();

        // TODO: Move this logic somewhere else.
        for camera in other.cameras_mut() {
            if camera.id() == 0 {
                camera.set_id(crypto::random::clocked_rng().uniform::<u64>());
            }
        }

        diff.merge_from(&other)?;
        merged.merge_from(&other)?;

        self.diff = diff;
        self.merged = merged;

        Ok(())
    }

    pub fn value(&self) -> &MachineConfig {
        &self.merged
    }
}

impl Deref for MachineConfigContainer {
    type Target = MachineConfig;

    fn deref(&self) -> &Self::Target {
        &self.merged
    }
}
