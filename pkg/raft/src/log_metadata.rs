use std::collections::VecDeque;

use common::algorithms::upper_bound_by;

use crate::proto::consensus::{LogEntry, LogIndex, LogPosition};

/// Partial view of the full LogEntry log which only tracks the metadata
/// associated with entries in the log (index, term, sequence).
///
/// This data structure is completely synchronous and will be used by the
/// ConsensusModule to store its own copy of metadata for all entries since the
/// last commited entry.
///
/// Every log operation will first be applied to this LogMetadata object. This
/// will provide LogMutations which will be later applied asyncronously to the
/// log on durable storage.
pub struct LogMetadata {
    offsets: VecDeque<LogOffset>,
    last_offset: LogOffset,

    // NOTE: If a truncation occured, then this may not equal
    last_sequence: u64,

    /// Last sequence flushed to persistent storage.
    last_flushed: u64,
}

impl LogMetadata {
    pub fn last(&self) -> &LogOffset {
        &self.last_offset
    }

    pub fn lookup(&self, index: LogIndex) -> Option<LogOffset> {
        // self.offsets.is_empty() || index < self.offsets[0].position.index() ||

        if index > self.last_offset.position.index() {
            return None;
        }

        let prev_offset_idx =
            match upper_bound_by(&self.offsets, index, |off, idx| off.position.index() <= idx) {
                Some(idx) => idx,
                None => {
                    return None;
                }
            };

        let prev_offset = &self.offsets[prev_offset_idx];

        Some(LogOffset {
            position: LogPosition::new(prev_offset.position.term(), index),
            sequence: prev_offset.sequence + (index - prev_offset.position.index()),
        })
    }

    pub fn last_flushed(&self) -> u64 {
        self.last_flushed
    }

    pub fn set_last_flushed(&mut self, sequence: u64) {
        assert!(sequence >= self.last_flushed);
        self.last_flushed = sequence;
    }

    pub fn append(&mut self, entry: LogEntry) -> LogMutation {
        let sequence = self.last_sequence + 1;
        self.last_sequence = sequence;

        // TODO: Must update last_offset and ranges if needed.

        LogMutation {
            sequence,
            operation: LogOperation::Append(entry),
        }
    }
}

pub struct LogOffset {
    pub position: LogPosition,
    pub sequence: u64,
}

pub struct LogMutation {
    pub sequence: u64,
    pub operation: LogOperation,
}

pub enum LogOperation {
    Append(LogEntry),
    Truncate { start_index: LogIndex },
}

// struct LogState {
//     // pub offsets:

// // pub last_sequence: u64,
// // pub last_log_position: LogPosition,
// /*
//     Need to maintain a compact map of all LogIndex ->

//     Can use a Break system like for
// */
// // pub entries: VecDeque<(LogIndex, )>

// /*
//     Vector<(LogIndex, Term, Sequence)>

//
//     - Whenever LogIndex increments, Sequence also increments.

// */}
