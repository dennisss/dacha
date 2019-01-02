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

		// Should round exactly to u64 offsets
		if buf.len() % 8 != 0 {
			return Err("Volumes index is corrupt".into());
		}

		let mut out = Vec::new();


		let size = buf.len() / 8;
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

	/// Location of all files on this machine
	volumes_dir: String,

	index: VolumesIndex

}

impl StoreMachine {

	/// Opens or creates a new store machine configuration based out of the given directory
	/// TODO: Long term this will also take a Directory client so that we can bootstrap everything from that 
	pub fn load(dir: &mut Directory, folder: &str) -> Result<StoreMachine> {

		let mut opts = OpenOptions::new();
		opts.write(true).create(true).read(true);

		let lockfile = opts.open(Path::new(folder).join(String::from("lock")))?;

		match lockfile.try_lock_exclusive() {
			Ok(_) => true,
			Err(err) => return Err(err.into())
		};

		let volumes_path = Path::new(folder).join(String::from("volumes"));
		
		let idx = if volumes_path.exists() {
			VolumesIndex::open(&volumes_path)?
		} else {
			let machine = dir.create_store_machine()?;
			VolumesIndex::create(&volumes_path, dir.cluster_id, machine.id.to_unsigned())?
		};

		// Not we want to valid t

		let mut machine = StoreMachine {
			volumes_dir: String::from(folder),
			index: idx,
			volumes: HashMap::new()
		};

		let vol_ids = machine.index.read_all()?;
		for id in vol_ids {
			machine.open_volume(id, false)?;
		}

		Ok(machine)
	}

	fn get_volume_path(&self, volume_id: VolumeId) -> PathBuf {
		Path::new(&self.volumes_dir).join(String::from("haystack_") + &volume_id.to_string())
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

		if expect_empty && vol.len_needles() > 0 {
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



}


