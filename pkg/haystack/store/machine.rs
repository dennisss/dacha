extern crate fs2;

use std::io;
use std::io::{Write, Read, Seek};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, SeekFrom};
use std::collections::{HashMap};
use super::super::common::*;
use super::super::errors::*;
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
	pub volumes: HashMap<VolumeId, PhysicalVolume>,

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

		self.volumes.insert(volume_id, vol);

		Ok(())
	}

	pub fn create_volume(&mut self, volume_id: VolumeId) -> Result<()> {

		self.open_volume(volume_id, true)?;
		
		// We run this after the open_volume succeeds to gurantee that we don't try adding duplicate ids to the index
		// Currently we don't particularly care about inconsistencies with empty files with no corresponding index id
		self.index.add_volume_id(volume_id)?;

		Ok(())
	}


	pub fn start(mac_handle: Arc<Mutex<StoreMachine>>) {
		
		thread::spawn(move || {
			// TODO: On Ctrl-C, must mark as not-ready to stop this loop, issue one last heartbeat marking us as not ready and wait for all active http requests to finish
			loop {
				{
					let mut mac = mac_handle.lock().unwrap();
					if let Err(e) = mac.do_heartbeat() {
						println!("{:?}", e);
					}
				}

				let dur = time::Duration::from_millis(STORE_MACHINE_HEARTBEAT_INTERVAL);
				thread::sleep(dur);
			}
		});
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

	// TODO: We will have multiple degrees of writeability for the volumes and the machine itself
	pub fn can_write(&self) -> bool {
		true
	}

	pub fn do_heartbeat(&mut self) -> Result<()> {

		self.check_allocated()?;

		self.dir.db.update_store_machine_heartbeat(
			self.index.machine_id,
			true,
			"127.0.0.1", self.port,
			self.allocated_space() as u64,
			self.total_space() as u64,
			true
		)?;

		Ok(())
	}


	pub fn check_allocated(&mut self) -> Result<()> {
		// For now we will simply make sure that we have at least one volume

		if self.volumes.len() < 1 {
			let vol = self.dir.create_logical_volume()?;

			let vol_id = vol.id.to_unsigned();

			self.create_volume(vol_id)?;

			self.dir.db.create_physical_volume(
				vol_id,
				self.index.machine_id
			)?;

			self.dir.db.update_logical_volume_writeable(vol_id, true)?;
		}

		Ok(())
	}



}


