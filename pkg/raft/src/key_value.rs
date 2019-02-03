use bytes::Bytes;
use raft::errors::*;
use raft::protos::*;
use raft::state_machine::*;
use std::collections::HashMap;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use std::io::Read;
use futures::sync::oneshot;


#[derive(Serialize, Deserialize)]
pub enum KeyValueCheck {
	Exists,
	NonExistent,
	Version(LogIndex)
}

// A basic store for storing in-memory data
// Currently implemented for 
// Additionally a transaction may be composed of any number of non-transaction operations (typically these will have some type of additional )
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KeyValueOperation {
	Set {
		key: Vec<u8>,
		value: Vec<u8>,

		/// Optional check to perform before setting the key. The check must hold for the operation to succeed
		compare: Option<KeyValueCheck>,

		/// Expiration time in milliseconds
		expires: Option<SystemTime>
	},
	Delete {
		key: Vec<u8>
	}

	// May also have ops like Get, but those don't mutate the state so probably don't need to be explicitly requested
}

pub struct KeyValueReturn {
	pub success: bool
}

pub struct KeyValueData {
	pub version: LogIndex,
	pub expires: Option<SystemTime>,

	// XXX: May also be of different types (AKA mainly could be either a blob, set, or list in redis land)
	pub value: Bytes
}

/*
	Scaling Redis performance
	- Mainly would be based on the splitting of operations across multiple systems
		- Naturally if we support parititioning, then we can support 
	- Mixing consistency levels
		- Easiest to do this over specific key ranges as mixing consistency levels will end up downgrading the gurantees to the lowest consistency level available

*/

/// A simple key-value state machine implementation that provides most redis style functionality including atomic (multi-)key operations and transactions
/// NOTE: This does not 
pub struct MemoryKVStateMachine {
	// Better to also hold on to a version and possibly 
	data: Mutex<HashMap<Vec<u8>, Bytes>>
}

impl MemoryKVStateMachine {
	pub fn new() -> MemoryKVStateMachine {
		MemoryKVStateMachine {
			data: Mutex::new(HashMap::new())
		}
	}

	/// Very simple, non-linearizable read operation
	pub fn get(&self, key: &[u8]) -> Option<Bytes> {
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

impl StateMachine<KeyValueReturn> for MemoryKVStateMachine {

	// XXX: It would be useful to have a time and an index just for the sake of versioning of it 
	fn apply(&self, index: LogIndex, data: &[u8]) -> Result<KeyValueReturn> {
		// TODO: Switch to using the common marshalling code
		let mut de = rmps::Deserializer::new(data);
		let ret: KeyValueOperation = Deserialize::deserialize(&mut de)
			.map_err(|_| Error::from("Failed to deserialize command"))?;

		let mut map = self.data.lock().unwrap();

		// Could be split into a check phase and a run phase
		// Thus we can maintain transactions without lock 

		Ok(match ret {
			KeyValueOperation::Set { key, value, compare, expires } => {

				map.insert(key, value.into());

				KeyValueReturn { success: true }
			},
			KeyValueOperation::Delete { key } => {
				let old = map.remove(&key);

				KeyValueReturn { success: old.is_some() }
			}
		})
	}

	fn snapshot<'a>(&'a self) -> Option<StateMachineSnapshot<'a>> {
		None
	}

	fn restore(&self, data: Bytes) -> Result<()> {
		// A snapshot should not have been generatable
		Ok(())
	}

}
