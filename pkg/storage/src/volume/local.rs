use alloc::string::{String, ToString};
use std::collections::HashMap;

use common::errors::*;

use crate::proto::volume::*;

/// Table containing the set of all files stored as local non-replicated files
/// on a single volume.
///
/// This table is initialized from VolumeLocalSnapshots and evolves when new log
/// entries are applied.
#[derive(Default)]
pub struct VolumeLocalFileTable {
    sequence_num: u64,
    path_to_id: HashMap<String, u64>,
    file_by_id: HashMap<u64, LocalFileEntry>,
}

impl VolumeLocalFileTable {
    /// Restores the file table from a snapshot.
    pub fn from_snapshot(snapshot: &VolumeLocalSnapshot) -> Result<Self> {
        let mut path_to_id = HashMap::new();
        let mut file_by_id = HashMap::new();

        for file in snapshot.files() {
            if path_to_id.contains_key(file.path()) {
                return Err(err_msg("Duplicate path in snapshot"));
            }

            if file_by_id.contains_key(&file.id()) {
                return Err(err_msg("Multiple files with id"));
            }

            path_to_id.insert(file.path().to_string(), file.id());
            file_by_id.insert(file.id(), file.as_ref().clone());
        }

        Ok(Self {
            sequence_num: snapshot.sequence_num(),
            path_to_id,
            file_by_id,
        })
    }

    /// Applies an already logged change to the file table.
    pub fn apply(&mut self, batch: &VolumeLogBatch) -> Result<()> {
        if batch.sequence_num() != self.sequence_num + 1 {
            return Err(err_msg("Expected to get monotonic log entries"));
        }

        todo!()
    }

    // pub fn apply(&mut self, )
}
