use bytes::Bytes;
use super::errors::*;
use std::collections::HashMap;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Read;
use futures::sync::oneshot;


pub trait StateMachine {

	// TODO: Should probably have a check operation that validates an operation is good before a leader decide to commit them (either way we will still be consistent )	

	// ^ issue being that because operations are not independent, this would need to be checked per operation
	// So the alternative would be to require the StateMachine to implement an apply, revert, and commit


	/// Should apply the given operation to the state machine immediately integrating it
	fn apply(&self, op: &[u8]) -> Result<()>;

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


// A basic store 
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KeyValueOperation {
	Set {
		key: Vec<u8>,
		value: Vec<u8>
	},
	Delete {
		key: Vec<u8>
	}

	// May also have ops like Get, but those don't mutate the state so probably don't need to be explicitly requested
}


pub struct MemoryKVStateMachine {
	data: Mutex<HashMap<Vec<u8>, Vec<u8>>>
}

impl MemoryKVStateMachine {
	pub fn new() -> MemoryKVStateMachine {
		MemoryKVStateMachine {
			data: Mutex::new(HashMap::new())
		}
	}

	/// Very simple, non-linearizable read operation
	pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
		let data = self.data.lock().unwrap();
		
		// TODO: Probably inefficient (probably better to return an Arc)
		data.get(key).map(|v| v.clone())
	}

	/*
	pub fn new_op(op: &KeyValueOperation) -> Bytes {
		// Basically I want to 
	}
	*/
}

impl StateMachine for MemoryKVStateMachine {

	fn apply(&self, data: &[u8]) -> Result<()> {
		// TODO: Switch to using the common marshalling code
		let mut de = rmps::Deserializer::new(data);
		let ret: KeyValueOperation = Deserialize::deserialize(&mut de).unwrap();

		let mut map = self.data.lock().unwrap();

		match ret {
			KeyValueOperation::Set { key, value } => {
				map.insert(key, value);
			},
			KeyValueOperation::Delete { key } => {
				map.remove(&key);
			}
		};

		Ok(())
	}

	fn snapshot<'a>(&'a self) -> Option<StateMachineSnapshot<'a>> {
		None
	}

	fn restore(&self, data: Bytes) -> Result<()> {
		// A snapshot should not have been generatable
		Ok(())
	}

}


