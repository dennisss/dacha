use bytes::Bytes;
use super::errors::*;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};


pub trait StateMachine {

	// TODO: Should probably have a check operation that validates an operation is good before a leader decide to commit them (either way we will still be consistent )	

	// ^ issue being that because operations are not independent, this would need to be checked per operation
	// So the alternative would be to require the StateMachine to implement an apply, revert, and commit


	/// Should apply the given operation to the state machine immediately integrating it
	/// 
	/// If the operation is invalid or otherwise can't be applied, then an error should be returned. We will assume that it atomically failed and may be retried in the future, but for the mean time this will stop further requests going to the state machine
	/// If the operation was already applied in the past, then it should be silently accepted
	fn apply(&mut self, id: u64, op: Bytes) -> Result<()>;

	/// Gets the index of the last operation applied to the state machine
	/// 
	/// Empty or non-persistent state machines should return None initially
	fn last_applied(&self) -> Option<u64>;

	// TODO: May also require operations to be able to install new snapshots

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
	last_id: Option<u64>,
	data: HashMap<Vec<u8>, Vec<u8>>
}

impl MemoryKVStateMachine {
	pub fn new() -> MemoryKVStateMachine {
		MemoryKVStateMachine {
			last_id: None,
			data: HashMap::new()
		}
	}

	/*
	pub fn new_op(op: &KeyValueOperation) -> Bytes {
		// Basically I want to 
	}
	*/
}

impl StateMachine for MemoryKVStateMachine {

	fn apply(&mut self, id: u64, data: Bytes) -> Result<()> {
		// TODO: Switch to using the common marshalling code
		let mut de = rmps::Deserializer::new(&data[..]);
		let ret: KeyValueOperation = Deserialize::deserialize(&mut de).unwrap();

		match ret {
			KeyValueOperation::Set { key, value } => {
				self.data.insert(key, value);
			},
			KeyValueOperation::Delete { key } => {
				self.data.remove(&key);
			}
		};

		Ok(())
	}

	fn last_applied(&self) -> Option<u64> {
		self.last_id
	}

}


