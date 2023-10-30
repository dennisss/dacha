
For testing, create a lookback device (1GiB):
- `dd if=/dev/zero of=disk.img bs=1M count=1024`
- `sudo losetup /dev/loop100 disk.img`
  - Make sure `/dev/loop100` doesn't already exist
- Then we used Ubuntu's 'Disks' GUI application to add a GPT partition table to it with an EXT4 and FAT32 file system.
  - First partition named "P1" with UUID e09c74ce-40a9-4591-9a37-e6964625e59b
  - Second partition named "P2" with UUID C4E0-3684
  - In the fat32 FS we created a 'documents' directory containing the 'lorem_ipsum.txt' file from our testdata directory.
- To make sure things have been flushed to the image:
  - `sync -f /media/dennis/P2/documents/lorem_ipsum.txt`
- - `sync -f /media/dennis/P2/documents`
  - `sudo sync /dev/loop100`
- Then we can compress it:
  - `mksquashfs disk.img disk.squashfs -comp zstd`

The fdisk util may be a good reference for how to get disk info:

- https://github.com/util-linux/util-linux/blob/master/disk-utils/fdisk.c

- `cat /sys/dev/block/8\:0/queue/optimal_io_size`

- There is also ioctl
  - - https://github.com/util-linux/util-linux/blob/master/libblkid/src/topology/ioctl.c

Also hdparm for how to do things like sleep.

We want to use raw I/O (O_DIRECT on block device)


We can list block devices in 

- `cat /sys/block/sda/queue/logical_block_size`
- `cat /sys/block/sda/device/model`
- https://github.com/tinganho/linux-kernel/blob/master/Documentation/ABI/testing/sysfs-block
- `/sys/block/sda/size`
  - Size in logical sectors?
- TODO: Figure out how `/sys/block/<disk>/alignment_offset` matters.
- `/sys/block/sda/device/wwid`
- Example of getting serial number
  - - https://stackoverflow.com/questions/21977311/obtaining-wwn-of-sata-disks
  - HDIO_GET_IDENTITY
- `/dev/disk/by-id/`

We also need TRIM support


Key stats about disks:
- We want to know which system devices are connected to
- Ideally use udev to grant us access to 

