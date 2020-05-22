use std::io;
use std::io::{Write, Read, Seek};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, SeekFrom};
use std::mem::size_of;
use std::path::{Path};
use common::errors::*;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use crate::common::*;



const VOLUMES_MAGIC: &str = "HAYV";
const VOLUMES_MAGIC_SIZE: usize = 4;

const VOLUMES_HEADER_SIZE: usize =
	VOLUMES_MAGIC_SIZE +
	size_of::<FormatVersion>() +
	size_of::<ClusterId>() +
	size_of::<MachineId>();

/// File that stores the list of all volumes on the current store machine
pub struct StoreMachineIndex {
	pub cluster_id: ClusterId,
	pub machine_id: MachineId,
	file: File

	// TODO: We also want to store the amount of space allocated to each physical volume and also run a crc over all of the data in this file
	// considerng tha this is the only real meaningful file, we should ensure that it always checks out
}

impl StoreMachineIndex {

	pub fn open(path: &Path) -> Result<StoreMachineIndex> {
		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut f = opts.open(path)?;

		let mut header = [0u8; VOLUMES_HEADER_SIZE];
		f.read_exact(&mut header)?;

		if &header[0..VOLUMES_MAGIC_SIZE] != VOLUMES_MAGIC.as_bytes() {
			return Err(err_msg("Volumes magic is incorrect"));
		}

		let mut c = Cursor::new(&header[VOLUMES_MAGIC_SIZE..]);

		let version = c.read_u32::<LittleEndian>()?;
		let cluster_id = c.read_u64::<LittleEndian>()?;
		let machine_id = c.read_u32::<LittleEndian>()?;

		if version != CURRENT_FORMAT_VERSION {
			return Err(err_msg("Volumes version is incorrect"));
		}


		let idx = StoreMachineIndex {
			cluster_id,
			machine_id,
			file: f
		};

		Ok(idx)
	}

	// Need to do a whole lot right here
	pub fn create(path: &Path, cluster_id: ClusterId, machine_id: MachineId) -> Result<StoreMachineIndex> {
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let mut f = opts.open(path)?;

		// Sync directory
		File::open(path.parent().unwrap()).unwrap().sync_all()?;


		f.write_all(VOLUMES_MAGIC.as_bytes())?;
		f.write_u32::<LittleEndian>(CURRENT_FORMAT_VERSION)?;
		f.write_u64::<LittleEndian>(cluster_id)?;
		f.write_u32::<LittleEndian>(machine_id)?;

		f.sync_data()?;

		Ok(StoreMachineIndex {
			cluster_id: cluster_id.clone(),
			machine_id: machine_id.clone(),
			file: f
		})
	}

	/// Get all volume ids referenced in this index
	/// It's someone else's problem to ensure that there are no duplicates
	pub fn read_all(&mut self) -> Result<Vec<VolumeId>> {
		
		self.file.seek(SeekFrom::Start(VOLUMES_HEADER_SIZE as u64))?;

		let mut buf = Vec::new();
		self.file.read_to_end(&mut buf)?;

		// Should round exactly to id
		if buf.len() % size_of::<VolumeId>() != 0 {
			return Err(err_msg("Volumes index is corrupt"));
		}

		let mut out = Vec::new();


		let size = buf.len() / size_of::<VolumeId>();
		let mut cur = Cursor::new(buf);
		for _ in 0..size {
			out.push(cur.read_u32::<LittleEndian>()?);
		}

		Ok(out)
	}

	pub fn add_volume_id(&mut self, id: VolumeId) -> io::Result<()> {
		self.file.seek(SeekFrom::End(0))?;
		self.file.write_u32::<LittleEndian>(id)?;
		self.file.sync_data()?;
		Ok(())
	}

}





