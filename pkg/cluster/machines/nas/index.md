# NAS Build

This doc describes my home NAS build.

## Hardware

- Case: 45Drives HL15
  - With 6 x 120mm Noctua fans.
- CPU: AMD EPYC 7042
- Motherboard: Supermicro H11SSL-i
- RAM: 16G x 8 2133P DDR4 ECC REG
- PSU: Seasonic PRIME TX-750W 80 Plus Titanium
- LAN: Mellanox Connect X3 Pro (CX312B) (SFP 10G card)
- Drives:
  - 15 x Seagate 2X18 18 TB SATA drives (ST18000NM0092) (dual actuator)
    - ATA Version: ACS-4
    - SATA Version: 3.3, 6.0 Gb/s
  - Cables:
    - 2 x SFF-8643 to SFF-8643 (Mini SAS HD to Mini SAS HD)
    - 2 x SFF-8643 to 4 x SaTA (Mini SAS HD to 4 x SATA)
    - These all connect directly from the case backplane to the motherboard
  - 4 x `Samsung PM983 1.92TB` (MZ1LB1T9HALS, M.2 22110)
    - Mounted to a `ASUS Hyper M.2 X16 PCIe 3.0 X4 Expansion Card V2`
    - Note that the chosen card must supprot 22110 length M.2.
    - Note that the newer ASUS PCIe 4.0 card doesn't fit in the HL15.
    - Note that the card is responsibily for 12V -> 3.3V conversion for the M.2 power
      - Max power supported needs to be >10W per drive.
      - The standard PCIe connector onl supports ~10W of power overall for raw 3.3V so conversion is necessary.
      - DON'T use these [AliExpress cards](https://www.aliexpress.us/item/3256804776969407.html) as they seem to have bad PSUs that break after excessive use with high power enterprise drives.
    - Cool the card with 2 x 4010 blower fans
    - TODO: Update this with my upgraded fan solution.
- Boot Drive:
  - 1 x `Samsung SSD 970 Pro (512 GB) M.2`
- Fan Controller: See [here](/pkg/things/fan_controller/boards/board-hl15-latest/)
- CPU Cooler: `ARCTIC Freezer 4U SP3`
  - 400-2300 rpm, 120mm fans, PWM Controlled

## BIOS Setup

Setup CPU Slot 6 in x4x4x4x4 bifurcation in the BIOS. We assume this is the slot used for the NVMe SSDs.

## Software

### OS

Install Ubuntu Server 24.04.1 LTS from a USB drive. Settings should be:

- 'Minimal Install'
- Custom storage layout (use the small SSD as a boot disk)
  - 1 GiB FAT32 EFI partition (mounted as `/boot/efi`)
  - Rest of disk as a BTRFS partition (mounted as `/`)
- For standardization with the rest of the servers, we use the username `cluster-user` for the main sudo user.
- Install OpenSSH
  - Be sure to enable `Allow password authentication over SSH`
    - This is mainly temporary and we'll remove the password using the password.

Then restart and SSH into the server to run the rest of the steps.

Install used packages:

```
sudo apt update

sudo apt install vim smartmontools hdparm ipmitool lm-sensors psmisc zfsutils-linux uidmap nvme-cli
```

### SSH

This section will setup SSH key based authentication to the server and disable password based authentication.

If you don't already have an SSH key, run `ssh-keygen -t ed25519` on a local machine and save to `~/.ssh/id_cluster`. Then running the following locally to transfer the SSH key to the server (ajdusting ip address appropriately):

```
cat ~/.ssh/id_cluster.pub | ssh cluster-user@10.1.0.129 "mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys"
```

On the server, add the following line to the end of `/etc/sudoers`:

```
cluster-user ALL=(ALL) NOPASSWD:ALL
```

On the server, run the following to disable the password for the user:

```
sudo usermod --pass='*' cluster-user
```

Modify `/etc/ssh/sshd_config` and set the following line:

```
PasswordAuthentication no
```

also remove the file that overrides this setting:

```
sudo rm /etc/ssh/sshd_config.d/50-cloud-init.conf
```

After restarting, you will be able to continue SSHing in using the key with a command like:

```
ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.129
```

### Static IP

In this section we will setup the following:

- Static IP for one of the 1 Gbps Ethernet ports (connecting through my network switch to the outside world).
  - We will use `10.1.1.4` as the static IP for this machine.
- Static IP for one of the 10 Gbps SFP ports (direct connection to my PC. No internet access).

Ubuntu Server uses `systemd-networkd` for network setup, so we can do this as follows:

Create `/etc/systemd/network/20-eth.network` with the following:

```
[Match]
Name=eno1

[Network]
Address=10.1.1.4/16
Gateway=10.1.0.1
DNS=10.1.0.1
```

Note that you way need to change the name based on what is listed in `ip addr`

And create `/etc/systemd/network/21-sfp.network` with the following

```
[Match]
Name=enp136s0d1

[Network]
Address=10.2.0.1/16
# No gateway so can't reach the internet through this interface
```

TODO: The name of this changes. Sometimes it is `enp132s0d1`

And delete the file `/etc/netplan/50-cloud-init.yaml` to avoid the interface getting overriden.

Separately on my own PC (conected via SFP), I've used the Ubuntu network UI to configure the network interface with:

- IP: `10.2.0.2/16`
- 'Only use it for resources on the network'

### GRUB

By default, the GRUB boot menu will wait 30 seconds before continuing to boot. We will decrease this to speed up boot time.

Edit `/etc/default/grub` and set

```
GRUB_TIMEOUT=4
GRUB_RECORDFAIL_TIMEOUT=4
```

Then run `sudo update-grub`.

### ZFS

#### HDD Pool

We can verify that all 15 disks are connected by running `lsblk` or `sudo fdisk -l` which will print out something like below:

```
Disk /dev/sda: 16.37 TiB, 18000207937536 bytes, 35156656128 sectors
Disk model: ST18000NM0092-3C
Units: sectors of 1 * 512 = 512 bytes
Sector size (logical/physical): 512 bytes / 4096 bytes
I/O size (minimum/optimal): 4096 bytes / 4096 bytes

...
```

As documented in the [Level1Techs guide](https://forum.level1techs.com/t/how-to-zfs-on-dual-actuator-mach2-drives-from-seagate-without-worry/197067/1), in order to fully utilize the dual actuator drives we need to treat each disk as two separate disks/partitions each with exactly half of the physical sectors.

Our general plan of action to form the ZFS pool will thus be as follows:

- Creating two separate partitions on each disk. Each partition will have a name of the format `Seagate-Disk-{SerialNumber}-{1|2}`.
  - Exactly the first 50% of the LBAs are reserved for each actuator and the final 50% for the second one.
  - The disks also support the `Concurrent Positioning Ranges` ATA log page if we wanted to programatically verify we are talking to dual actuator disks.
  - Note that each logical sector is 512 bytes and the the first 34 LBAs on the disk need to be reserved for the MBR and GPT headers. Similarly the last 33 LBAs are reserved for the backup partition table.
- Create the pool with 5 x 6 partition RAIDZ2 VDevs
  - A single vdev shouldn't use two partitions from the same physical disk.
  - A single vdev should prefer to not use adjacent disks for better isolation.

To do all of this, we run the below script after adjusting the serial numbers flag to specify the physical ordering/positions of the disks:

```
# Print out all the serial numbers. Then copy and paste them into the below command in the 1-1 to 1-15 order.
sudo hdparm -I /dev/sd? | grep 'Serial\ Number'

cargo build --bin zfs_util

scp target/debug/zfs_util dennis@10.1.0.129:~

sudo ./zfs_util create-mach2-pool \
    --disks='/dev/sd*' \
    --pool_name=tank \
    --serial_order=ZVV07AQD,ZVV06BRZ,ZVV05WDL,ZVV07JZE,ZVV07K3W,ZVV064Z9,ZVV04WAA,ZVV05WS1,ZVV07SFN,ZVV06YMG,ZVV05RZ2,ZVV0902E,ZVV01AEG,ZVV069XR,ZVV069DD \
    --num_disks_per_vdev=6 \
    --topology=raidz2
```

#### NVMe Pool

Next we will setup the pool for the NVMe drives. Per the [ZFS documentation](https://openzfs.github.io/openzfs-docs/Performance%20and%20Tuning/Hardware.html#nvme-low-level-formatting), we will first switch the drives to use 4KB sectors.

We can verify the current and supported sizes of the drive with a command shown below:

```
$ sudo smartctl -a /dev/nvme2n1
...
Supported LBA Sizes (NSID 0x1)
Id Fmt  Data  Metadt  Rel_Perf
 0 +     512       0         0
 1 -    4096       0         0
...
```

We then run the following command on each NVMe drive to switch to 4KB sectors:

```
sudo nvme format /dev/nvme0n1 --lbaf=1
```

WARNING: On some of my drives `0` is the 4KB one and NOT `1` so please check every drive to  verify the id to use.

Finally we create the pool:

```
sudo zpool create -o ashift=12 flash -f \
  mirror /dev/nvme0n1 /dev/nvme1n1 /dev/nvme2n1 /dev/nvme3n1

# Export and re-import with /dev/disk/by-id paths (since /dev/nvme paths are unstable).
sudo zpool export flash
sudo zpool import flash -d /dev/disk/by-id

```

The `-f` is required as some of my disks were slightly different sizes due to being slightly different SKUs of the same product.

#### Datasets

To create the datasets, we will start by setting overall ZFS tuning parameters and creating a directory to store encryption keys:

```
sudo 
sudo mkdir /zfs/
sudo mkdir /zfs/keys
sudo chown root:root /zfs/keys
sudo chmod 700 /zfs/keys

sudo zfs set mountpoint=/zfs/tank tank
sudo zfs set atime=off tank
sudo zfs set relatime=off tank
sudo zfs set compression=off tank
sudo zfs set recordsize=1M tank

sudo zfs set mountpoint=/zfs/flash flash
sudo zfs set atime=off flash
sudo zfs set relatime=off flash
sudo zfs set compression=off flash
sudo zfs set recordsize=128K flash
```

Then add the following line to `/etc/fstab` and **restart** the server:

```
tmpfs   /zfs/keys         tmpfs   rw,nodev,nosuid,mode=0700          0  0
```

On a local secure machine, run the following command to generate a key and then **back it up** somewhere:

```
mkdir -p ~/.credentials
head -c 32 /dev/urandom > ~/.credentials/nas-data.key
```

Then we will copy the key onto the server into the tmp dir:

```
cat ~/.credentials/nas-data.key | ssh -i ~/.ssh/id_cluster cluster-user@10.2.0.1 "sudo tee /zfs/keys/data.key > /dev/null"
```

TODO: The above command currently sets world readable permissions on the file by default (though the directory is not readable so this is probably ok).

Then we can create an encrypted dataset as follows:

```
sudo zpool set feature@encryption=enabled tank
sudo zfs create -o encryption=on -o keyformat=raw -o keylocation=file:///zfs/keys/data.key tank/data

sudo zpool set feature@encryption=enabled flash
sudo zfs create -o encryption=on -o keyformat=raw -o keylocation=file:///zfs/keys/data.key flash/data
```

We'll also create another unencrypted dataset for restic (since restic does its own encryption):

```
sudo zfs create tank/restic
```

If we later restart the machine, we will need to re-copy the key file and run the following to re-mount the datasets:

```
sudo zfs load-key -a
sudo zfs mount tank/data
sudo zfs mount flash/data
sudo rm /zfs/keys/data.key
```


#### Re-importing

Sometimes if there are pool failures and the machine is restarted, a pool may disappear from `zpool status`. In this case, you need to run something like below to re-discover/import the pool:

```
sudo zpool import tank
```

#### Scrubing

TODO: ZFS Scrubing. Default uses `/etc/cron.d/zfsutils-linux` in ubuntu (which is monthly)

#### Disk Spindown

The save power over night, we will have the drives spin down when not in use.

We can check is a drive is currently active with a command like this:

```
$ sudo hdparm -C /dev/sda

/dev/sda:
 drive state is:  active/idle

```

Run the following command for each drive to have each spin down after 2 hours of inactivity:

```
sudo hdparm -S 244 /dev/sda
```

Note that this is stored in disk firmware and should only need to be setup once.

Also be sure to disable `smartd` so that it doesn't periodically keep the drives alive:

```
sudo systemctl stop smartd
sudo systemctl disable smartd
sudo systemctl mask smartd
```

#### References

- ZFS Tuning
  - https://openzfs.github.io/openzfs-docs/Performance%20and%20Tuning/Workload%20Tuning.html#
  - https://forum.level1techs.com/t/zfs-guide-for-starters-and-advanced-users-concepts-pool-config-tuning-troubleshooting/196035
  - https://www.high-availability.com/docs/ZFS-Tuning-Guide/
- Seagate 2X18 HDD
  - https://www.seagate.com/content/dam/seagate/migrated-assets/www-content/manuals/exos-x-2x18/pdf/203859600a.pdf


### NVidia Drivers

Installing per the [CUDA download page instructions](https://developer.nvidia.com/cuda-downloads?target_os=Linux&target_arch=x86_64&Distribution=Ubuntu&target_version=24.04&target_type=deb_local):

```
wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2404/x86_64/cuda-ubuntu2404.pin
sudo mv cuda-ubuntu2404.pin /etc/apt/preferences.d/cuda-repository-pin-600
wget https://developer.download.nvidia.com/compute/cuda/12.8.0/local_installers/cuda-repo-ubuntu2404-12-8-local_12.8.0-570.86.10-1_amd64.deb
sudo dpkg -i cuda-repo-ubuntu2404-12-8-local_12.8.0-570.86.10-1_amd64.deb
sudo cp /var/cuda-repo-ubuntu2404-12-8-local/cuda-*-keyring.gpg /usr/share/keyrings/
sudo apt-get update
sudo apt-get -y install cuda-toolkit-12-8
```

Then install nvidia drivers:

```
sudo apt-get install -y nvidia-open
```

and finally reboot.

You can verify it is working by running `nvidia-smi`


### Cluster Setup

We will follow the cluster node setup guide [here](/pkg/cluster/index.md) to set up the server as a cluster node. It is a good idea to read that to get a good idea of the semantics but all the needed commands are listed below. Note that newer Ubuntu alredy sets up CGroups V2 so nothing is required for that.

Assuming that the ZFS datasets are currently unlocked and mounted, we used the following commands.

We will set up ZFS permissions as follows:

```
sudo chown cluster-user:cluster-user /zfs/flash/data

# RWX , RX , RX
sudo chmod 755 /zfs/flash/data
```

Then we will use the following command to setup the first cluster node on the flash pool:

```
cargo run --bin cluster_cli -- \
    setup_node \
    --zone=home \
    --node_addr=10.1.1.4 \
    --ssh_args="-i ~/.ssh/id_cluster" \
    --enable_service=false \
    --base_dir=/zfs/flash/data/node \
    --first_user_name=$USER \
    --bootstrap
```

Then setup a label for the node:

```
cargo run --bin cluster_cli -- list nodes

cargo run --bin cluster_cli -- \
  labels set --node_id=[insert] "name=nas"
```


At this point it is recommended to back up the root key stored in `~/.dacha/zone/home/root` in your local machine.

Whenever the server is restarted, the node service won't be running so we will need to manually SSH in to unlock the ZFS datasets and then start the service. To do this all in one step, use the following command:

```
cargo run --bin cluster_cli -- \
    unlock pkg/cluster/machines/nas/config.txtpb
```

### PC Backup

See the [backup](./backup.md) documentation.

#### Fan Controller

TODO

#### ZFS Permissions

TODO: Setup proper user:group permissions on all the datasets.

### SSH FS

On my local machine run:

```
sudo apt install sshfs
sudo mkdir /mnt/nas
sudo chown dennis:dennis /mnt/nas
sshfs -o default_permissions nas:/zfs /mnt/nas
```

See also https://www.digitalocean.com/community/tutorials/how-to-use-sshfs-to-mount-remote-file-systems-over-ssh


## Power Measurements

- Off
    - 4.5W
- Idle with just CX312B, no disks or GPU
    - 48W
- Adding a A2000 GPU
    - 20W idle without a driver
- Adding an Nvidia 1050 Ti
    - 10W idle without a driver
- Adding an Nvidia P4
    - 14W idle without a driver.
- Adding a single 22110 SSD
    - 4W
- Adding a single HDD
    - 5W idle
    - 10W spinning (including the above 5W)


## TODOs

- Periodic disk checking?
- Program to:
  - Run fans
  - Do disk spindown
  - Do temperature monitoring
  - Do ZFS monitoring
- Underclock the GPU?
    - https://github.com/kevinlekiller/nvidia-control-linux/blob/master/nvidia-control.sh



Reading temperatures:

```
/sys/class/hwmon

/sys/class/hwmon/hwmon0/name

lrwxrwxrwx 1 root root 0 Feb  1 17:16 /sys/class/hwmon/hwmon0 -> ../../devices/pci0000:80/0000:80:01.3/0000:83:00.0/nvme/nvme2/hwmon0

https://www.kernel.org/doc/Documentation/hwmon/sysfs-interface

```


Internal USB connector
- 20-pin (2x10) (19 pins used)
- 2mm pitch
- Something like G823J201240BHR (0.4mm thick contacts)

