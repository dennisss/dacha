use super::super::common::*;
use super::super::errors::*;
use std::io::{Read, Write, Cursor};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use std::mem::size_of;

const SUPERBLOCK_MAGIC_SIZE: usize = 4;

pub const SUPERBLOCK_SIZE: usize =
	SUPERBLOCK_MAGIC_SIZE +
	size_of::<FormatVersion>() +
	size_of::<ClusterId>() +
	size_of::<MachineId>() +
	size_of::<VolumeId>();

pub struct PhysicalVolumeSuperblock {
	pub magic: Vec<u8>,
	pub cluster_id: ClusterId,
	pub machine_id: MachineId,
	pub volume_id: VolumeId,
}

impl PhysicalVolumeSuperblock {

	pub fn read(reader: &mut Read) -> Result<PhysicalVolumeSuperblock> {
		
		let mut magic = Vec::new(); magic.resize(SUPERBLOCK_MAGIC_SIZE, 0);
		reader.read_exact(&mut magic);

		let ver = reader.read_u32::<LittleEndian>()?;
		let cluster_id = reader.read_u64::<LittleEndian>()?;
		let machine_id = reader.read_u32::<LittleEndian>()?;
		let volume_id = reader.read_u32::<LittleEndian>()?;

		if ver != CURRENT_FORMAT_VERSION {
			return Err("Superblock unknown format version".into());
		}

		Ok(PhysicalVolumeSuperblock {
			magic,
			cluster_id,
			machine_id,
			volume_id
		})
	}

	pub fn write(&self, writer: &mut Write) -> Result<()> {
		writer.write_all(&self.magic)?;
		writer.write_u32::<LittleEndian>(CURRENT_FORMAT_VERSION)?;
		writer.write_u64::<LittleEndian>(self.cluster_id)?;
		writer.write_u32::<LittleEndian>(self.machine_id)?;
		writer.write_u32::<LittleEndian>(self.volume_id)?;
		Ok(())
	}

}
