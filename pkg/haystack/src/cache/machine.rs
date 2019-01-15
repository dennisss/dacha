use super::super::common::*;
use super::super::errors::*;
use super::super::directory::*;
use super::super::background_thread::*;
use super::memory::*;
use std::time::Duration;
use std::sync::{Arc, Mutex};


pub struct MachineContext {
	pub id: MachineId,
	pub inst: Mutex<CacheMachine>,
	pub config: ConfigRef,
	pub thread: BackgroundThread
}

impl MachineContext {
	pub fn from(machine: CacheMachine) -> MachineContext {
		let config = machine.dir.config.clone();

		MachineContext {
			id: 0,
			inst: Mutex::new(machine),
			config,
			thread: BackgroundThread::new()
		}
	}
}

pub type MachineHandle = Arc<MachineContext>;



pub struct CacheMachine {

	pub id: MachineId,
	pub dir: Directory,
	pub port: u16,
	pub memory: MemoryStore

}




impl CacheMachine {

	pub fn load(dir: Directory, port: u16) -> Result<CacheMachine> {

		let mac = dir.db.create_cache_machine("127.0.0.1", port)?;

		let memory = MemoryStore::new(
			dir.config.cache.memory_size, dir.config.cache.max_entry_size,
			Duration::from_millis(dir.config.cache.max_age)
		);

		Ok(CacheMachine {
			id: mac.id as MachineId,
			dir,
			port,
			memory
		})
	}


	pub fn start(mac_handle_in: &MachineHandle) {

		let mac_handle = mac_handle_in.clone();
		mac_handle_in.thread.start(move || {

			while mac_handle.thread.is_running() {
				{
					let mac = mac_handle.inst.lock().unwrap();

					// TODO: Current issue is that blocking the entire machine for a long time will be very expensive during concurrent operations
					if let Err(e) = mac.do_heartbeat(true) {
						println!("{:?}", e);
					}
				}

				mac_handle.thread.wait(mac_handle.config.store.heartbeat_interval);
			}

			// Perform final heartbeart to take this node off of the ready list
			mac_handle.inst.lock().unwrap().do_heartbeat(false).expect("Failed to mark as not-ready");

		});
	}

	pub fn do_heartbeat(&self, ready: bool) -> Result<()> {

		self.dir.db.update_cache_machine_heartbeat(
			self.id,
			ready,
			"127.0.0.1", self.port
		)?;

		Ok(())
	}

}
