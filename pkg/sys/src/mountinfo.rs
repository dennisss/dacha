/*
$ cat /proc/656/mountinfo
480 454 0:22 /opt/dacha/data/run/7a88567e306a40fec61e3f4c7a76e793/root / ro,noatime shared:224 master:1 - btrfs /dev/mmcblk0p2 rw,ssd,discard=async,space_cache,subvolid=5,subvol=/
481 480 0:47 / /proc rw,nosuid,nodev,noexec,relatime shared:225 - proc proc rw
482 480 0:5 /null /dev/null rw,nosuid,relatime shared:226 master:2 - devtmpfs udev rw,size=3728688k,nr_inodes=932172,mode=755
483 480 0:5 /zero /dev/zero rw,nosuid,relatime shared:227 master:2 - devtmpfs udev rw,size=3728688k,nr_inodes=932172,mode=755
484 480 0:5 /random /dev/random rw,nosuid,relatime shared:228 master:2 - devtmpfs udev rw,size=3728688k,nr_inodes=932172,mode=755
485 480 0:5 /urandom /dev/urandom rw,nosuid,relatime shared:229 master:2 - devtmpfs udev rw,size=3728688k,nr_inodes=932172,mode=755
486 480 0:48 / /dev/pts rw,nosuid,noexec,relatime shared:230 - devpts devpts rw,gid=400001,mode=600,ptmxmode=666
487 480 0:22 /usr/bin /usr/bin rw,noatime shared:231 master:1 - btrfs /dev/mmcblk0p2 rw,ssd,discard=async,space_cache,subvolid=5,subvol=/
488 480 0:22 /usr/lib /usr/lib rw,noatime shared:232 master:1 - btrfs /dev/mmcblk0p2 rw,ssd,discard=async,space_cache,subvolid=5,subvol=/
489 480 0:22 /opt/dacha/bundle/built/pkg/container/container_init /init rw,noatime shared:233 master:1 - btrfs /dev/mmcblk0p2 rw,ssd,discard=async,space_cache,subvolid=5,subvol=/
490 480 0:22 /opt/dacha/data/blob/sha256:c3f5601b6cea6464f3cb054d014f26f87d7569a91ae3d152e0fe0cefbb506414/extracted /volumes/bundle rw,noatime shared:234 master:1 - btrfs /dev/mmcblk0p2 rw,ssd,discard=async,space_cache,subvolid=5,subvol=/
491 480 0:22 /opt/dacha/data/volume/per-worker/system.meta.jgwwk8sssrehh/metastore_data /volumes/data rw,noatime shared:235 master:1 - btrfs /dev/mmcblk0p2 rw,ssd,discard=async,space_cache,subvolid=5,subvol=/


*/

use base_error::*;

use crate::pid_t;

#[derive(Clone, Debug)]
pub struct MountInfo {
    pub mounts: Vec<MountInfoEntry>,
}

#[derive(Clone, Debug)]
pub struct MountInfoEntry {
    pub root: String,
    pub mount_point: String,
}

impl MountInfo {
    pub fn read(pid: Option<pid_t>) -> Result<Self> {
        // TODO: Need to support handling file paths with spaces in them.

        let pid_str = pid
            .map(|s| s.to_string())
            .unwrap_or_else(|| "self".to_string());

        let data = crate::blocking_read_to_string(&format!("/proc/{}/mountinfo", pid_str))?;

        let mut out = vec![];

        for line in data.split('\n') {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts = line.split(' ').collect::<Vec<_>>();
            if parts.len() < 6 {
                return Err(err_msg("Mount info contains invalid data"));
            }

            let root = parts[3];
            let mount_point = parts[4];
            out.push(MountInfoEntry {
                root: root.into(),
                mount_point: mount_point.into(),
            });
        }

        Ok(Self { mounts: out })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mountinfo_works() {
        let info = MountInfo::read(None).unwrap();

        println!("{:#?}", info);
    }
}
