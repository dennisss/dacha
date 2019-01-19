use super::errors::*;
use super::protos::*;

/*
pub struct LogEntryIndex {
	pub index: u64,
	pub term: u64
}
*/

pub trait LogStore {

	/// Given the index of a log entry, this should get the term stored for it
	fn term(&self, index: u64) -> Option<u64>;

	fn first_index(&self) -> Option<u64>;

	fn last_index(&self) -> Option<u64>; // XXX: Default would be 

	/// Gets a specific entry in the log by index
	fn entry(&self, index: u64) -> Option<&LogEntry>; 

	/// Adds entries to the very end of the log and atomically flushes them
	/// TODO: Realistically this flush can occur whenever on the leader as long as it is before we respond to the client?
	fn append(&mut self, entries: &[LogEntry]) -> Result<()>;

	/// Should immediately remove all log entries starting at the given index until the end of the log
	fn truncate_suffix(&mut self, start_index: u64) -> Result<()>;

}


pub struct MemoryLogStore {
	log: Vec<LogEntry>
}

impl MemoryLogStore {
	pub fn new() -> Self {
		MemoryLogStore {
			log: vec![]
		}
	}

	fn pos_for(&self, index: u64) -> Option<usize> {
		if self.log.len() == 0 {
			return None;
		}

		let first_index = self.log[0].index;

		// TODO: This could go negative if we are not careful
		Some((index - first_index) as usize)
	}
}

impl LogStore for MemoryLogStore {

	fn term(&self, index: u64) -> Option<u64> {
		
		if index == 0 {
			return Some(0);
		}

		if self.log.len() == 0 {
			return None;
		}

		// TODO: This does not properly implement the previous index case right now

		let pos = match self.pos_for(index) {
			Some(v) => v,
			None => return None
		};
		
		match self.log.get(pos) {
			Some(v) => {
				assert_eq!(v.index, index);
				Some(v.term)
			},
			None => None
		}
	}

	fn first_index(&self) -> Option<u64> {
		match self.log.first() {
			Some(v) => Some(v.index),
			None => None
		}
	}

	fn last_index(&self) -> Option<u64> {
		match self.log.last() {
			Some(v) => Some(v.index),
			None => None
		}
	}

	fn entry(&self, index: u64) -> Option<&LogEntry> {
		let pos = match self.pos_for(index) {
			Some(v) => v,
			None => return None
		};
		
		match self.log.get(pos) {
			Some(v) => {
				assert_eq!(v.index, index);
				Some(&v)
			},
			None => None
		}
	}

	fn append(&mut self, entries: &[LogEntry]) -> Result<()> {
		self.log.extend_from_slice(entries);
		Ok(())
	}

	fn truncate_suffix(&mut self, start_index: u64) -> Result<()> {
		let pos = match self.pos_for(start_index) {
			Some(v) => v,
			None => panic!("Truncating starting at unknown position")
		};

		self.log.truncate(pos);
		Ok(())
	}

}




/*
	General operations:

	- Append to End atomically

	- Must know index of first entry
	- Must know index of last entry
	

	- Eventually be able to truncate from beginning or end of the log
	- Ideally should be able to get any entry very quickly
		- Entries that were most recently appended should be immediately still in memory for other threads (the state machine to see)

*/

/*
	Threads
	1. Heartbeat/Election/Catchup-Replication
	2. Consensus server (read from server, append to log)
	3. State machine applier (read from log, write to machine)
	4. Client interface

	Make everything single-thread-able with multithreading as an option if needed

*/
