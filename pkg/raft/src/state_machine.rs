use bytes::Bytes;
use super::errors::*;
use super::protos::*;
use std::io::Read;


pub trait StateMachine<R> {

	// TODO: Should probably have a check operation that validates an operation is good before a leader decide to commit them (either way we will still be consistent )	

	// ^ issue being that because operations are not independent, this would need to be checked per operation
	// So the alternative would be to require the StateMachine to implement an apply, revert, and commit


	/// Should apply the given operation to the state machine immediately integrating it
	/// If successful, then some result type can be output that is persisted to disk but is made available to the task that proposed this change to receive feedback on how the operation performed
	fn apply(&self, index: LogIndex, op: &[u8]) -> Result<R>;

	/// Should retrieve the last created snapshot if any is available
	/// This should be a cheap operation that can quickly queried to check on the last snapshot
	fn snapshot<'a>(&'a self) -> Option<StateMachineSnapshot<'a>>;

	fn restore(&self, data: Bytes) -> Result<()>;

	// Triggers a new snapshot to begin being created and persisted to disk
	// The index of the last entry applied to the state machine is given as an argument to be stored alongside the snapshot
	// Returns a receiver which resolves once the snapshot has been created or has failed to be created
	//fn perform_snapshot(&self, last_applied: u64) -> Result<oneshot::Receiver<()>>;
}

pub struct StateMachineSnapshot<'a> {

	/// Index of the last log entry in this snapshot (same value originally given to the perform_snapshot that created this snapshot )
	pub last_applied: u64,

	/// Number of bytes needed to store this snapshot
	pub size: u64,

	/// A reader for retrieving the contents of the snapshot
	pub data: &'a Read
}

