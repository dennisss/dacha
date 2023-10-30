extern crate common;
extern crate storage;
#[macro_use]
extern crate macros;

use std::{fs::File, io::Read};

use common::errors::*;
use storage::partition::gpt::GPT;
use storage::partition::mbr;
use storage::LOGICAL_BLOCK_SIZE;

/*
Binary struct is parsed by going from one field to the next.
- Each field can be decoded
*/

pub trait Hello {
    fn parse(data: &[u8]) -> Result<(Self, &[u8])>
    where
        Self: Sized;
}

impl<const L: usize> Hello for [u8; L] {
    fn parse(data: &[u8]) -> Result<(Self, &[u8])> {
        let mut buf = [0u8; L];
        buf.copy_from_slice(data);
        Ok((buf, data))
    }
}

/*
PartitionEntry {
    status: 0,
    first_absolute_sector: CHSAddress {
        head: 0,
        sector_and_cylinder_high: 2,
        cylinder: 0,
    },
    partition_type: GPTProtectiveMBR,
    last_absolute_sector: CHSAddress {
        head: 255,
        sector_and_cylinder_high: 255,
        cylinder: 255,
    },
    first_absolute_sector_lba: 1,
    num_sectors: 2097151,
},
*/

#[executor_main]
async fn main() -> Result<()> {
    /*
    let devices = storage::devices::BlockDevice::list().await?;
    println!("{:#?}", devices);

    return Ok(());
     */

    // let mut disk = File::open("disk.img")?;
    let mut disk = File::open("/home/dennis/workspace/pi-gen/deploy/2023-02-05-Daspbian-lite.img")?;

    let mut first_sector = [0u8; LOGICAL_BLOCK_SIZE];
    disk.read_exact(&mut first_sector)?;

    let mbr = mbr::parse_mbr(&first_sector)?;
    println!("{:#?}", mbr);

    let partition = &mbr.partition_entries[0];

    // TODO: Verify no other partitions defined and that GPT takes up all the space
    // (based on first_sector and num_sectors).
    if partition.partition_type != mbr::PartitionType::GPTProtectiveMBR {
        return Err(err_msg("Expected GPT partition"));
    }

    if partition.first_absolute_sector_lba != 1 {
        return Err(err_msg("Unexpected GPT start MBR"));
    }

    let gpt = GPT::read(
        &mut disk,
        partition.first_absolute_sector_lba as u64,
        // TODO: This will be flipped to u32 max value if we have too big of a disk.
        partition.num_sectors as u64,
    )?;

    println!("{:#?}", gpt);

    println!("Disk GUID: {:?}", gpt.disk_guid());

    for entry in gpt.entries() {
        println!("Type: {:?}", entry.type_guid());
        println!("GUID: {:?}", entry.partition_guid());
        println!("")
    }

    // Should consume entire disk.

    println!("Hello");

    Ok(())
}
