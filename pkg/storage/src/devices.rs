use alloc::string::{String, ToString};
use alloc::vec::Vec;

use common::errors::*;
use file::LocalPath;

use crate::LOGICAL_BLOCK_SIZE;

#[derive(Clone, Debug)]
pub struct BlockDevice {
    /// Name of the device (e.g. 'sda') (can be accessed at '/dev/[name]')
    pub name: String,

    pub model: Option<String>,

    /// Size in bytes.
    pub size: usize,

    pub removable: bool,

    pub logical_block_size: usize,

    pub physical_block_size: usize,

    pub partitions: Vec<BlockDevicePartition>,
}

#[derive(Clone, Debug)]
pub struct BlockDevicePartition {
    /// e.g. 'sda1' for the first partition of block device 'sda'
    pub name: String,

    pub number: usize,

    /// Offset in bytes at which this partition starts relative to the start of
    /// the block device.
    pub start: usize,

    /// Size in bytes.
    pub size: usize,
}

impl BlockDevice {
    pub async fn list() -> Result<Vec<BlockDevice>> {
        let mut out = vec![];

        let path = LocalPath::new("/sys/block");
        let devices = file::read_dir(path)?;
        for entry in devices {
            let name = entry.name().to_string();
            let device_dir = path.join(entry.name());

            let size = Self::read_property(device_dir.join("size")).await? * LOGICAL_BLOCK_SIZE;
            let removable = Self::read_bool_property(device_dir.join("removable")).await?;
            let logical_block_size =
                Self::read_property(device_dir.join("queue/logical_block_size")).await?;
            let physical_block_size =
                Self::read_property(device_dir.join("queue/physical_block_size")).await?;

            let model = {
                let p = device_dir.join("device/model");
                if file::exists(&p).await? {
                    Some(file::read_to_string(&p).await?.trim().to_string())
                } else {
                    None
                }
            };

            let mut partitions = vec![];
            for entry in file::read_dir(&device_dir)? {
                if entry.typ() != file::FileType::Directory {
                    continue;
                }

                let partition_dir = device_dir.join(entry.name());

                // Filter out non-partition directories.
                let partition_prop = partition_dir.join("partition");
                if !file::exists(&partition_prop).await? {
                    continue;
                }

                let number = Self::read_property(&partition_prop).await?;

                let start =
                    Self::read_property(partition_dir.join("start")).await? * LOGICAL_BLOCK_SIZE;
                let size =
                    Self::read_property(partition_dir.join("size")).await? * LOGICAL_BLOCK_SIZE;

                partitions.push(BlockDevicePartition {
                    name: entry.name().to_string(),
                    number,
                    start,
                    size,
                });
            }

            partitions.sort_by_key(|p| p.number);

            out.push(BlockDevice {
                name,
                model,
                size,
                removable,
                logical_block_size,
                physical_block_size,
                partitions,
            });
        }

        out.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(out)
    }

    async fn read_property<P: AsRef<LocalPath>>(path: P) -> Result<usize> {
        Ok(file::read_to_string(path).await?.trim().parse::<usize>()?)
    }

    async fn read_bool_property<P: AsRef<LocalPath>>(path: P) -> Result<bool> {
        let path = path.as_ref();
        let v = Self::read_property(path).await?;

        Ok(match v {
            0 => false,
            1 => true,
            _ => {
                return Err(format_err!(
                    "Unknown value of bool property: {} in {:?}",
                    v,
                    path
                ))
            }
        })
    }
}
