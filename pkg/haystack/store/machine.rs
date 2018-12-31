extern crate fs2;

use std::io;
use std::io::{Write, Read, Seek};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, SeekFrom};
use std::collections::{HashMap};
use super::common::*;
use super::volume::{HaystackPhysicalVolume};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use fs2::FileExt;
use std::path::{Path, PathBuf};

const HAYSTACK_VOLUMES_MAGIC: &str = "HAYV";

// magic + version + machine_id + cluster_id
const HAYSTACK_VOLUMES_HEADER_SIZE: usize = 4 + 4 + 8 + 16;

const FORMAT_VERSION: u32 = 1;

// TODO: Ask the directory to allocate a machine id if we a brand new node
type MachineId = [u8; 8];

/// File that stores the list of all 
struct HaystackVolumesIndex {
	pub cluster_id: ClusterId,
	pub machine_id: MachineId,
	file: File
}

impl HaystackVolumesIndex {

	fn open(path: &Path) -> io::Result<HaystackVolumesIndex> {
		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut f = opts.open(path)?;

		let mut header = [0u8; HAYSTACK_VOLUMES_HEADER_SIZE];
		f.read_exact(&mut header)?;

		if &header[0..HAYSTACK_VOLUMES_MAGIC.len()] != HAYSTACK_VOLUMES_MAGIC.as_bytes() {
			return Err(io::Error::new(io::ErrorKind::Other, "Volumes magic is incorrect"));
		}

		let mut c = Cursor::new(&header[HAYSTACK_VOLUMES_MAGIC.len()..]);

		let version = c.read_u32::<LittleEndian>()?;

		if version != FORMAT_VERSION {
			return Err(io::Error::new(io::ErrorKind::Other, "Volumes version is incorrect"));
		}

		let mut idx = HaystackVolumesIndex {
			cluster_id: [0u8; 16],
			machine_id: [0u8; 8],
			file: f
		};

		c.read_exact(&mut idx.machine_id)?;
		c.read_exact(&mut idx.cluster_id)?;

		Ok(idx)
	}

	// Need to do a whole lot right here
	fn create(path: &Path, cluster_id: &ClusterId, machine_id: &MachineId) -> io::Result<HaystackVolumesIndex> {
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let mut f = opts.open(path)?;

		f.write_all(HAYSTACK_VOLUMES_MAGIC.as_bytes())?;
		f.write_u32::<LittleEndian>(FORMAT_VERSION)?;
		f.write_all(machine_id)?;
		f.write_all(cluster_id)?;

		// TODO: Should probably also flush the directory as well?
		f.flush()?;

		Ok(HaystackVolumesIndex {
			cluster_id: cluster_id.clone(),
			machine_id: machine_id.clone(),
			file: f
		})
	}

	/// Get all volume ids referenced in this index
	/// It's someone else's problem to ensure that there are no duplicates
	fn read_all(&mut self) -> io::Result<Vec<u64>> {
		
		self.file.seek(SeekFrom::Start(HAYSTACK_VOLUMES_HEADER_SIZE as u64));
	
		let mut buf = Vec::new();
		self.file.read_to_end(&mut buf)?;

		// Should round exactly to u64 offsets
		if buf.len() % 8 != 0 {
			return Err(io::Error::new(io::ErrorKind::Other, "Volumes index is corrupt"));
		}

		let mut out = Vec::new();


		let size = buf.len() / 8;
		let mut cur = Cursor::new(buf);
		for _ in 0..size {
			out.push(cur.read_u64::<LittleEndian>()?);
		}

		Ok(out)
	}

	fn add_volume_id(&mut self, id: u64) -> io::Result<()> {
		self.file.seek(SeekFrom::End(0));
		self.file.write_u64::<LittleEndian>(id)?;
		self.file.flush()?;
		Ok(())
	}

}


/// Encapsulates the broad configuration and current state of a single store machine
pub struct HaystackStoreMachine {

	/// Unique identifies all files in 
	//cluster_id: ClusterId,

	/// Location of all physical volume volumes
	volumes_dir: String,

	/// All volumes 
	pub volumes: HashMap<u64, HaystackPhysicalVolume>,

	index: HaystackVolumesIndex

}

impl HaystackStoreMachine {

	/// Opens or creates a new store machine configuration based out of the given directory
	/// TODO: Long term this will also take a Directory client so that we can bootstrap everything from that 
	pub fn load(dir: &str) -> io::Result<HaystackStoreMachine> {

		let mut opts = OpenOptions::new();
		opts.write(true).create(true).read(true);

		let lockfile = opts.open(Path::new(dir).join(String::from("lock")))?;

		match lockfile.try_lock_exclusive() {
			Ok(_) => true,
			Err(err) => return Err(err)
		};

		let volumes_path = Path::new(dir).join(String::from("volumes"));
		
		// Otherwise better to carry over an owned version
		let idx = if volumes_path.exists() {
			HaystackVolumesIndex::open(&volumes_path)?
		} else {
			
			// TODO: Ask the directory for a set of ids
			let cluster_id = [0u8; 16];
			let machine_id = [0u8; 8];

			HaystackVolumesIndex::create(&volumes_path, &cluster_id, &machine_id)?
		};

		let mut machine = HaystackStoreMachine {
			volumes_dir: String::from(dir),
			index: idx,
			volumes: HashMap::new()
		};

		let vol_ids = machine.index.read_all()?;
		for id in vol_ids {
			machine.open_volume(id, false)?;
		}

		Ok(machine)
	}

	fn get_volume_path(&self, volume_id: u64) -> PathBuf {
		Path::new(&self.volumes_dir).join(String::from("haystack_") + &volume_id.to_string())
	}

	fn open_volume(&mut self, volume_id: u64, expect_empty: bool) -> io::Result<()> {

		if self.volumes.contains_key(&volume_id) {
			return Err(io::Error::new(io::ErrorKind::Other, "Trying to open volume multiple times"));
		}

		let path = self.get_volume_path(volume_id);

		let vol = if path.exists() {
			HaystackPhysicalVolume::open(&path)?
		} else {
			HaystackPhysicalVolume::create(&path, &self.index.cluster_id, volume_id)?
		};

		// Verify that if we opened an existing file, that it wasn't from some other conflicting store
		if vol.volume_id != volume_id || &vol.cluster_id != &self.index.cluster_id {
			return Err(io::Error::new(io::ErrorKind::Other, "Opened volume does not belong to this store"));
		}

		if expect_empty && vol.len_needles() > 0 {
			return Err(io::Error::new(io::ErrorKind::Other, "Opened volume that we expected to be empty"));
		}

		self.volumes.insert(volume_id, vol);

		Ok(())
	}

	pub fn create_volume(&mut self, volume_id: u64) -> io::Result<()> {

		self.open_volume(volume_id, true)?;
		
		// We run this after the open_volume succeeds to gurantee that we don't try adding duplicate ids to the index
		// Currently we don't particularly care about inconsistencies with empty files with no corresponding index id
		self.index.add_volume_id(volume_id)?;

		Ok(())
	}



}


