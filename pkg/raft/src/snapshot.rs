use std::io::Read;

/// Represents some interface to querying the most recent snapshot of the state machine
/// 
/// This is orthogonal to the configuration snapshot which is handled separately and owned by the consensus module
pub trait SnapshotStore {
	/// Gets the latest available snapshot
	/// NOTE: If the log has been compacted, then a snapshot must be always available
	fn get(&self) -> Option<Snapshot>;
}

pub struct Snapshot<'a> {

	/// Index of the last log entry in this snapshot
	pub last_applied: u64,

	/// Number of bytes needed to store this snapshot
	pub size: u64,

	/// A reader for retrieving the contents of the snapshot
	/// TODO: Possibly support 
	pub data: &'a Read
}

/*
	Good things to know about the snapshot

	-> Does it exist
	-> 

*/
