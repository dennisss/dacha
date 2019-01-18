use super::errors::*;
use super::protos::*;

pub struct LogEntryIndex {
	pub index: u64,
	pub term: u64
}

pub trait LogStore {

	/// Given the index of a log entry, this should get the term stored for it
	fn get_term_at(&self, index: u64) -> Option<u64>;

	fn last_entry_index(&self) -> Option<LogEntryIndex>; // XXX: Default would be 

	/// Adds entries to the very end of the log and atomically flushes them
	/// TODO: Realistically this flush can occur whenever on the leader as long as it is before we respond to the client?
	fn append(&mut self, entries: Vec<LogEntry>);

	/// Should immediately remove all log entries starting at the given index until the end of the log
	fn truncate_suffix(&mut self, start_index: u64);

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
		Some((index - first_index) as usize)
	}
}

impl LogStore for MemoryLogStore {

	fn get_term_at(&self, index: u64) -> Option<u64> {
		
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

	fn last_entry_index(&self) -> Option<LogEntryIndex> {
		match self.log.last() {
			Some(v) => Some(LogEntryIndex {
				index: v.index,
				term: v.term
			}),
			None => None
		}
	}

	fn append(&mut self, entries: Vec<LogEntry>) {
		// TODO: Reserve space first?
		self.log.extend(entries.into_iter());
	}

	fn truncate_suffix(&mut self, start_index: u64) {
		let pos = match self.pos_for(start_index) {
			Some(v) => v,
			None => panic!("Truncating starting at unknown position")
		};

		self.log.truncate(pos);
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
