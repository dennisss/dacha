use std::sync::Arc;
use std::time::Duration;

use common::async_std::sync::Mutex;
use common::errors::*;

use crate::background_thread::*;
use crate::cache::memory::*;
use crate::directory::*;
use crate::types::*;

pub struct MachineContext {
    pub id: MachineId,
    pub inst: Mutex<CacheMachine>,
    pub config: ConfigRef,
    pub thread: BackgroundThread,
}

impl MachineContext {
    pub fn from(machine: CacheMachine) -> MachineContext {
        let config = machine.dir.config.clone();

        MachineContext {
            id: 0,
            inst: Mutex::new(machine),
            config,
            thread: BackgroundThread::new(),
        }
    }
}

pub type MachineHandle = Arc<MachineContext>;

pub struct CacheMachine {
    pub id: MachineId,
    pub dir: Directory,
    pub port: u16,
    pub memory: MemoryStore,
}

impl CacheMachine {
    pub fn load(dir: Directory, port: u16) -> Result<CacheMachine> {
        let mac = dir.db.create_cache_machine("127.0.0.1", port)?;

        let memory = MemoryStore::new(
            dir.config.cache().memory_size() as usize,
            dir.config.cache().max_entry_size() as usize,
            Duration::from_millis(dir.config.cache().max_age()),
        );

        Ok(CacheMachine {
            id: mac.id as MachineId,
            dir,
            port,
            memory,
        })
    }

    pub async fn start(mac_handle: &MachineHandle) {
        mac_handle
            .thread
            .start(Self::run_thread(mac_handle.clone()))
            .await;
    }

    async fn run_thread(mac_handle: MachineHandle) {
        while mac_handle.thread.is_running() {
            {
                let mac = mac_handle.inst.lock().await;

                // TODO: Current issue is that blocking the entire machine for a long time will
                // be very expensive during concurrent operations
                if let Err(e) = mac.do_heartbeat(true) {
                    println!("{:?}", e);
                }
            }

            mac_handle
                .thread
                .wait(mac_handle.config.store().heartbeat_interval())
                .await;
        }

        // Perform final heartbeart to take this node off of the ready list
        mac_handle
            .inst
            .lock()
            .await
            .do_heartbeat(false)
            .expect("Failed to mark as not-ready");
    }

    pub fn do_heartbeat(&self, ready: bool) -> Result<()> {
        self.dir
            .db
            .update_cache_machine_heartbeat(self.id, ready, "127.0.0.1", self.port)?;

        Ok(())
    }
}
