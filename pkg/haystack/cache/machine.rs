use super::super::common::*;
use super::super::errors::*;
use super::super::directory::*;
use super::memory::*;
use std::time::Duration;
use std::sync::{Arc, Mutex};


pub struct MachineContext {
	//pub id: MachineId,
	pub inst: Mutex<CacheMachine>
}

pub type MachineHandle = Arc<MachineContext>;

pub struct CacheMachine {

	pub dir: Directory,
	pub port: u16,
	pub memory: MemoryStore

}

impl CacheMachine {

	pub fn load(dir: Directory, port: u16) -> Result<CacheMachine> {
		Ok(CacheMachine {
			dir,
			port,
			memory: MemoryStore::new(CACHE_MEMORY_SIZE, CACHE_MAX_ENTRY_SIZE, Duration::from_millis(CACHE_MAX_AGE))
		})
	}

}
