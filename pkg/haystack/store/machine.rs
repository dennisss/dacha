extern crate fs2;

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
use std::thread;
use std::time;
use std::sync::{Arc,Mutex};
use std::sync::atomic::AtomicUsize;
use rand::seq::SliceRandom;
use rand::Rng;


pub struct MachineContext {
	pub id: MachineId,
	pub inst: Mutex<StoreMachine>,

	/// Number of volumes in this machine that are writeable
	/// Will be updated in the routes whenever a write operation occurs
	pub nwriteable: AtomicUsize
}

impl MachineContext {
	pub fn from(store: StoreMachine) -> MachineContext {
		let nwrite = 0;


		MachineContext {
			id: store.id(),
			inst: Mutex::new(store),
			nwriteable: AtomicUsize::new(0)
		}
	}
}


// Whenever unlocking a volume, we will update the counter


pub type MachineHandle = Arc<MachineContext>;

const VOLUMES_MAGIC: &str = "HAYV";
const VOLUMES_MAGIC_SIZE: usize = 4;

const VOLUMES_HEADER_SIZE: usize =
	VOLUMES_MAGIC_SIZE +
	size_of::<FormatVersion>() +
	size_of::<ClusterId>() +
	size_of::<MachineId>();

/// File that stores the list of all 
struct VolumesIndex {
	pub cluster_id: ClusterId,
	pub machine_id: MachineId,
	file: File

	// TODO: We also want to store the amount of space allocated to each physical volume and also run a crc over all of the data in this file
	// considerng tha this is the only real meaningful file, we should ensure that it always checks out
}

impl VolumesIndex {

	fn open(path: &Path) -> Result<VolumesIndex> {
		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut f = opts.open(path)?;

		let mut header = [0u8; VOLUMES_HEADER_SIZE];
		f.read_exact(&mut header)?;

		if &header[0..VOLUMES_MAGIC_SIZE] != VOLUMES_MAGIC.as_bytes() {
			return Err("Volumes magic is incorrect".into());
		}

		let mut c = Cursor::new(&header[VOLUMES_MAGIC_SIZE..]);

		let version = c.read_u32::<LittleEndian>()?;
		let cluster_id = c.read_u64::<LittleEndian>()?;
		let machine_id = c.read_u32::<LittleEndian>()?;

		if version != CURRENT_FORMAT_VERSION {
			return Err("Volumes version is incorrect".into());
		}


		let idx = VolumesIndex {
			cluster_id,
			machine_id,
			file: f
		};

		Ok(idx)
	}

	// Need to do a whole lot right here
	fn create(path: &Path, cluster_id: ClusterId, machine_id: MachineId) -> Result<VolumesIndex> {
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let mut f = opts.open(path)?;

		f.write_all(VOLUMES_MAGIC.as_bytes())?;
		f.write_u32::<LittleEndian>(CURRENT_FORMAT_VERSION)?;
		f.write_u64::<LittleEndian>(cluster_id)?;
		f.write_u32::<LittleEndian>(machine_id)?;

		// TODO: Should probably also flush the directory as well?
		f.flush()?;

		Ok(VolumesIndex {
			cluster_id: cluster_id.clone(),
			machine_id: machine_id.clone(),
			file: f
		})
	}

	/// Get all volume ids referenced in this index
	/// It's someone else's problem to ensure that there are no duplicates
	fn read_all(&mut self) -> Result<Vec<VolumeId>> {
		
		self.file.seek(SeekFrom::Start(VOLUMES_HEADER_SIZE as u64))?;

		let mut buf = Vec::new();
		self.file.read_to_end(&mut buf)?;

		// Should round exactly to id
		if buf.len() % size_of::<VolumeId>() != 0 {
			return Err("Volumes index is corrupt".into());
		}

		let mut out = Vec::new();


		let size = buf.len() / size_of::<VolumeId>();
		let mut cur = Cursor::new(buf);
		for _ in 0..size {
			out.push(cur.read_u32::<LittleEndian>()?);
		}

		Ok(out)
	}

	fn add_volume_id(&mut self, id: VolumeId) -> io::Result<()> {
		self.file.seek(SeekFrom::End(0))?;
		self.file.write_u32::<LittleEndian>(id)?;
		self.file.flush()?;
		Ok(())
	}

}


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
			let machine = dir.create_store_machine()?;
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
		if vol.volume_id != volume_id || vol.cluster_id != self.index.cluster_id {
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

	pub fn start(mac_handle: &MachineHandle) {
		let mac_handle = mac_handle.clone();
		thread::spawn(move || {
			let mut pending_alloc = false;

			// TODO: On Ctrl-C, must mark as not-ready to stop this loop, issue one last heartbeat marking us as not ready and wait for all active http requests to finish
			// TODO: For allocation events, we do want this loop to be able to be woken up by a write action that causes the used space amount to tip over the max size
			loop {
				{
					let mut mac = mac_handle.inst.lock().unwrap();

					// TODO: Current issue is that blocking the entire machine for a long time will be very expensive during concurrent operations
					if let Err(e) = mac.do_heartbeat() {
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

				let dur = time::Duration::from_millis(time);
				thread::sleep(dur);
			}
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

	pub fn can_write_volume_soft(&self, vol: &PhysicalVolume) -> bool {
		(((vol.used_space() as f64) * 0.95) as usize) < ALLOCATION_SIZE
	}

	pub fn can_write_volume(&self, vol: &PhysicalVolume) -> bool {
		vol.used_space() < ALLOCATION_SIZE
	}

	/// Whether or not any volue on this machine can be written within a reasonable margin of error
	pub fn can_write_soft(&self) -> bool {
		for (_, v) in self.volumes.iter() {
			if self.can_write_volume_soft(&v.lock().unwrap()) {
				return true;
			}
		}

		return false;
	}

	/// Whether or not any any volume volume in the entire store is writeable
	pub fn can_write(&self) -> bool {
		for (_, v) in self.volumes.iter() {
			if self.can_write_volume(&v.lock().unwrap()) {
				return true;
			}
		}

		return false;
	}

	pub fn can_allocate(&self) -> bool {
		self.allocated_space() + ALLOCATION_SIZE < self.total_space()
	}

	pub fn do_heartbeat(&mut self) -> Result<()> {

		self.dir.db.update_store_machine_heartbeat(
			self.index.machine_id,
			true,
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

			let writeable = self.can_write_volume_soft(&vol);

			if !writeable && v.write_enabled {
				self.dir.db.update_logical_volume_writeable(vol.volume_id, false)?;
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
			println!("Not enough replicas available to allocate new volume");
		}

		let vol = self.dir.create_logical_volume()?;
		let vol_id = vol.id.to_unsigned();

		// Random choice of which machines to choose as replicas
		// TODO: Possibly more useful to spear across less-allocated machines first (with randomness still though)
		macs.shuffle(&mut rng);

		let client = reqwest::Client::new();

		// TODO: Should retry once for each machine
		// Also, if a machine fails, then we can proceed up to the next available machine (next in our random sequence)
		let chosen_macs = &macs[0..(NUM_REPLICAS - 1)];
		for m in chosen_macs {
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


