extern crate common;
extern crate storage;
#[macro_use]
extern crate macros;

use std::io::Seek;
use std::{fs::File, io::Read};

use common::errors::*;
use common::io::Readable;
use file::LocalFileOpenOptions;
use storage::partition::gpt::GPT;
use storage::partition::mbr;
use storage::scsi::SCSIDevice;
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

/*
cargo build --package storage

scp target/debug/storage dennis@10.1.0.135:~


*/

#[executor_main]
async fn main() -> Result<()> {
    let devices = storage::devices::BlockDevice::list().await?;
    println!("Block Devs:\n{:#?}", devices);


    let hosts = storage::sas::SASExpander::list().await?;
    println!("SAS Expanders:\n{:#?}", hosts);

    {
        let mut disk = file::LocalFile::open_with_options(
            "/dev/sda",
            &LocalFileOpenOptions::new().read(true).write(true),
        )?;

        let mut scsi = SCSIDevice::create(disk)?;

        let serial = scsi.unit_serial_number()?;
        println!("Serial num: {}", serial);

        println!("{:?}", scsi.scsi_temperature()?);
    }

    {
        let mut disk = file::LocalFile::open_with_options(
            "/dev/sg1",
            &LocalFileOpenOptions::new().read(true).write(true),
        )?;

        let mut scsi = SCSIDevice::create(disk)?;

        println!("Expander Temp: {}", scsi.scsi_enclosure_temperature()?);
    }

    println!("SCSI Generics:\n{:#?}", storage::scsi::SCSIGenericDeviceEntry::list().await?);

    println!("Enclosures:\n{:#?}", storage::enclosure::EnclosureEntry::list().await?);

    /*
    println!("Identity: {:?}", scsi.ata_identify_device()?);

    let dev_stats = scsi.ata_smart_device_statistics()?;
    println!("Temperature: {:?}", dev_stats.current_temperature());

    let attrs = scsi.ata_smart_read_data()?;
    println!("{:?}", attrs);

    println!("{:?}", scsi.ata_concurrent_positioning_ranges()?);
    */

    return Ok(());

    // let mut disk = std::fs::OpenOptions::new().read(true).sy
    // File::open("/dev/sdd")?; let mut disk =
    // File::open("/home/dennis/workspace/pi-gen/deploy/2024-07-06-Daspbian-lite.
    // img")?;

    // disk.seek(std::io::SeekFrom::Start(0))?;

    /*
    disk.seek(0);

    let mut first_sector = [0u8; LOGICAL_BLOCK_SIZE];
    disk.read_exact(&mut first_sector).await?;

    let mbr = mbr::parse_mbr(&first_sector)?;
    println!("{:#?}", mbr);

    /*
    {
        let partition = &mbr.partition_entries[1];

        let offset = partition.first_absolute_sector_lba as usize * LOGICAL_BLOCK_SIZE;
        let size = partition.num_sectors as usize * LOGICAL_BLOCK_SIZE;

        println!("{} :  {:?}", offset, base_units::ByteCount::from(size));

        return Ok(());
    }
    */

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
    )
    .await?;

    println!("{:#?}", gpt);

    println!("Disk GUID: {:?}", gpt.disk_guid());

    for entry in gpt.entries() {
        println!("Type: {:?}", entry.type_guid());
        println!("GUID: {:?}", entry.partition_guid());
        println!("")
    }

    // Should consume entire disk.
    */

    Ok(())
}
