use std::io;
use std::io::{Write, Read, Seek};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, SeekFrom};
use std::collections::{HashMap};
use super::super::common::*;
use super::super::errors::*;
use super::super::paths::Host;
use super::api::*;
use super::volume::{PhysicalVolume};
use core::fs::DirLock;
use std::path::{Path, PathBuf};
use super::super::directory::Directory;
use core::FlipSign;
use std::sync::{Arc,Mutex,RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use rand::seq::SliceRandom;
use rand::Rng;
use super::machine_index::*;
use super::super::background_thread::*;
use common::futures::future;
use common::futures::future::*;
use common::futures::prelude::*;
use common::futures::compat::Future01CompatExt;
use std::future::Future;

pub struct MachineContext {
	pub id: MachineId, // < Same as the id of the 'inst', just externalized to avoid needing to lock the machine
	pub inst: RwLock<StoreMachine>,
	pub config: ConfigRef,
	pub dir: Mutex<Directory>,
	pub thread: BackgroundThread,

	/// Caches whether or not the store should qualify as 'writeable'. This is updated in the background thread that does heartbeats
	pub writeable: AtomicBool
}

impl MachineContext {
	pub fn from(store: StoreMachine, dir: Directory) -> MachineContext {
		let writeable = store.stats().can_write_soft();

		MachineContext {
			id: store.id(),
			inst: RwLock::new(store),
			config: dir.config.clone(),
			dir: Mutex::new(dir),
			thread: BackgroundThread::new(),
			writeable: AtomicBool::new(writeable)
		}
	}

	pub fn is_writeable(&self) -> bool {
		self.writeable.load(Ordering::SeqCst)
	}
}


// Whenever unlocking a volume, we will update the counter


pub type MachineHandle = Arc<MachineContext>;


/// Encapsulates the broad configuration and current state of a single store machine
pub struct StoreMachine {

	/// All volumes 
	pub volumes: HashMap<VolumeId, Arc<Mutex<PhysicalVolume>>>,

	_lock: DirLock,
	
	config: ConfigRef,

	port: u16,

	/// Location of all files on this machine
	folder: String,

	index: StoreMachineIndex

}

impl StoreMachine {

	/// Opens or creates a new store machine configuration based out of the given directory
	/// TODO: Long term this will also take a Directory client so that we can bootstrap everything from that 
	pub fn load(dir: &Directory, port: u16, folder: &str) -> Result<StoreMachine> {

		// Sanity checking sizes of all sizings
		// TODO: Move this somewhere else?
		assert!(dir.config.store.preallocate_size <= dir.config.store.allocation_size);
		assert!(dir.config.store.allocation_size <= dir.config.store.space);

		let path = Path::new(folder);

		let lock = DirLock::open(path)?;

		let volumes_path = path.join(String::from("volumes"));

		let idx = if volumes_path.exists() {
			StoreMachineIndex::open(&volumes_path)?
		} else {
			let machine = dir.db.create_store_machine("127.0.0.1", port)?;
			StoreMachineIndex::create(&volumes_path, dir.cluster_id, machine.id.flip())?
		};

		if dir.cluster_id != idx.cluster_id {
			return Err(err_msg("Connected to a different cluster"));
		}

		let mut machine = StoreMachine {
			_lock: lock,
			folder: String::from(folder),
			config: dir.config.clone(),
			index: idx,
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
			return Err(err_msg("Trying to open volume multiple times"));
		}

		let path = self.get_volume_path(volume_id);

		let vol = if path.exists() {
			PhysicalVolume::open(self.config.clone(), &path)?
		} else {
			PhysicalVolume::create(self.config.clone(), &path, self.index.cluster_id, self.index.machine_id, volume_id)?
		};

		// Verify that if we opened an existing file, that it wasn't from some other conflicting store
		if vol.superblock.volume_id != volume_id || vol.superblock.cluster_id != self.index.cluster_id {
			return Err(err_msg("Opened volume does not belong to this store"));
		}

		if expect_empty && vol.num_needles() > 0 {
			return Err(err_msg("Opened volume that we expected to be empty"));
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

	pub fn stats(&self) -> StoreMachineStats {

		let mut used = 0;
		let mut alloc = 0;

		let mut vol_stats = HashMap::new();
		vol_stats.reserve(self.volumes.len());

		for (id, v) in self.volumes.iter() {
			let v = v.lock().unwrap();

			vol_stats.insert(*id, StoreMachineVolumeStats {
				used_space: v.used_space(),
				allocated_space: v.superblock.allocated_space,
				can_write: v.can_write(),
				can_write_soft: v.can_write_soft()
			});
		}

		let total_space = self.config.store.space -
			(self.config.store.allocation_size * (self.config.store.allocation_reserved as u64));

		StoreMachineStats {
			config: self.config.clone(),
			volumes: vol_stats,
			total_space
		}
	}

	pub fn start(mac_handle_in: &MachineHandle) {
		// TODO: Another duty of this thread will be to start and run compactions (as it has to do the updating of compactions anyway)
	
		let mac_handle = mac_handle_in.clone();
		mac_handle_in.thread.start(move || {
			let mut pending_alloc = false;

			// TODO: On Ctrl-C, must mark as not-ready to stop this loop, issue one last heartbeat marking us as not ready and wait for all active http requests to finish
			// TODO: For allocation events, we do want this loop to be able to be woken up by a write action that causes the used space amount to tip over the max size
			while mac_handle.thread.is_running() {

	
				let (cur_should_alloc,) = {

					let (stats, port) = {
						let mac = mac_handle.inst.read().unwrap();
						(mac.stats(), mac.port)
					};


					// TODO: We should just acquire ownership of the directory, as this is the only thread that will actually ever use it (aside from the main thread which probably would like to reclaim ownership of the directory for shutdown)
					let dir = mac_handle.dir.lock().unwrap();

					// TODO: Current issue is that blocking the entire machine for a long time will be very expensive during concurrent operations
					// Hence why read-only machine access would be useful as it rarely ever needs to change
					if let Err(e) = StoreMachine::do_heartbeat(&mac_handle, port, &stats, &dir, true) {
						println!("{:?}", e);
					}

					if let Err(e) = StoreMachine::check_writeability(&mac_handle, &stats, &dir) {
						println!("{:?}", e);
					}

					(stats.should_allocate(),)
				};

				// NOTE: This is mainly done separately as 
				if pending_alloc {
					// If we still need to allocate (meaning that no other machine tried allocating since we last checked), then we will proceed with allocating
					if cur_should_alloc {

						let f = StoreMachine::perform_allocation(mac_handle.clone());
						tokio::run(
							f.map_err(|e| {
								println!("{:?}", e);
							})
						);

						// Notify ourselves as we probably just created a new volume 
						mac_handle.thread.notify();

						// Should should generally mean that at least 2 more cycles to perform additional allocations
						pending_alloc = false;
					}
				}
				else {
					pending_alloc = cur_should_alloc;
				}


				let mut time = mac_handle.config.store.heartbeat_interval;

				// Because many machines can be pending allocation all at once, we will randomly sleep some fraction of the heartbeat before trying to create a volume in order to avoid many machines trying to 
				if pending_alloc {
					// NOTE: .gen() should return a positive float from 0-1
					time = ((time as f64) * rand::thread_rng().gen::<f64>() * 0.25) as u64;
				}

				mac_handle.thread.wait(time);
			}

			// TODO: First mark all volumes on this machine as not writeable

			if let Err(e) = StoreMachine::shutdown(mac_handle) {
				eprintln!("Failed during node shutdown: {:?}", e);
			}
		});
	}

	// For shutting down inside of the thread (should consume the thread's mac_handle)
	fn shutdown(mac_handle: MachineHandle) -> Result<()> {

		let mac = mac_handle.inst.read().unwrap();
		let dir = mac_handle.dir.lock().unwrap();

		// Mark all volumes associated with this machine as read-only
		// (On restart it will be pitch-fork's responsibility to bring them back up)
		let vols = dir.db.read_logical_volumes_for_store_machine(mac_handle.id)?;
		for v in vols {
			dir.db.update_logical_volume_writeable(v.id as VolumeId, false)?;
		}

		// Perform final heartbeart to take this node off of the ready list
		// The main thing being that we don't really want this blocking with a hold of the machine
		StoreMachine::do_heartbeat(&mac_handle, mac.port, &mac.stats(), &dir, false)?;

		Ok(())
	}


	fn do_heartbeat(mac_handle: &MachineHandle, port: u16, stats: &StoreMachineStats, dir: &Directory, ready: bool) -> Result<()> {

		let writeable = stats.can_write_soft();
		mac_handle.writeable.store(writeable, Ordering::SeqCst);

		dir.db.update_store_machine_heartbeat(
			mac_handle.id,
			ready,
			"127.0.0.1", port,
			stats.allocated_space(),
			stats.total_space,
			writeable
		)?;

		Ok(())
	}

	/// Check that if for all volumes that exist on this machine, if any of them are close to being empty, then we need to mark them as being read-only remotely
	/// TODO: We could probably split this entirely once we mark every single volume as read-only until other constraints change
	fn check_writeability(mac_handle: &MachineHandle, stats: &StoreMachineStats, dir: &Directory) -> Result<()> {

		let vols = dir.db.read_logical_volumes_for_store_machine(mac_handle.id)?;

		for v in vols {
			let s = match stats.volumes.get(&(v.id as VolumeId)) {
				Some(v) => v,
				None => {
					eprintln!("Inconsistent volume not on this machine: {}", v.id);
					continue;
				}
			};

			let writeable = s.can_write_soft;

			if !writeable && v.write_enabled {
				dir.db.update_logical_volume_writeable(v.id as VolumeId, false)?;
			}
		}

		Ok(())
	}

	/// Assuming that we want to, this will create a new volume on this machine and pick other replicas to go along with it
	fn perform_allocation(mac_handle: MachineHandle) -> impl Future<Output=Result> + Send {

		let num_replicas = mac_handle.config.store.num_replicas;

		let (mut macs, vol) = {
			let dir = mac_handle.dir.lock().unwrap();

			let all_macs = match dir.db.index_store_machines() {
				Ok(v) => v,
				Err(e) => return Either::A(err(e))
			};

			let mut macs = all_macs.into_iter().filter(|m| {
				m.can_allocate(&dir.config) && (m.id as MachineId) != mac_handle.id
			}).collect::<Vec<_>>();

			if macs.len() < num_replicas - 1 {
				return Either::A(err("Not enough replicas available to allocate new volume".into()));
			}

			let vol = match dir.create_logical_volume() { Ok(v) => v, Err(e) => return Either::A(err(e)) };
			
			(macs, vol)
		};

		let vol_id = vol.id.flip();

		let mut rng = rand::thread_rng();

		// Random choice of which machines to choose as replicas
		// TODO: Possibly more useful to spear across less-allocated machines first (with randomness still though)
		macs.shuffle(&mut rng);

		// TODO: Should retry once for each machine
		// Also, if a machine fails, then we can proceed up to the next available machine (next in our random sequence)
		// Basically using the vector as a stream and take until we get some number of successes (but all in parallel)
		// NOTE: We assume that NUM_REPLICAS is always at least 1
		let n_other = if num_replicas > 1 { num_replicas - 1 } else { 0 };

		// Fanning out and making requests to all machines we need
		let arr = macs[0..n_other].iter().map(move |m| {
			let client = hyper::Client::new();

			let url = format!("{}{}", m.addr(), StorePath::Volume { volume_id: vol_id }.to_string() );
			let req = hyper::Request::builder()
				.uri(&url)
				.method("POST")
				.header("Host", Host::Store(m.id as MachineId).to_string())
				.body(hyper::Body::empty())
				.unwrap();

			client.request(req)
			.compat()
			.map_err(|e| e.into())
			.and_then(move |resp| {
				if !resp.status().is_success() {
					return err(format!("Failed to create volume on replica store #{}", vol_id).into());
				}

				return ok(())
			})
		}).collect::<Vec<_>>();

		Either::B(join_all(arr)
		.and_then(move |_| {
			let mut mac = mac_handle.inst.write().unwrap();
			let dir = mac_handle.dir.lock().unwrap();

			// Finally create the volume on ourselves
			if let Err(e) = mac.create_volume(vol_id) { return err(e); }

			// Get all machine ids (including ourselves) involved in the volume
			let mut all_ids = vec![mac.index.machine_id];
			all_ids.extend(macs[0..n_other].iter().map(|m| {
				m.id as MachineId
			}));

			// Create all physical mappings
			if let Err(e) = dir.db.create_physical_volumes(vol_id, &all_ids) { return err(e); }

			// Mark as writeable
			if let Err(e) = dir.db.update_logical_volume_writeable(vol_id, true) { return err(e); }

			ok(())
		}))
	}

}


pub struct StoreMachineVolumeStats {
	/// Total amount of space occupied on disk
	pub used_space: u64,
	
	/// Total amount of space on disk commited towards this volume but not necessarily fully used
	pub allocated_space: u64,

	pub can_write: bool,
	pub can_write_soft: bool
}

/// Represents a single snapshot of the store's space usage/allocations at one point in time
pub struct StoreMachineStats {
	// Mainly copied from the store machine for convenience
	pub config: ConfigRef,

	/// The total amount of space that we are allowed to allocate towards primary data files
	pub total_space: u64,

	pub volumes: HashMap<VolumeId, StoreMachineVolumeStats>
}

impl StoreMachineStats {
	pub fn used_space(&self) -> u64 {
		let mut sum = 0;
		for (_, v) in self.volumes.iter() {
			sum += v.used_space
		}

		sum
	}

	pub fn allocated_space(&self) -> u64 {
		let mut sum = 0;
		for (_, v) in self.volumes.iter() {
			sum += v.allocated_space
		}

		sum
	}

	pub fn can_allocate(&self) -> bool {
		self.allocated_space() + self.config.store.allocation_size < self.total_space
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

	/// Whether or not any value on this machine can be written within a reasonable margin of error
	/// NOTE: Machines with no volumes on them are NOT writeable currently (so we can't immediately accept uploads to a new volume until all machines have heartbeated again)
	pub fn can_write_soft(&self) -> bool {
		for (_, v) in self.volumes.iter() {
			if v.can_write_soft {
				return true;
			}
		}

		return false;
	}

	/// Whether or not any any volume volume in the entire store is writeable
	pub fn can_write(&self) -> bool {
		for (_, v) in self.volumes.iter() {
			if v.can_write {
				return true;
			}
		}

		return false;
	}

}
