use std::sync::Arc;

use common::errors::*;

use crate::log_metadata::LogSequence;
use crate::proto::consensus::*;

// XXX: Also useful to have a fast estimate of the total size of the log up to
// now to decide on snapshotting policies

/// A log consisting of indexed entries with persistence capabilities
/// It can be assumed that for a single log instance, there will exclusively be
/// a single ConsensusModule appending entries to it
#[async_trait]
pub trait Log: Send + Sync {
    /// Given the index of a log entry, this should get the term stored for it
    ///
    /// If given a value from [first_index - 1, last_index], this should return
    /// a value
    /// None will be returned if the given index is completely out of range of
    /// the log
    async fn term(&self, index: LogIndex) -> Option<Term>;

    /// Get's the position of the last discarded log entry (immediately before
    /// the first entry in this log).
    async fn prev(&self) -> LogPosition;

    /// Gets the index of the last entry in the log (this may be less than the
    /// first_index if the log is empty)
    async fn last_index(&self) -> LogIndex;

    /// Gets a specific entry in the log by index
    /// XXX: Currently we do assume that all data fits in memory (but if we ever
    /// lose that assume, then it would still be critical that the
    /// ConsensusModule never has to call any blocking code inside of itself)
    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)>;

    /// Should add the given entry to the log returning the seq of that entry
    ///
    /// If there is already already an entry in the log with the same index as
    /// the given entry, then the log implementation should atomically truncate
    /// all old entries with index >= entry.index and append the new entry in
    /// one operation.
    ///
    /// The new entry is guranteed to have a higher term. If truncation and
    /// appending does not occur in one operation, then we may lose information
    /// about the highest term seen.
    ///
    /// This does not need to flush anything to the disk
    /// But the new entries should be immediately reflected in the state of the
    /// other operations.
    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()>;

    /// Should schedule all log entries from the beginning of the log up to and
    /// including the given position to be deleted
    /// The given position is assumed to be valid and committed position (if it
    /// isn't present in this log, we assume that it is present in someone
    /// else's log as a committed entry)
    ///
    /// TODO: Implement bypassing the local log if a follower is catching up and
    /// gets a committed entry.
    async fn discard(&self, pos: LogPosition) -> Result<()>;

    /// Retrieves the last sequence persisted to durable storage
    ///
    /// This can be implemented be tracking the position of the last entry
    /// written and synced to disk
    ///
    /// MUST always return a sequence >= than previous sequences returned
    /// by previous calls to this. If the sync state is initially uncertain,
    /// this can return LogSequence::zero().
    async fn last_flushed(&self) -> LogSequence;

    /// Should flush all log entries to persistent storage
    /// After this is finished, the match_index for it should be equal to the
    /// last_index (at least the one as of when this was first called)
    async fn flush(&self) -> Result<()>;
}
