use std::sync::Arc;
use common::errors::*;
use crate::protos::*;

/*
	If all snapshots are beyond the end of the log:
	- We can call discard() up to the highest snapshoted index

	Suppose snapshots are beyond the log
	-> should never happen
	-> 
	-> 

	Suppose the log is empty:
	- If all snapshots are beyond the end of 


*/

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
	
	/// Gets the index of the first full entry in the log
	async fn first_index(&self)-> LogIndex;

	/// Gets the index of the last entry in the log (this may be less than the
	/// first_index if the log is empty)
	async fn last_index(&self) -> LogIndex;

	/// Gets a specific entry in the log by index
	/// XXX: Currently we do assume that all data fits in memory (but if we ever
	/// lose that assume, then it would still be critical that the
	/// ConsensusModule never has to call any blocking code inside of itself)
	async fn entry(&self, index: LogIndex) -> Option<(Arc<LogEntry>, LogSeq)>;
	
	/// Should add the given entry to the log returning the seq of that entry
	/// 
	/// This does not need to flush anything to the disk
	/// But the new entries should be immediately reflected in the state of the
	/// other operations
	async fn append(&self, entry: LogEntry) -> LogSeq;

	/// Should remove all log entries starting at the given index until the end
	/// of the log
	/// 
	/// If the underlying storage system explicitly stores truncations as a
	/// separate operation, then this function may return a sequence to uniquely
	/// identify the truncation operation during flushing.
	/// Supporting this mode allows the persistent storage to perform the
	/// minimum number of writes to maintain progress in the consensus module
	/// if this machine has other higher priority writes to finish first
	async fn truncate(&self, start_index: LogIndex) -> Option<LogSeq>;

	async fn checkpoint(&self) -> LogPosition;

	/// Should schedule all log entries from the beginning of the log up to and
	/// including the given position to be deleted
	/// The given position is assumed to be valid and committed position (if it
	/// isn't present in this log, we assume that it is present in someone
	/// else's log as a committed entry)
	async fn discard(&self, pos: LogPosition);

	/// Retrieves the last sequence persisted to durable storage
	/// 
	/// This can be implemented be tracking the position of the last entry
	/// written and synced to disk
	async fn last_flushed(&self) -> Option<LogSeq>;
	// ^ If this returns None, then we will assume that it is equivalent to
	// LogPosition::zero()

	/// Should flush all log entries to persistent storage
	/// After this is finished, the match_index for it should be equal to the
	/// last_index (at least the one as of when this was first called)
	async fn flush(&self) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogSeq(pub usize);

impl LogSeq {
	/// Determines whether or not everything up to this sequence has been
	/// persisted locally
	pub async fn is_flushed(&self, log: &dyn Log) -> bool {
		let last_flushed = log.last_flushed().await.unwrap_or(LogSeq(0));
		self.0 >= last_flushed.0
	}
}
