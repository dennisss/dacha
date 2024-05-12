use std::collections::VecDeque;

use common::algorithms::upper_bound_by;

use crate::proto::{LogIndex, LogPosition};

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
///
/// In practice, operations on this data structure should feel like O(k) where k
/// is the average number of uncomitted elections. Under normal operation, k
/// should never go beyond 2.
///
/// TODO: Consolidate this implementation more with the MemoryLog.
pub struct LogMetadata {
    /// Start offsets of contiguous ranges of entries stored in the log.
    ///
    /// In other words, this is the compressed form of a vector of the form:
    ///   list[log_index] = (log_term, sequence)
    ///
    /// These are stored in sorted order by log index (indirectly this means
    /// they are also sorted by sequence and term).
    ///
    /// - This will always contain at least one entry.
    /// - offsets[0] will always be the offset of the previous log entry before
    /// the start of this log (index=0, term=0, sequence=0 if entries have
    /// never been appended to any log before).
    /// - Using this array, you can extrapolate the term/sequence of any
    ///   arbitrary log index as follows:
    ///   - Find an abjacent pair of offsets such that offsets[i] <= idx <
    ///     offsets[i+1]
    ///   - Return a term of 'offsets[i].term'
    ///   - Return a sequence of 'offsets[i].sequence + (idx -
    ///     offsets[i].index)'
    offsets: VecDeque<LogOffset>,

    /// Offset of the most recent entry added to the log.
    /// For an empty log, this will be equal to offsets[0].
    last_offset: LogOffset,
}

impl LogMetadata {
    pub fn new() -> Self {
        let zero = LogOffset {
            sequence: LogSequence::zero(),
            position: LogPosition::zero(),
        };

        let mut offsets = VecDeque::new();
        offsets.push_back(zero.clone());

        Self {
            offsets,
            last_offset: zero,
        }
    }

    /// Gets the offset of the log entry immediately before the first entry in
    /// this log.
    ///
    /// This offset should be considered to be immutable as it was already
    /// discarded and merged into the latest snapshot.
    pub fn prev(&self) -> &LogOffset {
        &self.offsets[0]
    }

    pub fn last(&self) -> &LogOffset {
        &self.last_offset
    }

    pub fn lookup(&self, index: LogIndex) -> Option<LogOffset> {
        if index > self.last_offset.position.index() {
            return None;
        }

        let prev_offset_idx = match upper_bound_by(self.offsets.as_slices(), index, |off, idx| {
            off.position.index() <= idx
        }) {
            Some(idx) => idx,
            None => {
                return None;
            }
        };

        let prev_offset = &self.offsets[prev_offset_idx];

        Some(LogOffset {
            position: LogPosition::new(prev_offset.position.term(), index),
            sequence: prev_offset
                .sequence
                .plus(index.value() - prev_offset.position.index().value()),
        })
    }

    pub fn lookup_seq(&self, seq: LogSequence) -> Option<LogOffset> {
        if seq > self.last_offset.sequence {
            return None;
        }

        let prev_offset_idx = match upper_bound_by(self.offsets.as_slices(), seq, |off, seq| {
            off.sequence <= seq
        }) {
            Some(idx) => idx,
            None => {
                return None;
            }
        };

        let prev_offset = &self.offsets[prev_offset_idx];

        let relative_offset = seq.opaque_value - prev_offset.sequence.opaque_value;

        Some(LogOffset {
            position: LogPosition::new(
                prev_offset.position.term(),
                prev_offset.position.index() + relative_offset,
            ),
            sequence: seq,
        })
    }

    pub fn append(&mut self, offset: LogOffset) {
        assert!(offset.position.term() >= self.last().position.term());
        assert!(offset.sequence > self.last_offset.sequence);

        if offset.position.index() <= self.last().position.index() {
            self.truncate(offset.position.index());
        }

        assert_eq!(offset.position.index(), self.last().position.index() + 1);

        if offset.position.term() != self.last_offset.position.term()
            || offset.sequence != self.last_offset.sequence.plus(1)
        {
            self.offsets.push_back(offset.clone());
        }

        self.last_offset = offset;
    }

    // TODO: Implement and call this whenever the commit_index changes in the
    // consensus module.
    pub fn discard(&mut self, new_start: LogPosition) {
        if new_start.index() > self.last_offset.position.index() {
            self.offsets.clear();
            self.offsets.push_back(LogOffset {
                position: new_start.clone(),
                sequence: self.last_offset.sequence,
            });
            self.last_offset.position = new_start;
            return;
        }

        if new_start.index() <= self.prev().position.index() {
            return;
        }

        let seq = self.lookup(new_start.index()).unwrap().sequence;

        let offset_idx = upper_bound_by(self.offsets.as_slices(), new_start.index(), |off, idx| {
            off.position.index() <= idx
        })
        .unwrap();

        // Truncate front
        drop(self.offsets.drain(0..offset_idx));

        self.offsets[0].position = new_start;
        self.offsets[0].sequence = seq;
    }

    /// Should remove all log entries starting at the given index until the end
    /// of the log
    fn truncate(&mut self, start_index: LogIndex) {
        // Find the offset needed to resolve start_index - 1.
        let prev_offset_idx =
            upper_bound_by(self.offsets.as_slices(), start_index - 1, |off, idx| {
                off.position.index() <= idx
            })
            .unwrap();

        // All future indexes are invalid so we only need to keep up to the
        // prev_offset_idx offset.
        self.offsets.truncate(prev_offset_idx + 1);

        self.last_offset = self.lookup(start_index - 1).unwrap();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogOffset {
    pub position: LogPosition,
    pub sequence: LogSequence,
}

impl LogOffset {
    pub fn zero() -> Self {
        Self {
            position: LogPosition::zero(),
            sequence: LogSequence::zero(),
        }
    }
}

/// Monotonically increasing identifier for newly appended log entries.
///
/// Whenever a log entry is appended to the log, it will be assigned a new
/// sequence which is greater than all prior sequences generated in the same
/// process.
///
/// Note that LogSequence intentionally doesn't expose a good API for wire
/// serialization as it meant to only be valid until the process dies, so should
/// never be stored on disk.
#[derive(PartialEq, PartialOrd, Clone, Copy, Debug)]
pub struct LogSequence {
    opaque_value: u64,
}

impl LogSequence {
    /// Generates the smallest possible sequence.
    ///
    /// All log entries MUST have a sequence > zero().
    pub fn zero() -> Self {
        LogSequence { opaque_value: 0 }
    }

    /// Generates an new sequence that is larger than the current sequence.
    pub fn next(self) -> Self {
        self.plus(1)
    }

    fn plus(self, value: u64) -> Self {
        Self {
            opaque_value: self.opaque_value + value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_metadata_test() {
        let mut meta = LogMetadata::new();
        assert_eq!(*meta.prev(), LogOffset::zero());
        assert_eq!(*meta.last(), LogOffset::zero());
        assert_eq!(meta.lookup(0.into()), Some(LogOffset::zero()));
        assert_eq!(meta.lookup(1.into()), None);

        let off1 = LogOffset {
            position: LogPosition::new(1, 1),
            sequence: LogSequence { opaque_value: 1 },
        };

        meta.append(off1.clone());
        assert_eq!(*meta.prev(), LogOffset::zero());
        assert_eq!(*meta.last(), off1);
        assert_eq!(meta.lookup(0.into()), Some(LogOffset::zero()));
        assert_eq!(meta.lookup(1.into()), Some(off1.clone()));
        assert_eq!(meta.lookup(2.into()), None);

        let off2 = LogOffset {
            position: LogPosition::new(1, 2),
            sequence: LogSequence { opaque_value: 2 },
        };
        let off3 = LogOffset {
            position: LogPosition::new(1, 3),
            sequence: LogSequence { opaque_value: 3 },
        };

        meta.append(off2.clone());
        meta.append(off3.clone());
        assert_eq!(*meta.prev(), LogOffset::zero());
        assert_eq!(*meta.last(), off3);
        assert_eq!(meta.lookup(0.into()), Some(LogOffset::zero()));
        assert_eq!(meta.lookup(1.into()), Some(off1.clone()));
        assert_eq!(meta.lookup(2.into()), Some(off2.clone()));
        assert_eq!(meta.lookup(3.into()), Some(off3.clone()));
        assert_eq!(meta.lookup(4.into()), None);

        // Starting a new term
        let off4 = LogOffset {
            position: LogPosition::new(2, 4),
            sequence: LogSequence { opaque_value: 4 },
        };

        meta.append(off4.clone());
        assert_eq!(*meta.prev(), LogOffset::zero());
        assert_eq!(*meta.last(), off4);
        assert_eq!(meta.lookup(0.into()), Some(LogOffset::zero()));
        assert_eq!(meta.lookup(1.into()), Some(off1.clone()));
        assert_eq!(meta.lookup(2.into()), Some(off2.clone()));
        assert_eq!(meta.lookup(3.into()), Some(off3.clone()));
        assert_eq!(meta.lookup(4.into()), Some(off4.clone()));
        assert_eq!(meta.lookup(5.into()), None);

        meta.truncate(off3.position.index());
        assert_eq!(*meta.prev(), LogOffset::zero());
        assert_eq!(*meta.last(), off2);
        assert_eq!(meta.lookup(0.into()), Some(LogOffset::zero()));
        assert_eq!(meta.lookup(1.into()), Some(off1.clone()));
        assert_eq!(meta.lookup(2.into()), Some(off2.clone()));
        assert_eq!(meta.lookup(3.into()), None);
        assert_eq!(meta.lookup(4.into()), None);
    }

    #[test]
    fn discard_from_empty() {
        let mut meta = LogMetadata::new();

        let offset = LogOffset {
            position: LogPosition::new(10, 20),
            sequence: LogSequence { opaque_value: 0 },
        };

        meta.discard(offset.position.clone());
        assert_eq!(*meta.prev(), offset);
        assert_eq!(*meta.last(), offset);
        assert_eq!(meta.lookup(4.into()), None);
        assert_eq!(meta.lookup(11.into()), None);
        assert_eq!(meta.lookup(25.into()), None);
    }

    #[test]
    fn discard_some_entries() {
        let mut meta = LogMetadata::new();

        assert_eq!(
            meta.prev().clone(),
            LogOffset {
                position: LogPosition::new(0, 0),
                sequence: LogSequence { opaque_value: 0 },
            }
        );

        for i in 1..101 {
            meta.append(LogOffset {
                position: LogPosition::new((i / 10) + 1, i),
                sequence: LogSequence { opaque_value: i },
            });
        }

        assert_eq!(meta.offsets.len(), 12);

        // TODO: Also check immediately before and after the end of the range.
        for i in 1..101 {
            let offset = LogOffset {
                position: LogPosition::new((i / 10) + 1, i),
                sequence: LogSequence { opaque_value: i },
            };

            assert_eq!(meta.lookup(i.into()), Some(offset.clone()));

            assert_eq!(
                meta.lookup_seq(LogSequence { opaque_value: i }),
                Some(offset.clone())
            );
        }

        // TODO: Extend this test to try discarding at all log positions.
        {
            let i = 32;
            meta.discard(LogPosition::new((i / 10) + 1, i));

            assert_eq!(
                meta.prev().clone(),
                LogOffset {
                    position: LogPosition::new((i / 10) + 1, i),
                    sequence: LogSequence { opaque_value: i },
                }
            );
        }

        for i in 33..101 {
            let offset = LogOffset {
                position: LogPosition::new((i / 10) + 1, i),
                sequence: LogSequence { opaque_value: i },
            };

            assert_eq!(meta.lookup(i.into()), Some(offset.clone()));

            assert_eq!(
                meta.lookup_seq(LogSequence { opaque_value: i }),
                Some(offset.clone())
            );
        }
    }

    // TODO: Test truncation to various indexes or the truncation of everything
    // in the log.

    // TODO: Test discard only some existing entries.

    // TODO: Test discarding all entries in a non-empty LogMetadata.

    // TODO:
}
