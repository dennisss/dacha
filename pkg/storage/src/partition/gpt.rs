use alloc::string::String;
use alloc::vec::Vec;
use crypto::random::Rng;
use file::LocalFile;
use std::io::SeekFrom;

use common::check_zero_padding;
use common::io::{Readable, Writeable};
use common::{errors::*, InRange};
use crypto::checksum::crc::CRC32Hasher;
use crypto::hasher::Hasher;
use uuid::UUID;

use crate::{LogicalBlockRange, LOGICAL_BLOCK_SIZE};

mod proto {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/partition/gpt.rs"));
}

pub const UNUSED_PARTITION_ENTRY_GUID: UUID = uuid!("00000000-0000-0000-0000-000000000000");
pub const EFI_SYSTEM_PARTITION_GUID: UUID = uuid!("C12A7328-F81F-11D2-BA4B-00A0C93EC93B");
pub const BASIC_DATA_PARTITION_GUID: UUID = uuid!("EBD0A0A2-B9E5-4433-87C0-68B6B72699C7");
pub const LINUX_FILESYSTEM_DATA_GUID: UUID = uuid!("0FC63DAF-8483-4772-8E79-3D69D8477DE4");

pub const SIGNATURE: [u8; 8] = *b"EFI PART";
pub const REVISION: [u8; 4] = [0, 0, 1, 0];

/// Minimum number of LBAs which we expect to have for storing partition entries
/// on disk. This is also the default value.
pub const MIN_ENTRY_SECTORS: u64 = 32;

/// Maximum number of entry sectors we will parse. We will fail when reading a
/// GPT that exceeds this limit to avoid running out of memory.
pub const MAX_ENTRY_SECTORS: u64 = 32;

#[derive(Debug)]
pub struct GPT {
    header: proto::Header,
    entries: Vec<PartitionEntry>,
}

impl GPT {
    /// Creates a new empty partition table with a pseudo-random disk GUID.
    pub fn new(first_sector: u64, num_sectors: u64) -> Result<Self> {
        if num_sectors < 2 * (1 + MIN_ENTRY_SECTORS) as u64 {
            return Err(err_msg("Too few sectors for GPT"));
        }

        let num_entry_sectors = MIN_ENTRY_SECTORS;

        let mut header = proto::Header {
            signature: SIGNATURE,
            revision: REVISION,
            header_size: (proto::Header::size_of() as u32),
            header_checksum: 0, // Computed when writing.
            reserved: [0u8; 4],
            current_lba: first_sector,
            backup_lba: (first_sector + num_sectors - 1),
            first_usable_lba: first_sector + num_entry_sectors + 1,
            last_usable_lba: (first_sector + num_sectors - 1 - num_entry_sectors - 1),
            disk_guid: [0u8; 16],
            partition_entries_lba: first_sector + 1,
            num_partition_entries: num_entry_sectors as u32,
            partition_entry_size: proto::PartitionEntry::size_of() as u32,
            partition_entries_checksum: 0, // Computed when writing
        };

        crypto::random::clocked_rng().generate_bytes(&mut header.disk_guid);

        Ok(Self {
            header,
            entries: vec![],
        })
    }

    pub fn first_usable_lba(&self) -> u64 {
        self.header.first_usable_lba
    }

    pub fn last_usable_lba(&self) -> u64 {
        self.header.last_usable_lba
    }

    pub fn add_partition(&mut self, entry: PartitionEntry) {
        // TODO: Check that we don't have too many.
        self.entries.push(entry);
    }

    /// Reads a GUID Partition Table from a disk. This assumes that the MBR
    /// sector is already checked.
    ///
    /// Arguments:
    /// - first_sector: Index of the sector containing the GPT header.
    /// - num_sectors: Number of sectors spanned by the GPT.
    pub async fn read(file: &mut LocalFile, first_sector: u64, num_sectors: u64) -> Result<Self> {
        // 1 sector for header
        // at least 32 sectors for partition table.

        if num_sectors < 2 * (1 + MIN_ENTRY_SECTORS) as u64 {
            return Err(err_msg("Too few sectors for GPT"));
        }

        // TODO: If the main header is unreadable, check the backup one
        // ^ Always read the backup so we know if we should re-write it.
        // - If the backup one is not valid or not the same as the regular one, we must
        //   repair it before we write a new one.

        let mut header_buf = [0u8; LOGICAL_BLOCK_SIZE];

        file.seek(first_sector * (LOGICAL_BLOCK_SIZE as u64));
        file.read_exact(&mut header_buf).await?;

        let (header, header_padding) = proto::Header::parse(&header_buf)?;
        if header.signature != SIGNATURE
            || header.revision != REVISION
            || header.header_size != (proto::Header::size_of() as u32)
        {
            return Err(err_msg("Unrecognized GPT header type"));
        }

        let expected_sum = {
            let mut header_zero = header.clone();
            header_zero.header_checksum = 0;

            let mut buf = vec![];
            header_zero.serialize(&mut buf)?;

            let mut hasher = CRC32Hasher::new();
            hasher.update(&buf);

            hasher.finish_u32()
        };

        if expected_sum != header.header_checksum {
            return Err(err_msg("GPT header has wrong checksum"));
        }

        check_zero_padding(&header.reserved)?;

        if header.current_lba != first_sector {
            return Err(err_msg("GPT header at wrong position?"));
        }

        check_zero_padding(header_padding)?;

        if header.backup_lba != (first_sector + num_sectors - 1) {
            return Err(err_msg(
                "Expected backup GPT header to be at the end of the disk region",
            ));
        }

        if (header.partition_entry_size as usize) != proto::PartitionEntry::size_of() {
            return Err(err_msg("Unsupported size of GPT partition entries"));
        }

        let num_entry_bytes =
            (header.partition_entry_size as usize) * (header.num_partition_entries as usize);
        if num_entry_bytes % LOGICAL_BLOCK_SIZE != 0 {
            return Err(err_msg(
                "Partition table does not span a full set of sectors",
            ));
        }

        let num_entry_sectors = (num_entry_bytes / LOGICAL_BLOCK_SIZE) as u64;
        if !num_entry_sectors.in_range(MIN_ENTRY_SECTORS, MAX_ENTRY_SECTORS) {
            return Err(err_msg("Too many/few partition entry sectors"));
        }

        if header.partition_entries_lba != first_sector + 1 {
            return Err(err_msg(
                "Expected partition entries to be immediately after the header",
            ));
        }

        if header.first_usable_lba != header.current_lba + num_entry_sectors + 1 {
            return Err(err_msg(
                "Expected first usable LBA to be immediately after the header",
            ));
        }

        if header.last_usable_lba != header.current_lba + num_sectors - 1 - num_entry_sectors - 1 {
            return Err(err_msg(
                "Expected backup table to be right before backup header",
            ));
        }

        // TODO: Validate we span the entire partition.

        let entries = {
            // TODO: Verify it is a reasonable size and at least 32 sectors.
            let mut table_buf = vec![];
            table_buf.resize(
                (header.partition_entry_size * header.num_partition_entries) as usize,
                0,
            );

            file.seek(header.partition_entries_lba * (LOGICAL_BLOCK_SIZE as u64));
            file.read_exact(&mut table_buf).await?;

            let expected_sum = {
                let mut hasher = CRC32Hasher::new();
                hasher.update(&table_buf);
                hasher.finish_u32()
            };

            if expected_sum != header.partition_entries_checksum {
                return Err(err_msg("Invalid partition entries table"));
            }

            let mut entries = vec![];

            let mut rest = &table_buf[..];
            while !rest.is_empty() {
                let (entry, r) = proto::PartitionEntry::parse(rest)?;
                rest = r;

                if &entry.type_guid == UNUSED_PARTITION_ENTRY_GUID.as_ref() {
                    continue;
                }

                if entry.first_lba > entry.last_lba {
                    return Err(err_msg("Last LBA before first LBA"));
                }

                if entry.first_lba < header.first_usable_lba
                    || entry.last_lba > header.last_usable_lba
                {
                    return Err(err_msg("Partition out of bounds"));
                }

                // TODO: Validate all partitions are non-overlapping

                entries.push(PartitionEntry { entry });
            }

            entries
        };

        Ok(Self { header, entries })
    }

    /// NOTE: If this is an existing GPT header read from a disk, we assume that
    /// it is being written back to the same disk.
    pub async fn write(&self, disk: &mut LocalFile) -> Result<()> {
        // Build new partition entries table.
        let mut table_buf = vec![];
        {
            let entry_size = self.header.partition_entry_size as usize;
            for entry in &self.entries {
                entry.entry.serialize(&mut table_buf)?;
            }

            // Pad with zeros.
            table_buf.resize(
                (self.header.partition_entry_size * self.header.num_partition_entries) as usize,
                0,
            );
        }

        let mut header = self.header.clone();

        header.partition_entries_checksum = {
            let mut hasher = CRC32Hasher::new();
            hasher.update(&table_buf);
            hasher.finish_u32()
        };

        // Build the main header.
        let header_buf = Self::serialize_header(&mut header)?;

        // Build the backup header.
        let mut backup_header = header.clone();
        core::mem::swap(
            &mut backup_header.current_lba,
            &mut backup_header.backup_lba,
        );
        backup_header.partition_entries_lba = backup_header.last_usable_lba + 1;

        let backup_header_buf = Self::serialize_header(&mut backup_header)?;

        // Write everything

        Self::write_at_lba(disk, header.current_lba, &header_buf).await?;
        Self::write_at_lba(disk, header.partition_entries_lba, &table_buf).await?;
        disk.sync_all().await?;

        Self::write_at_lba(disk, backup_header.current_lba, &backup_header_buf).await?;
        Self::write_at_lba(disk, backup_header.partition_entries_lba, &table_buf).await?;
        disk.sync_all().await?;

        Ok(())
    }

    async fn write_at_lba(disk: &mut LocalFile, lba: u64, data: &[u8]) -> Result<()> {
        disk.seek(lba * (LOGICAL_BLOCK_SIZE as u64));
        disk.write_all(data).await
    }

    fn serialize_header(header: &mut proto::Header) -> Result<Vec<u8>> {
        let mut header_buf = vec![];

        header.header_checksum = 0;
        header.serialize(&mut header_buf)?;

        let mut hasher = CRC32Hasher::new();
        hasher.update(&header_buf);
        header.header_checksum = hasher.finish_u32();

        header_buf.clear();
        header.serialize(&mut header_buf)?;

        header_buf.resize(LOGICAL_BLOCK_SIZE, 0);

        Ok(header_buf)
    }

    pub fn disk_guid(&self) -> UUID {
        UUID::from_gpt_bytes(self.header.disk_guid)
    }

    pub fn entries(&self) -> &[PartitionEntry] {
        &self.entries
    }
}

#[derive(Debug)]
pub struct PartitionEntry {
    entry: proto::PartitionEntry,
}

impl PartitionEntry {
    pub fn new(first_lba: u64, last_lba: u64) -> Self {
        let mut partition_guid = [0u8; 16];
        crypto::random::clocked_rng().generate_bytes(&mut partition_guid);

        Self {
            entry: proto::PartitionEntry {
                type_guid: LINUX_FILESYSTEM_DATA_GUID.to_gpt_bytes(),
                partition_guid,
                first_lba,
                last_lba,
                attribute_flags: 0,
                partition_name: [0u8; 72],
            },
        }
    }

    pub fn type_guid(&self) -> UUID {
        UUID::from_gpt_bytes(self.entry.type_guid)
    }

    pub fn partition_guid(&self) -> UUID {
        UUID::from_gpt_bytes(self.entry.partition_guid)
    }

    pub fn range(&self) -> LogicalBlockRange {
        LogicalBlockRange {
            start_block: self.entry.first_lba,
            num_blocks: self.entry.last_lba - self.entry.first_lba,
        }
    }

    pub fn name(&self) -> Result<String> {
        let mut shorts = vec![];
        for c in self.entry.partition_name.chunks_exact(2) {
            let i = u16::from_le_bytes(*array_ref![c, 0, 2]);
            if i == 0 {
                break;
            }

            shorts.push(i);
        }

        Ok(String::from_utf16(&shorts)?)
    }

    pub fn set_name(&mut self, name: &str) -> Result<()> {
        let mut data = vec![];
        for c in name.encode_utf16() {
            data.extend_from_slice(&c.to_le_bytes());
        }

        if data.len() > self.entry.partition_name.len() {
            return Err(err_msg("Partition name is too long."));
        }

        self.entry.partition_name[0..data.len()].copy_from_slice(&data);
        Ok(())
    }
}

// TODO: Dedup this.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpt_size() {
        assert_eq!(proto::Header::size_of(), 92);
        assert_eq!(proto::PartitionEntry::size_of(), 128);
    }
}
