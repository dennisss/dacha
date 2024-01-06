use std::sync::Arc;

use common::errors::*;

use crate::log::log_metadata::LogSequence;
use crate::proto::*;

/// A log consisting of indexed entries which are persisted to durable storage.
///
/// It can be assumed that for a single log instance, there will exclusively be
/// a single ConsensusModule appending entries to it.
///
/// If the log has any internal errors (like background flushing tasks failing),
/// they should be detectable by callers who periodically call
/// wait_for_flushed().
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
    ///
    /// MUST be durable to persistent storage.
    async fn prev(&self) -> LogPosition;

    /// Gets the index of the last entry in the log (this may be less than the
    /// first_index if the log is empty)
    async fn last_index(&self) -> LogIndex;

    /// Gets a specific entry in the log by index.
    async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSequence)>;

    /// Atomically fetches a range of entries with indices in `[start_index,
    /// end_index]`.
    ///
    /// (atomically means that new log truncations are not applied during the
    /// execution of this function)
    ///
    /// Returns the list of entries and the sequence of the last returned entry
    /// (or None if the range isn't in the log)
    async fn entries(
        &self,
        start_index: LogIndex,
        end_index: LogIndex,
    ) -> Option<(Vec<Arc<LogEntry>>, LogSequence)>;

    /// Should add the given entry to the log.
    ///
    /// Only entries with index > last_log_index can be appended. Other entries
    /// can be discarded immediately.
    ///
    /// If there is already already an entry in the log with the same index as
    /// the given entry, then the log implementation should atomically truncate
    /// all old entries with index >= entry.index and append the new entry in
    /// one operation.
    ///
    /// The new entry is guranteed to have a higher term. Note that if
    /// truncation is needed, it must be atomic with appending the new entry as
    /// we MUST NOT lose information about what the latest term is.
    ///
    /// This does not need to flush anything to the disk
    /// But the new entries should be immediately reflected in the state of the
    /// other operations. Eventually last_flushed() should start returning a
    /// number >= sequence.
    async fn append(&self, entry: LogEntry, sequence: LogSequence) -> Result<()>;

    /// Marks all log entries up to and including 'pos' as eligible for removal
    /// from the start of the log.
    ///
    /// The value of prev() is expected to eventually become 'pos'.
    ///
    /// The given position is assumed to be valid and committed position (if it
    /// isn't present in this log, we assume that it is present in someone
    /// else's log as a committed entry)
    ///
    /// - SHOULD retain discarded log entries for a short period of time for the
    ///   purposes of slow follower recovery.
    /// - MUST NOT return an error if the entry was already discarded.
    /// - MUST support discarding beyond the end of the log.
    ///   - If pos.index > last_log_index, then prev() MUST immediately start
    ///     returning 'pos'.
    ///
    /// NOTE: This operation should have no effect on the last_flushed()
    /// sequence.
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

    /// Should block until the log has changed since the last time
    /// wait_for_flush() was called (or since log initialization from persistent
    /// storage was complete).
    ///
    /// A change can be to the value returned be prev() or last_flushed().
    async fn wait_for_flush(&self) -> Result<()>;
}
