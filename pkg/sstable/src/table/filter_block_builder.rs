use std::sync::Arc;

use crate::table::filter_policy::*;

pub struct FilterBlockBuilder {
    policy: Arc<dyn FilterPolicy>,

    /// Output buffer containing all fully built filters so far.
    output: Vec<u8>,

    /// Offset of each individual block filter in 'output' relative to the start
    /// of 'output'.
    offsets: Vec<u32>,

    base: u64,

    log_base: u8,

    pending_keys: Vec<Vec<u8>>,
}

impl FilterBlockBuilder {
    pub fn new(policy: Arc<dyn FilterPolicy>, log_base: u8) -> Self {
        Self {
            policy,
            output: vec![],
            offsets: vec![],
            base: 1 << (log_base as usize),
            log_base,
            pending_keys: vec![],
        }
    }

    /// Writes the current set of keys to the output block.
    fn flush(&mut self) {
        // TODO: Eventually skip the first offset (should always be 0).
        self.offsets.push(self.output.len() as u32);

        // If no keys, generate an empty filter.
        if self.pending_keys.len() == 0 {
            return;
        }

        // TODO: Re-use this memory
        let mut pending_key_slices = vec![];
        pending_key_slices.reserve(self.pending_keys.len());
        for k in &self.pending_keys {
            pending_key_slices.push(k.as_ref());
        }

        self.policy.create(pending_key_slices, &mut self.output);
        self.pending_keys.clear();
    }

    /// Indicates to the filter builder that future key additions will be in a
    /// data block whose file offset is 'offset'.
    ///
    /// NOTE: This should only ever be called with offsets in increasing order.
    /// There is no harm in calling this multiple times with the same offset.
    pub fn start_block(&mut self, offset: u64) {
        let filter_idx = (offset / self.base) as usize;
        assert!(filter_idx >= self.offsets.len(), "Non-monotonic blocks");
        while self.offsets.len() < filter_idx {
            self.flush();
        }
    }

    pub fn add_key(&mut self, key: Vec<u8>) {
        self.pending_keys.push(key);
    }

    pub fn finish(mut self) -> Vec<u8> {
        // No need to push an offset for the last block if it is empty.
        if self.pending_keys.len() > 0 {
            self.flush();
        }

        let mut block = self.output;
        let offsets_start = block.len();
        for off in self.offsets {
            block.extend_from_slice(&off.to_le_bytes());
        }

        block.extend_from_slice(&(offsets_start as u32).to_le_bytes());
        block.push(self.log_base);

        block
    }
}
