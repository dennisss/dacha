use common::errors::*;
use common::io::Writeable;
use file::LocalFile;

use crate::partition::gpt;
use crate::partition::mbr;
use crate::LOGICAL_BLOCK_SIZE;

// TODO: Must also ensure partitions are 1 MiB aligned just in case we have bad

/// Formats a disk with a brand new (MBR+) GPT partition table with the given
/// entries.
pub async fn format_gpt_disk<F: FnMut(&mut gpt::GPT) -> Result<()>>(
    disk: &mut LocalFile,
    num_sectors: u64,
    mut partitions_builder: F,
) -> Result<()> {
    if num_sectors < 2 {
        return Err(err_msg("Too few sectors"));
    }

    let mbr = mbr::MBR {
        boot_signature: [0x55, 0xAA],
        bootstrap_code_area: [0u8; 446],
        partition_entries: [
            mbr::PartitionEntry {
                status: 0,
                first_absolute_sector: mbr::CHSAddress {
                    head: 0,
                    sector_and_cylinder_high: 2,
                    cylinder: 0,
                },
                partition_type: mbr::PartitionType::GPTProtectiveMBR,
                last_absolute_sector: mbr::CHSAddress {
                    head: 0,
                    sector_and_cylinder_high: 255,
                    cylinder: 255,
                },
                first_absolute_sector_lba: 1,
                num_sectors: num_sectors.min(u32::MAX as u64) as u32,
            },
            mbr::PartitionEntry::default(),
            mbr::PartitionEntry::default(),
            mbr::PartitionEntry::default(),
        ],
    };

    let mut table = gpt::GPT::new(1, num_sectors - 1)?;
    partitions_builder(&mut table)?;

    let mut mbr_buf = vec![];
    mbr.serialize(&mut mbr_buf)?;
    mbr_buf.resize(LOGICAL_BLOCK_SIZE, 0);

    disk.seek(0);
    disk.write_all(&mut mbr_buf).await?;
    table.write(disk).await?;

    disk.sync_all().await?;

    Ok(())
}
