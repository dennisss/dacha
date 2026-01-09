use std::collections::HashSet;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use common::errors::*;
use file::{LocalPath, LocalPathBuf};

use crate::LOGICAL_BLOCK_SIZE;

#[derive(Clone, Debug)]
pub struct BlockDevice {
    /// Name of the device (e.g. 'sda') (can be accessed at '/dev/[name]')
    pub name: String,

    /// NOTE: When using USB disk adapters, this may be inaccurate and report
    /// the adapter model rather than the disk model. It is best to directly
    /// probe the ATA identity if a more accurate name is required.
    pub model: Option<String>,

    pub device_path: Option<LocalPathBuf>,

    /// Size in bytes.
    pub size: usize,

    pub removable: bool,

    pub logical_block_size: usize,

    pub physical_block_size: usize,

    /// Rough guess of the physical protocol in use.
    pub protocol: BlockDeviceProtocol,

    pub partitions: Vec<BlockDevicePartition>,

    pub sas_address: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockDeviceProtocol {
    Unknown,

    /// NOTE: Currently this is currently only able to detect ATA devices that
    /// are directly connected (not via a USB adapter).
    ATA,

    NVME,

    SAS,
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

            let device_path = {
                let p = device_dir.join("device");
                if file::exists(&p).await? {
                    let mut p = file::realpath(&p).await?;;
                    Some(p.normalized())
                } else {
                    None
                }
            };

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

            let mut sas_address = None;

            let protocol = {
                if entry.name().starts_with("nvme") {
                    BlockDeviceProtocol::NVME
                } else if entry.name().starts_with("sd") {
                    let sas_address_path = device_dir.join("device/sas_address");
                    if file::exists(&sas_address_path).await? {
                        sas_address = Some(file::read_to_string(&sas_address_path).await?.trim().to_string());
                        
                        BlockDeviceProtocol::SAS
                    } else {
                        // NOTE: For SAS drives, this will actually be the disk manufacturer
                        // (e.g. "WDC" for Western Digital drives.)
                        let vendor_prop = device_dir.join("device/vendor");

                        if file::read_to_string(vendor_prop).await?.trim() == "ATA" {
                            BlockDeviceProtocol::ATA
                        } else {
                            BlockDeviceProtocol::Unknown
                        }
                    }
                } else {
                    BlockDeviceProtocol::Unknown
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
                device_path,
                logical_block_size,
                physical_block_size,
                partitions,
                protocol,
                sas_address,
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

    /// Unmounts all fs mounts found to be referencing this block device.
    pub async fn unmount_all(&self) -> Result<()> {
        let mut device_paths = HashSet::<String>::default();
        device_paths.insert(format!("/dev/{}", self.name));
        for partition in &self.partitions {
            device_paths.insert(format!("/dev/{}", partition.name));
        }

        let mut to_unmount = vec![];

        let mounts = sys::mounts()?;
        for mount in mounts {
            if !device_paths.contains(&mount.device) {
                continue;
            }

            if mount.mount_point == "/"
                || mount.mount_point.starts_with("/boot")
                || mount.mount_point.starts_with("/home")
            {
                return Err(format_err!(
                    "Attempting to unmount device used for system directories like \"{}\"",
                    mount.mount_point
                ));
            }

            to_unmount.push(mount.mount_point);
        }

        for path in to_unmount {
            println!("Umounting {}...", path);
            sys::umount(&path, sys::UmountFlags::empty())?;
        }

        Ok(())
    }

    /// Requests that the Linux kernel rescan this disk for partitions.
    ///
    /// Note that you should unmount all references to the disk before doing
    /// this to avoid inconsistency.
    ///
    /// DOES NOT mutate this entry so it may still contain old partitions.
    pub fn kernel_rescan(&self) -> Result<()> {
        // TODO: Make this work with file::write
        // (probably doesn't work as the file is not seekable).
        unsafe {
            let s =
                std::ffi::CString::new(format!("/sys/block/{}/device/rescan", self.name)).unwrap();

            let fd = sys::OpenFileDescriptor::new(sys::open(
                s.as_ptr(),
                sys::O_WRONLY | sys::O_CLOEXEC,
                0,
            )?);

            let mut buf = b"1";

            let n = sys::write(*fd, buf.as_ptr(), 1)?;

            assert_eq!(n, 1);
        };

        Ok(())
    }
}
