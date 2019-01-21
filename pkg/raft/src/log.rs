use super::errors::*;
use super::protos::*;

use std::sync::Arc;

#[derive(Debug)]
pub struct LogPosition {
	pub index: u64,
	pub term: u64
}


// TODO: Log entries must stay in memory until they 


pub trait LogStorage {

	/// Given the index of a log entry, this should get the term stored for it
	/// None will be returned if the 
	fn term(&self, index: u64) -> Option<u64>;

	/// Gets the index of the first entry in the log
	/// XXX: Should always be present (at least as 0)
	fn first_index(&self) -> Option<u64>;

	fn last_index(&self) -> Option<u64>;

	/// Retrieves the last index persisted to durable storage
	fn match_index(&self) -> Option<u64>;

	/// Gets a specific entry in the log by index
	fn entry(&self, index: u64) -> Option<Arc<LogEntry>>; 

	/// Adds entries to the very end of the log and atomically flushes them
	/// TODO: Realistically this flush can occur whenever on the leader as long as it is before we respond to the client?
	
	/// Should add the given entries to the log
	/// 
	/// This does not need to flush anything to the disk
	/// But the new entries should be immediately reflected in the state of the other operations 
	fn append(&self, entry: LogEntry);

	/// Should immediately remove all log entries starting at the given index until the end of the log
	fn truncate_suffix(&self, start_index: u64);


	// TODO: Everything belo this point should never be used by the core consensus code

	/// Should syncronously flush all log entries to persistent storage
	/// After this is finished, the match_index for it should be equal to the last_index (at least the one as of when this was first called)
	fn flush(&self) -> Result<()>;


	// TODO: Also all of the snapshot related stuff here

}


use std::sync::Mutex;

pub struct MemoryLogStorage {
	log: Mutex<Vec<Arc<LogEntry>>>
}

impl MemoryLogStorage {
	pub fn new() -> Self {
		MemoryLogStorage {
			log: Mutex::new(vec![])
		}
	}

	// EIther it is a valid index, it is the index for the previous entry, or None
	fn pos_for(index: u64, log: &Vec<Arc<LogEntry>>) -> Option<usize> {
		if log.len() == 0 {
			return None;
		}

		let first_index = log[0].index;

		// TODO: This could go negative if we are not careful
		Some((index - first_index) as usize)
	}
}

impl LogStorage for MemoryLogStorage {

	fn term(&self, index: u64) -> Option<u64> {
		
		let log = self.log.lock().unwrap();

		if index == 0 {
			return Some(0);
		}

		if log.len() == 0 {
			return None;
		}

		// TODO: This does not properly implement the previous index case right now

		let pos = match Self::pos_for(index, &log) {
			Some(v) => v,
			None => return None
		};
		
		match log.get(pos) {
			Some(v) => {
				assert_eq!(v.index, index);
				Some(v.term)
			},
			None => None
		}
	}

	fn first_index(&self) -> Option<u64> {
		let log = self.log.lock().unwrap();

		match log.first() {
			Some(v) => Some(v.index),
			None => None
		}
	}

	fn last_index(&self) -> Option<u64> {
		let log = self.log.lock().unwrap();

		match log.last() {
			Some(v) => Some(v.index),
			None => None
		}
	}

	fn entry(&self, index: u64) -> Option<Arc<LogEntry>> {
		let log = self.log.lock().unwrap();

		// XXX: Basically why it is better to pass around arcs 
		// Simplest 

		let pos = match Self::pos_for(index, &log) {
			Some(v) => v,
			None => return None
		};
		
		match log.get(pos) {
			Some(v) => {
				assert_eq!(v.index, index);
				Some(v.clone())
			},
			None => None
		}
	}

	// the memory store will just assume that everything in the log is immediately durable
	fn match_index(&self) -> Option<u64> {
		self.last_index()
	}

	fn append(&self, entry: LogEntry) {
		let mut log = self.log.lock().unwrap();

		log.push(Arc::new(entry));
	}

	fn truncate_suffix(&self, start_index: u64) {
		let log = self.log.lock().unwrap();

		let pos = match Self::pos_for(start_index, &log) {
			Some(v) => v,
			None => panic!("Truncating starting at unknown position")
		};

		self.log.lock().unwrap().truncate(pos);
	}


	fn flush(&self) -> Result<()> {
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

