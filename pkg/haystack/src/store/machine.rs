use std::io;
use std::io::{Write, Read, Seek};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, SeekFrom};
use std::collections::{HashMap};
use super::super::common::*;
use super::super::errors::*;
use super::super::paths::Host;
use super::volume::{PhysicalVolume};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use fs2::FileExt;
use std::path::{Path, PathBuf};
use std::mem::size_of;
use super::super::directory::Directory;
use bitwise::Word;
use std::sync::{Arc,Mutex};
use std::sync::atomic::{AtomicUsize};
use rand::seq::SliceRandom;
use rand::Rng;
use super::machine_index::*;
use super::super::background_thread::*;


pub struct MachineContext {
	pub id: MachineId,
	pub inst: Mutex<StoreMachine>,
	pub thread: BackgroundThread,

	/// Number of volumes in this machine that are writeable (soft-writeable)
	/// Will be updated in the routes whenever a write operation occurs
	/// TODO: We do not use this thing yet
	pub nwriteable: AtomicUsize,
}

impl MachineContext {
	pub fn from(store: StoreMachine) -> MachineContext {
		let nwrite = 0;


		MachineContext {
			id: store.id(),
			inst: Mutex::new(store),
			thread: BackgroundThread::new(),
			nwriteable: AtomicUsize::new(0)
		}
	}
}


// Whenever unlocking a volume, we will update the counter


pub type MachineHandle = Arc<MachineContext>;


/// Encapsulates the broad configuration and current state of a single store machine
pub struct StoreMachine {

	/// All volumes 
	pub volumes: HashMap<VolumeId, Arc<Mutex<PhysicalVolume>>>,

	dir: Directory,

	port: u16,

	/// Location of all files on this machine
	folder: String,

	index: VolumesIndex

}

impl StoreMachine {

	/// Opens or creates a new store machine configuration based out of the given directory
	/// TODO: Long term this will also take a Directory client so that we can bootstrap everything from that 
	pub fn load(dir: Directory, port: u16, folder: &str) -> Result<StoreMachine> {

		let path = Path::new(folder);
		if !path.exists() {
			return Err("Store data folder does not exist".into());	
		}

		let volumes_path = path.join(String::from("volumes"));

		// Before we create a lock file, verify that the directory is empty if this is a new store
		if !volumes_path.exists() {
		if path.read_dir()?.collect::<Vec<_>>().len() > 0 {
				return Err("Store folder is not empty".into());
			}
		}


		let mut opts = OpenOptions::new();
		opts.write(true).create(true).read(true);

		let lockfile = opts.open(path.join(String::from("lock")))?;

		match lockfile.try_lock_exclusive() {
			Ok(_) => true,
			Err(err) => return Err(err.into())
		};

		
		let idx = if volumes_path.exists() {
			VolumesIndex::open(&volumes_path)?
		} else {
			let machine = dir.db.create_store_machine("127.0.0.1", port)?;
			VolumesIndex::create(&volumes_path, dir.cluster_id, machine.id.to_unsigned())?
		};

		if dir.cluster_id != idx.cluster_id {
			return Err("Connected to a different cluster".into());
		}

		let mut machine = StoreMachine {
			folder: String::from(folder),
			index: idx,
			dir,
			port,
			volumes: HashMap::new()
		};

		let vol_ids = machine.index.read_all()?;
		for id in vol_ids {
			machine.open_volume(id, false)?;
		}

		Ok(machine)
	}

	pub fn id(&self) -> MachineId {
		self.index.machine_id
	}

	fn get_volume_path(&self, volume_id: VolumeId) -> PathBuf {
		Path::new(&self.folder).join(String::from("haystack_") + &volume_id.to_string())
	}

	fn open_volume(&mut self, volume_id: VolumeId, expect_empty: bool) -> Result<()> {

		if self.volumes.contains_key(&volume_id) {
			return Err("Trying to open volume multiple times".into());
		}

		let path = self.get_volume_path(volume_id);

		let vol = if path.exists() {
			PhysicalVolume::open(&path)?
		} else {
			PhysicalVolume::create(&path, self.index.cluster_id, self.index.machine_id, volume_id)?
		};

		// Verify that if we opened an existing file, that it wasn't from some other conflicting store
		if vol.superblock.volume_id != volume_id || vol.superblock.cluster_id != self.index.cluster_id {
			return Err("Opened volume does not belong to this store".into());
		}

		if expect_empty && vol.num_needles() > 0 {
			return Err("Opened volume that we expected to be empty".into());
		}

		self.volumes.insert(volume_id, Arc::new(Mutex::from(vol)));

		Ok(())
	}

	pub fn create_volume(&mut self, volume_id: VolumeId) -> Result<()> {

		self.open_volume(volume_id, true)?;
		
		// We run this after the open_volume succeeds to gurantee that we don't try adding duplicate ids to the index
		// Currently we don't particularly care about inconsistencies with empty files with no corresponding index id
		self.index.add_volume_id(volume_id)?;

		Ok(())
	}

	pub fn start(mac_handle_in: &MachineHandle) {
		// TODO: Another duty of this thread will be to start and run compactions (as it has to do the updating of compactions anyway)
	
		let mac_handle = mac_handle_in.clone();
		mac_handle_in.thread.start(move || {
			let mut pending_alloc = false;

			// TODO: On Ctrl-C, must mark as not-ready to stop this loop, issue one last heartbeat marking us as not ready and wait for all active http requests to finish
			// TODO: For allocation events, we do want this loop to be able to be woken up by a write action that causes the used space amount to tip over the max size
			while mac_handle.thread.is_running() {
				{
					let mut mac = mac_handle.inst.lock().unwrap();

					// TODO: Current issue is that blocking the entire machine for a long time will be very expensive during concurrent operations
					if let Err(e) = mac.do_heartbeat(true) {
						println!("{:?}", e);
					}

					if let Err(e) = mac.check_writeability() {
						println!("{:?}", e);
					}

					if pending_alloc {
						// If we still need to allocate (meaning that no other machine tried allocating since we last checked), then we will proceed with allocating
						if mac.should_allocate() {
							if let Err(e) = mac.perform_allocation() {
								println!("{:?}", e);
							}

							// Should should generally mean that at least 2 more cycles to perform additional allocations
							pending_alloc = false;
						}
					}
					else {
						pending_alloc = mac.should_allocate();
					}
				}

				let mut time = STORE_MACHINE_HEARTBEAT_INTERVAL;

				// Because many machines can be pending allocation all at once, we will randomly sleep some fraction of the heartbeat before trying to create a volume in order to avoid many machines trying to 
				if pending_alloc {
					// NOTE: .gen() should return a positive float from 0-1
					time = ((time as f64) * rand::thread_rng().gen::<f64>()) as u64;
				}


				mac_handle.thread.wait(time);

				//thread::sleep(dur);
			}

			// Perform final heartbeart to take this node off of the ready list
			mac_handle.inst.lock().unwrap().do_heartbeat(false).expect("Failed to mark as not-ready");

		});
	}

	/// Gets the total amount of occupied space on disk
	/// NOTE: This is an expensive heavily locking operation and generally not be used for normal operations
	pub fn used_space(&self) -> usize {

		// TODO: Likewise need to be marking volumes that are out of space as fully write-only in the database

		let mut sum = 0;

		for (_, v) in self.volumes.iter() {
			sum += v.lock().unwrap().used_space();
		}

		sum
	}

	pub fn allocated_space(&self) -> usize {
		let mut sum = 0;

		for (_, v) in self.volumes.iter() {
			sum += ALLOCATION_SIZE;
		}

		sum
	}

	pub fn total_space(&self) -> usize {
		STORE_MACHINE_SPACE - (ALLOCATION_SIZE * ALLOCATION_RESERVED)
	}

	/*
	pub fn num_write_soft(&self) -> bool {
		// TODO: We will only ask for a lock on everything during the heartbeats
	
	}
	*/

	/// Whether or not any volue on this machine can be written within a reasonable margin of error
	/// NOTE: Machines with no volumes on them are NOT writeable currently (so we can't immediately accept uploads to a new volume until all machines have heartbeated again)
	pub fn can_write_soft(&self) -> bool {
		for (_, v) in self.volumes.iter() {
			if v.lock().unwrap().can_write_soft() {
				return true;
			}
		}

		return false;
	}

	/// Whether or not any any volume volume in the entire store is writeable
	pub fn can_write(&self) -> bool {
		for (_, v) in self.volumes.iter() {
			if v.lock().unwrap().can_write() {
				return true;
			}
		}

		return false;
	}

	pub fn can_allocate(&self) -> bool {
		self.allocated_space() + ALLOCATION_SIZE < self.total_space()
	}

	pub fn do_heartbeat(&self, ready: bool) -> Result<()> {

		self.dir.db.update_store_machine_heartbeat(
			self.index.machine_id,
			ready,
			"127.0.0.1", self.port,
			self.allocated_space() as u64,
			self.total_space() as u64,
			self.can_write_soft()
		)?;

		Ok(())
	}

	/// Check that if for all volumes that exist on this machine, if any of them are close to being empty, then we need to mark them as being read-only remotely
	/// TODO: We could probably split this entirely once we mark every single volume as read-only until other constraints change
	pub fn check_writeability(&self) -> Result<()> {

		let vols = self.dir.db.read_logical_volumes_for_store_machine(self.id())?;

		for v in vols {
			let vol_handle = match self.volumes.get(&(v.id as VolumeId)) {
				Some(v) => v,
				None => {
					eprintln!("Inconsistent volume not on this machine: {}", v.id);
					continue;
				}
			};

			let vol = vol_handle.lock().unwrap();

			let writeable = vol.can_write_soft();

			if !writeable && v.write_enabled {
				self.dir.db.update_logical_volume_writeable(vol.superblock.volume_id, false)?;
			}
		}

		Ok(())
	}

	/// Check whether or not we should allocate another volume on this machine
	/// While the machine is not fully allocated, we will ensure that the machine never goes above 70% space utilization
	/// NOTE: Currently this will require a lock on all volumes unfortunately
	pub fn should_allocate(&self) -> bool {
		// Ignore if we don't have space to allocate more volumes on this machine
		if !self.can_allocate() {
			return false;
		}

		let need_more = self.volumes.len() < 1 ||
			(self.used_space() as f64) > ((self.allocated_space() as f64) * 0.70);

		need_more
	}

	/// Assuming that we want to, this will create a new volume on this machine and pick other replicas to go along with it
	pub fn perform_allocation(&mut self) -> Result<()> {
		let mut rng = rand::thread_rng();

		let mut macs = self.dir.db.index_store_machines()?.into_iter().filter(|m| {
			m.can_allocate() && (m.id as MachineId) != self.id()
		}).collect::<Vec<_>>();

		if macs.len() < NUM_REPLICAS - 1 {
			return Err("Not enough replicas available to allocate new volume".into());
		}

		let vol = self.dir.create_logical_volume()?;
		let vol_id = vol.id.to_unsigned();

		// Random choice of which machines to choose as replicas
		// TODO: Possibly more useful to spear across less-allocated machines first (with randomness still though)
		macs.shuffle(&mut rng);

		let client = reqwest::Client::new();

		// TODO: Should retry once for each machine
		// Also, if a machine fails, then we can proceed up to the next available machine (next in our random sequence)
		// NOTE: We assume that NUM_REPLICAS is always at least 1
		let n_other = if NUM_REPLICAS > 1 { NUM_REPLICAS - 1 } else { 0 };
		let chosen_macs = &macs[0..n_other];
		for m in chosen_macs {
			// TODO: Must standardize all of these api stuff
			let url = format!("http://{}/{}", m.addr(), vol_id);
			let res = client
				.post(&url)
				.header("Host", Host::Store(m.id as MachineId).to_string())
				.send()?;
			
			if !res.status().is_success() {
				return Err(format!("Failed to create volume on replica store #{}", vol_id).into());
			}
		}

		// Finally create the volume on ourselves
		self.create_volume(vol_id)?;

		// Create send mapping
		self.dir.db.create_physical_volume(
			vol_id,
			self.index.machine_id
		)?;

		// Creating mapping for all other machines
		for m in chosen_macs {
			self.dir.db.create_physical_volume(
				vol_id,
				m.id as MachineId
			)?;
		}

		// Mark as writeable
		self.dir.db.update_logical_volume_writeable(vol_id, true)?;

		Ok(())
	}



}


