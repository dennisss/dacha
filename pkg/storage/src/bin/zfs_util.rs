// This is a CLI utility for helping setup and manage ZFS disks and pools.

#[macro_use]
extern crate macros;

use std::collections::HashMap;
use std::collections::HashSet;
use std::process::Stdio;
use std::time::Duration;

use base_units::ByteCount;
use common::args::list::CommaSeparated;
use common::ceil_div;
use common::errors::*;
use file::LocalFile;
use file::LocalFileOpenOptions;
use file::LocalPath;
use file::LocalPathBuf;
use storage::devices::BlockDevice;
use storage::partition::format_gpt_disk;
use storage::partition::gpt::PartitionEntry;
use storage::scsi::ATAIdentityDevice;
use storage::scsi::SCSIDevice;
use storage::LOGICAL_BLOCK_SIZE;

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    /// Performs one off partitioning of a Mach.2 dual actuator disk into 2
    /// equal size partitions aligned with the actuators.
    #[arg(name = "partition-mach2-disk")]
    PartitionMach2Disk(PartitionMach2DiskCommand),

    #[arg(name = "create-mach2-pool")]
    CreatePool(CreatePoolCommand),
}

#[derive(Args)]
struct PartitionMach2DiskCommand {
    disks: LocalPathBuf,
}

#[derive(Args)]
struct CreatePoolCommand {
    pool_name: String,

    /// Glob pattern pointing to all the /dev/ disk files. Regardless of what is
    /// used here, devices will be referenced by the most appropriate
    /// '/dev/disk/by-x/y' available.
    disks: LocalPathBuf,

    /// List of serial numbers of disks identifying the physical ordering of the
    /// disks. We will prefer to not put adjacent disks in the same vdev.
    serial_order: Option<CommaSeparated<String>>,

    /// The number of physical disks to assign to each vdev. We will make enough
    /// vdevs to use all disks (so the total number of disks must be divisible
    /// by this). Note that we treat dual actuator drives as 'two disks' but the
    /// same physical disk won't be assigned twice to the same vdev.
    num_disks_per_vdev: usize,

    /// Something like 'raidz2'
    topology: String,
}

async fn create_pool_command(cmd: CreatePoolCommand) -> Result<()> {
    if cmd.num_disks_per_vdev < 1 {
        return Err(err_msg("Must have at least 1 disk per vdev"));
    }

    if cmd.topology.is_empty() {
        return Err(err_msg("Must specify a non-empty value for --topology"));
    }

    let mut disks = list_disks(&cmd.disks).await?;
    println!("");

    if disks.is_empty() {
        return Err(err_msg("No disks found"));
    }

    // TODO: Need to verify each partition in a vdev is roughly the same size.

    if let Some(serial_order) = cmd.serial_order {
        if serial_order.values.len() != disks.len() {
            return Err(err_msg(
                "Number of serial numbers doesn't match number of matched disks",
            ));
        }

        let mut disks_per_serial = HashMap::new();
        for disk in disks.drain(..) {
            if disks_per_serial.contains_key(&disk.identity.serial_number()) {
                return Err(err_msg("Disks have duplicate serial numbers"));
            }

            disks_per_serial.insert(disk.identity.serial_number(), disk);
        }

        for (idx, serial) in serial_order.values.into_iter().enumerate() {
            let mut disk = disks_per_serial.remove(&serial).ok_or_else(|| {
                format_err!("Missing or duplicate disk with serial number: {}", serial)
            })?;
            disk.position = idx;
            disks.push(disk);
        }
    }

    // Assuming the disks are ordered, re-sort them to avoid picking adjacent disks.
    {
        let mut disks_opt = disks.drain(..).map(|d| Some(d)).collect::<Vec<_>>();
        let num_disks = disks_opt.len();

        let mut i = 0;
        while disks.len() != num_disks {
            let d = match disks_opt[i % num_disks].take() {
                Some(d) => d,
                None => {
                    i += 1;
                    continue;
                }
            };

            disks.push(d);
            i += cmd.num_disks_per_vdev;
        }
    }

    let num_partitions_per_disk = disks[0].proposed_partitions.len();

    let mut flat_partitions = vec![];

    for part_i in 0..num_partitions_per_disk {
        for disk_i in 0..disks.len() {
            flat_partitions.push((
                disks[disk_i].position,
                disks[disk_i].proposed_partitions[part_i].clone(),
            ));
        }
    }

    if flat_partitions.len() % cmd.num_disks_per_vdev != 0 {
        return Err(err_msg(
            "Number of disks/partitions is not divisible by 'num_disks_per_vdev'",
        ));
    }

    let mut create_cmd = String::new();

    let mut zfs_args = vec![
        "create".to_string(),
        cmd.pool_name.clone(),
        "-o".to_string(),
        "ashift=12".to_string(),
    ];

    for chunk in flat_partitions.chunks(cmd.num_disks_per_vdev) {
        let mut disk_set = HashSet::new();

        zfs_args.push(cmd.topology.clone());

        for (disk_index, partlabel) in chunk {
            if !disk_set.insert(*disk_index) {
                return Err(err_msg("Duplicate physical disk in vdev"));
            }

            zfs_args.push(format!("/dev/disk/by-partlabel/{}", partlabel));
        }

        println!("VDev Disk Positions: {:?}", disk_set);
    }

    println!("ZFS Command To Run: {:?}", zfs_args);

    println!("");
    println!("Continue: [y/N]?");
    if !file::read_user_confirmation().await? {
        println!("[Exit without changing anything]");
        return Ok(());
    }

    // TODO: Only need to do this if we are using dual actuator disks.
    println!("Partitioning...");
    for entry in disks.into_iter().enumerate() {
        println!("- #{}", entry.0);
        partition_mach2_disk(entry.1).await?;
    }

    // Just make sure that all the kernel rescaning is definitely done.
    executor::sleep(Duration::from_secs(1)).await?;

    println!("Running zpool create...");

    let mut child = std::process::Command::new("zpool")
        .args(zfs_args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let status = child.wait()?;
    if !status.success() {
        return Err(err_msg("Command failed!"));
    }

    Ok(())
}

struct DiskEntry {
    position: usize,
    block_device: BlockDevice,
    identity: ATAIdentityDevice,
    disk: LocalFile,
    is_dual_actuator: bool,
    proposed_partitions: Vec<String>,
}

async fn list_disks(disks_pattern: &LocalPath) -> Result<Vec<DiskEntry>> {
    let all_devices = storage::devices::BlockDevice::list().await?;

    // TODO: If we end up supporting more complex patterns, dedup the files.
    let selected_disks = {
        let mut out = vec![];

        let mut iter = file::GlobIterator::create(disks_pattern)?;
        while let Some(path) = iter.next().await? {
            if !path.as_str().starts_with("/dev/sd") {
                return Err(err_msg("Expected to only match /dev/sd disks"));
            }

            // Skip partitions.
            if path.as_str().ends_with("1") || path.as_str().ends_with("2") {
                continue;
            }

            let block_dev = all_devices
                .iter()
                .find(|d| format!("/dev/{}", d.name) == path.as_str())
                .ok_or_else(|| format_err!("Failed to find sysfs entry for: {}", path.as_str()))?;

            out.push(block_dev);
        }

        out
    };

    let mut out = vec![];

    for (i, block_dev) in selected_disks.into_iter().enumerate() {
        let mut disk = file::LocalFile::open_with_options(
            format!("/dev/{}", block_dev.name),
            &LocalFileOpenOptions::new().read(true).write(true),
        )?;

        let mut scsi = SCSIDevice::create(disk)?;
        let identity = scsi.ata_identify_device()?;

        println!("");
        println!("==========================");
        println!("Disk: {}", block_dev.name);
        println!("Size: {:?}", ByteCount::from(block_dev.size));
        println!(
            "Block Size: {:?} logical / {:?} physical",
            ByteCount::from(block_dev.logical_block_size),
            ByteCount::from(block_dev.physical_block_size)
        );
        println!("Model: {}", identity.model_number());
        println!("Serial Number: {}", identity.serial_number());

        if block_dev.size % LOGICAL_BLOCK_SIZE != 0
            || block_dev.physical_block_size % LOGICAL_BLOCK_SIZE != 0
        {
            return Err(err_msg("Bad block sizes"));
        }

        let is_dual_actuator = check_if_dual_actuator_disk(&block_dev, &mut scsi)?;

        let disk = scsi.into_inner();

        let proposed_partitions = {
            if is_dual_actuator {
                let name_prefix = format!("Seagate-Disk-{}", identity.serial_number());
                let parts = vec![format!("{}-1", name_prefix), format!("{}-2", name_prefix)];

                println!("Partitions to made:");
                for p in &parts {
                    println!("- {}", p);
                }

                parts
            } else {
                vec![]
            }
        };

        out.push(DiskEntry {
            position: i,
            block_device: block_dev.clone(),
            identity,
            disk,
            is_dual_actuator,
            proposed_partitions,
        });
    }

    Ok(out)
}

fn check_if_dual_actuator_disk(block_dev: &BlockDevice, scsi: &mut SCSIDevice) -> Result<bool> {
    let ranges = match scsi.ata_concurrent_positioning_ranges()? {
        Some(v) => v,
        None => return Ok(false),
    };

    if ranges.len() == 1 {
        // TODO: Check that it covers the whole range.
        return Ok(false);
    }

    if ranges.len() != 2 {
        return Err(format_err!(
            "Unsupported number of actuators: {} (expected 2 for mach.2 formatting)",
            ranges.len()
        ));
    }

    if ranges[1].lowest_lba * (LOGICAL_BLOCK_SIZE as u64) * 2 != block_dev.size as u64 {
        return Err(err_msg(
            "Unsupported setup: Dual actuator disk doesn't split directly in the middle of the disk",
        ));
    }

    Ok(true)
}

async fn partition_mach2_disk(mut entry: DiskEntry) -> Result<()> {
    let mut disk = entry.disk;
    let block_dev = entry.block_device;
    let proposed_partitions = entry.proposed_partitions;

    let alignment_blocks = block_dev.physical_block_size / block_dev.logical_block_size;

    block_dev.unmount_all().await?;

    format_gpt_disk(
        &mut disk,
        block_dev.size as u64 / LOGICAL_BLOCK_SIZE as u64,
        |table| {
            // LBA to use for the first partition. This is the first physically aligned LBA
            // possible.
            let first_lba = (ceil_div(table.first_usable_lba() as usize, alignment_blocks)
                * alignment_blocks) as u64;

            // Middle LBA on the disk. To be used for the second partition.
            let mid_lba = (block_dev.size / LOGICAL_BLOCK_SIZE / 2) as u64;

            // The size of both partitions will be the same and will be the largest possible
            // value if we try to span as much space as possible.
            let partition_size = core::cmp::min(
                // Max possible first partition size.
                mid_lba - first_lba,
                // Max possible second partition size
                table.last_usable_lba() + 1 - mid_lba,
            );

            let mut entry1 = PartitionEntry::new(first_lba, first_lba + partition_size - 1);
            entry1.set_name(&proposed_partitions[0])?;

            let mut entry2 = PartitionEntry::new(mid_lba, mid_lba + partition_size - 1);
            entry2.set_name(&proposed_partitions[1])?;

            println!(
                "=> Partition 1: {} : Start LBA: {}",
                entry1.name()?,
                entry1.range().start_block
            );
            println!(
                "=> Partition 2: {} : Start LBA: {}",
                entry2.name()?,
                entry2.range().start_block
            );

            table.add_partition(entry1);
            table.add_partition(entry2);

            Ok(())
        },
    )
    .await?;

    // Close the disk reference before we rescan the disk.
    drop(disk);

    block_dev.kernel_rescan()?;

    Ok(())
}

async fn partition_mach2_disk_command(cmd: PartitionMach2DiskCommand) -> Result<()> {
    let mut disks = list_disks(&cmd.disks).await?;

    for disk in &mut disks {
        if !disk.is_dual_actuator {
            return Err(err_msg("One or more disks aren't dual actuator"));
        }
    }

    println!("");
    println!("Continue: [y/N]?");
    if !file::read_user_confirmation().await? {
        println!("[Exit without changing anything]");
        return Ok(());
    }

    for entry in disks {
        partition_mach2_disk(entry).await?;
    }

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::CreatePool(cmd) => create_pool_command(cmd).await?,
        Command::PartitionMach2Disk(cmd) => partition_mach2_disk_command(cmd).await?,
    };

    Ok(())
}
