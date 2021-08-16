use crate::types::*;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use common::errors::*;
use crypto::{checksum::crc::CRC32CHasher, hasher::Hasher};
use std::io::{Cursor, Read, Write};
use std::mem::size_of;

const SUPERBLOCK_MAGIC_SIZE: usize = 4;
const CHECKSUM_SIZE: usize = 4;

pub const SUPERBLOCK_SIZE: usize = SUPERBLOCK_MAGIC_SIZE +
	size_of::<FormatVersion>() +
	size_of::<ClusterId>() +
	size_of::<MachineId>() +
	size_of::<VolumeId>() + 
	size_of::<BlockSize>() +
	size_of::<u64>() + // < Allocated space
	CHECKSUM_SIZE;

pub struct PhysicalVolumeSuperblock {
    pub magic: Vec<u8>,
    pub cluster_id: ClusterId,
    pub machine_id: MachineId,
    pub volume_id: VolumeId,

    /// Block size used for alignment and units when the volume was created
    pub block_size: u64,

    /// Total amount of disk space reserved to this volume
    /// The total sum of space occupied on disk of all the volume's files (aside
    /// from active compactions) will try to stay within this limit
    pub allocated_space: u64,
}

impl PhysicalVolumeSuperblock {
    pub fn read(reader: &mut dyn Read) -> Result<PhysicalVolumeSuperblock> {
        let mut buf = Vec::new();
        buf.resize(SUPERBLOCK_SIZE, 0);
        reader.read_exact(&mut buf)?;

        let mut cursor = Cursor::new(&buf);

        let mut magic = Vec::new();
        magic.resize(SUPERBLOCK_MAGIC_SIZE, 0);
        cursor.read_exact(&mut magic)?;

        let ver = cursor.read_u32::<LittleEndian>()?;

        // Because the rest of the fields all depend on using the correct version, we
        // check that first
        if ver != CURRENT_FORMAT_VERSION {
            return Err(err_msg("Superblock unknown format version"));
        }

        let cluster_id = cursor.read_u64::<LittleEndian>()?;
        let machine_id = cursor.read_u32::<LittleEndian>()?;
        let volume_id = cursor.read_u32::<LittleEndian>()?;
        let block_size = cursor.read_u32::<LittleEndian>()?;
        let allocated_space = cursor.read_u64::<LittleEndian>()?;

        let expected_sum = {
            let mut hasher = CRC32CHasher::new();
            hasher.update(&buf[0..(cursor.position() as usize)]);
            hasher.finish_u32()
        };
        let checksum = cursor.read_u32::<LittleEndian>()?;

        assert_eq!(cursor.position(), SUPERBLOCK_SIZE as u64);

        if expected_sum != checksum {
            return Err(err_msg("Incorrect checksum in read superblock"));
        }

        Ok(PhysicalVolumeSuperblock {
            magic,
            cluster_id,
            machine_id,
            volume_id,
            block_size: block_size as u64,
            allocated_space,
        })
    }

    pub fn write(&self, writer: &mut Write) -> Result<()> {
        if (self.allocated_space / self.block_size) + 1 > (BlockOffset::max_value() as u64) {
            return Err(err_msg(
                "Volume allocated size is too large to fit into the block offset type",
            ));
        }

        let mut buf = Vec::new();
        buf.reserve(SUPERBLOCK_SIZE);

        {
            let mut cursor = Cursor::new(&mut buf);
            cursor.write_all(&self.magic)?;
            cursor.write_u32::<LittleEndian>(CURRENT_FORMAT_VERSION)?;
            cursor.write_u64::<LittleEndian>(self.cluster_id)?;
            cursor.write_u32::<LittleEndian>(self.machine_id)?;
            cursor.write_u32::<LittleEndian>(self.volume_id)?;
            cursor.write_u32::<LittleEndian>(self.block_size as u32)?;
            cursor.write_u64::<LittleEndian>(self.allocated_space)?;
        }
        {
            let sum = {
                let mut hasher = CRC32CHasher::new();
                hasher.update(&buf);
                hasher.finish_u32()
            };

            let cur = buf.len();
            let mut cursor = Cursor::new(&mut buf);
            cursor.set_position(cur as u64);

            cursor.write_u32::<LittleEndian>(sum)?;
        }

        assert_eq!(buf.len(), SUPERBLOCK_SIZE);

        writer.write_all(&buf)?;

        Ok(())
    }
}
