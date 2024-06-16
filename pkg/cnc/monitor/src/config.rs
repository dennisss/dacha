use core::ops::Deref;

use base_error::*;
use cnc_monitor_proto::cnc::MachineConfig;
use protobuf::Message;

use crate::{protobuf_table::ProtobufDB, tables::MACHINE_TABLE_TAG};

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
        let mut merged = preset.clone();
        merged.merge_from(&diff)?;

        Ok(Self { diff, merged })
    }

    // pub async fn save(&self, db: &ProtobufDB) -> Result<()> {
    //     db.insert(&MACHINE_TABLE_TAG, &self.diff).await
    // }

    pub fn merge_from(&mut self, other: &MachineConfig) -> Result<()> {
        self.diff.merge_from(other)?;
        self.merged.merge_from(other)?;
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
