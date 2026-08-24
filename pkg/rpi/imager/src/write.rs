use std::time::Duration;
use std::{collections::HashSet, time::Instant};

use base_units::ByteCount;
use common::aligned::AlignedVec;
use common::bool_to_num;
use common::{
    errors::*,
    io::{Readable, Writeable},
};
use compression::transform::Transform;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use file::temp::TempDir;
use file::{LocalFileOpenOptions, LocalPath, LocalPathBuf, project_path};
use storage::devices::*;
use storage::partition::mbr;
use sys::{MountFlags, UmountFlags};

use crate::*;


const BUFFER_SIZE: usize = 4096 * 16; // 64 KiB


#[derive(Args)]
pub struct WriteCommand {
    pub image: LocalPathBuf,
    pub disk: String,
    pub ssh_public_key: Option<LocalPathBuf>,
    pub wpa_ssid: Option<String>,
    pub wpa_password: Option<String>,

    // When these are set, a static ip will be assigned to the ethernet port.
    pub ip_address: Option<net::ip::IPAddress>,
    pub netmask: Option<net::ip::IPAddress>,
    pub gateway: Option<net::ip::IPAddress>,
    pub network_config_type: Option<NetworkConfigType>,

    pub hardware_model: Option<HardwareModel>,

    /// Extra lines to append to the end of the config.txt file.
    pub config_txt_patch_file: Option<LocalPathBuf>,

    #[arg(default = false)]
    pub generate_first_boot: bool,

    #[arg(default = false)]
    pub no_confirm: bool,
}

#[derive(Default)]
pub struct WriteExtraArgs {
    pub extra_files: Vec<(LocalPathBuf, Vec<u8>)>,
}

#[derive(Args)]
pub enum NetworkConfigType {
    #[arg(name = "networkd")]
    Networkd,

    #[arg(name = "ifupdown")]
    Ifupdown,
}

async fn open_image_file(path: &LocalPath) -> Result<(Box<dyn Readable>, usize)> {
    let mut image_file = file::LocalFile::open(path)?;
    let image_meta = image_file.metadata().await?;
    println!(
        "[Image] Raw Size: {:?}",
        ByteCount::from(image_meta.len() as usize)
    );

    if path.extension() == Some("img") {
        println!("File is a raw .img");
        Ok((Box::new(image_file), image_meta.len() as usize))
    } else if path.extension() == Some("gz") {
        let gzip_file = compression::gzip::GzipFile::open(image_file).await?;
        let size = gzip_file.uncompressed_size();
        println!("[GZip] Inner Size: {:?}", ByteCount::from(size));

        Ok((Box::new(gzip_file.data_reader()), size))
    } else {
        Err(format_err!(
            "Unsupported image format in file: {}",
            path.as_str()
        ))
    }
}

async fn find_usb_mass_storage_gadget() -> Result<LocalPathBuf> {
    println!("Looking for attached Pi in USB mass storage gadget mode...");

    let usb_device = {
        let ctx = usb::Context::create()?;

        let mut out = None;

        for dev in ctx.enumerate_devices().await? {
            let desc = dev.device_descriptor()?;
            if desc.idVendor == 0x0a5c && desc.idProduct == 0x0104 {
                println!("  => Found USB device");
                out = Some(dev);
                break;
            }
        }

        out.ok_or_else(|| err_msg("Couldn't find USB device"))?
    };

    let usb_real_path = file::realpath(usb_device.sysfs_dir()).await?;

    let block_devs = storage::devices::BlockDevice::list().await?;

    for block_dev in block_devs {
        if let Some(block_dev_path) = &block_dev.device_path {
            if block_dev_path.starts_with(&usb_real_path) {
                let p = LocalPath::new("/dev").join(&block_dev.name);
                println!("  => Found block device: {}", p.as_str());
                return Ok(p);
            }
        }
    }

    Err(err_msg("No block device attached to the USB device"))
}

pub async fn run_write_command(cmd: WriteCommand) -> Result<()> {
    run_write_command_ext(cmd, WriteExtraArgs::default()).await
}

pub async fn run_write_command_ext(cmd: WriteCommand, ext: WriteExtraArgs) -> Result<()> {

    let mut disk_path: LocalPathBuf = cmd.disk.as_str().into();

    if cmd.disk == "mass-storage-gadget" {
        disk_path = find_usb_mass_storage_gadget().await?;
    }

    // Command validation (goal is to error out early)
    {
        if !file::exists(&cmd.image).await? {
            return Err(format_err!("No image found at \"{:?}\"", cmd.image));
        }

        if !file::exists(&disk_path).await? {
            return Err(format_err!("No disk found at \"{:?}\"", disk_path));
        }

        if cmd.wpa_password.is_some() != cmd.wpa_ssid.is_some() {
            return Err(err_msg(
                "--wpa_ssid and --wpa_password must both be set to override WIFI settings.",
            ));
        }

        if let Some(path) = &cmd.ssh_public_key {
            if !file::exists(path).await? {
                return Err(format_err!("File does not exist: {:?}", path));
            }
        }

        let num_ip_args = bool_to_num!(cmd.ip_address.is_some())
            + bool_to_num!(cmd.gateway.is_some())
            + bool_to_num!(cmd.netmask.is_some());
        if num_ip_args == 3 {
            // TODO: Verify they are all IP v4
            // TODO: Check that 'gateway & mask' == ip_address & mask

            if cmd.ip_address == cmd.gateway {
                return Err(err_msg("--ip_address == --gateway. Probably a mistake?"));
            }
        } else if num_ip_args != 0 {
            return Err(err_msg(
                "Must set --ip_address, --gateway, --netmask as one set.",
            ));
        }
    }

    let (mut image_file, image_size) = open_image_file(&cmd.image).await?;

    // NOTE: After the image is written, the 'partitions' field of this will become
    // invalid.
    let disk_entry = BlockDevice::list()
        .await?
        .into_iter()
        .find(|disk| &format!("/dev/{}", disk.name) == disk_path.as_str())
        .ok_or_else(|| format_err!("Disk \"{:?}\" is not a block device", disk_path))?;

    if !disk_entry.removable {
        return Err(err_msg("Attempting to write to a non-removable disk?"));
    }

    if disk_entry.size >= 200 * 1024 * 1024 * 1024 {
        println!(
            "Disk is very large ({:?}), are you sure you want to write to it? [Y/n]",
            base_units::ByteCount::from(disk_entry.size)
        );

        if cmd.no_confirm {
            println!("[Ignoring]");
        } else {
            if !file::read_user_confirmation().await? {
                return Ok(());
            }
        }
    }

    let model = disk_entry
        .model
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("<unknown>");

    println!(
        "[Disk] Model: \"{}\"; Size: {:?}",
        model,
        ByteCount::from(disk_entry.size)
    );

    if image_size > disk_entry.size {
        return Err(err_msg("Image is too large to write to the disk"));
    }

    // Ensure that all references to the device are unmounted before we start
    // writing to it.
    disk_entry.unmount_all().await?;

    println!("Opening disk...");

    let mut disk_file = file::LocalFile::open_with_options(
        &disk_path,
        &LocalFileOpenOptions::new()
            .write(true)
            .direct(true)
            .exclusive(true),
    )?;

    println!("Starting write...");

    let block_size = disk_entry.logical_block_size;
    let mut buffer = AlignedVec::<u8>::new(BUFFER_SIZE, block_size);

    let mut offset = 0;

    let mut progress = ProgressTracker::new(image_size);

    // TODO: Use ioctl BLKSSZGET to get the logical block size for disk I/O.

    let mut hasher = SHA256Hasher::default();

    // TODO: Pipeline the reading and writing.
    while offset < image_size {
        let n = core::cmp::min(BUFFER_SIZE, image_size - offset);
        image_file.read_exact(&mut buffer[..n]).await?;

        hasher.update(&buffer[..n]);

        // Pad with zeros.
        buffer[n..].fill(0);

        // Number of bytes to write (block aligned 'n')
        let n_aligned = common::ceil_div(n, block_size) * block_size;

        disk_file.write_all(&mut buffer[0..n_aligned]).await?;

        offset += n;
        progress.update(offset);
    }

    // Verify that we did indeed fit the end of the file reader (for compressed
    // archives this will also verify the checksums).
    {
        let mut buf = [0u8; 1];
        let n = image_file.read(&mut buf[..]).await?;
        if n != 0 {
            return Err(err_msg("Extra unwritten data at the end of the image file"));
        }
    }

    disk_file.sync_all().await?;

    // TODO: Verify the contents on disk.

    // Allow the other sub-processes that we are using to access the disk.
    drop(disk_file);

    println!("Re-sync...");
    disk_entry.kernel_rescan()?;

    println!("Expanding root partition...");

    // Expand the root '/' partition to fill the entire disk.
    //
    // Example command: sudo parted -s /dev/sdb "resizepart 2 -1" quit
    {
        let status = std::process::Command::new("parted")
            .args(&["-s", disk_path.as_str(), "resizepart 2 -1", "quit"])
            .status()?;
        if !status.success() {
            return Err(err_msg("Failed to resize root partition"));
        }

        // TODO: Implement the above command in Rust code.
        /*
        let mut disk_first_sector = AlignedVec::new(512, block_size);
        disk_file.seek(0);
        disk_file.read_exact(&mut disk_first_sector).await?;

        let mut mbr = storage::partition::mbr::parse_mbr(&disk_first_sector)?;

        println!("{:#?}", mbr);
        */
    }

    println!("Reading fs type...");
    let root_part_fstype_str = get_partition_fstype(LocalPath::new(&format!("{}2", &disk_path.as_str()))).await?;
    println!("=> fs type: {}", root_part_fstype_str);

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum FSType {
        EXT4,
        BTRFS
    }

    let root_part_fstype = match root_part_fstype_str.as_str() {
        "ext4" => FSType::EXT4,
        "btrfs" => FSType::BTRFS,
        _ => {
            return Err(format_err!("Unsupported root fs type: {}", root_part_fstype_str))
        }
    };

    // BTRFS likes keeping track of all the partitions it has ever mounted so it will
    // get annoyed if mounting an old version of a filesystem it has previously modified. 
    //
    // NOTE: We don't randomize the MBR/GPT UUID since that is referenced in the fstab and
    // cmdline.txt files.
    if root_part_fstype == FSType::BTRFS {
        println!("Randomize BTRFS UUID...");

        let dev_name = format!("{}2", &disk_path.as_str());

        let status = std::process::Command::new("btrfstune")
            .args(&["-f", "-u", &dev_name])
            .status()?;
        if !status.success() {
            return Err(err_msg("Failed to randomize BTRFS partition UUID"));
        }
    }

    println!("Mounting boot filesystem..");
    let boot_dir = TempDir::create()?;
    {
        // TODO: Re-lookup the partitions list from BlockDevices::list()
        let dev_name = format!("{}1", &disk_path.as_str());
        let dir_name = boot_dir.path().as_str();

        println!("{} => {}", dev_name, dir_name);

        sys::mount(
            Some(&dev_name),
            dir_name,
            Some("vfat"),
            MountFlags::empty(),
            None,
        )?;
    }

    println!("Mounting root filesystem...");

    let root_dir = TempDir::create()?;
    {
        // TODO: Re-lookup the partitions list from BlockDevices::list()
        let dev_name = format!("{}2", &disk_path.as_str());
        let dir_name = root_dir.path().as_str();

        println!("{} => {}", dev_name, dir_name);

        sys::mount(
            Some(&dev_name),
            dir_name,
            Some(&root_part_fstype_str),
            MountFlags::empty(),
            None,
        )?;
    }

    // Sometimes takes time before the root fs is fully loaded.
    // (else the next command will fail.)
    executor::sleep(Duration::from_secs(2)).await?;

    // Example command: sudo btrfs filesystem resize max /media/dennis/rootfs
    println!("Expanding root filesystem...");
    match root_part_fstype {
        FSType::BTRFS => {
            let status = std::process::Command::new("btrfs")
                .args(&["filesystem", "resize", "max", root_dir.path().as_str()])
                .status()?;

            if !status.success() {
                return Err(err_msg("Failed to resize root file system"));
            }
        }
        FSType::EXT4 => {
            let dev_name = format!("{}2", &disk_path.as_str());
            let dir_name = root_dir.path().as_str();

            // resize2fs is at an older version of many OSes so may fail due to unsupported features.
            let status = std::process::Command::new("/lib/systemd/systemd-growfs" /* "resize2fs" */)
                .args(&[dir_name /* dev_name */])
                .status()?;

            if !status.success() {
                return Err(err_msg("Failed to resize root file system"));
            }
        }
    }

    println!("Reading config.txt...");
    let mut config_txt = {
        ConfigTxtFile::parse(
            &file::read_to_string(boot_dir.path().join("config.txt")).await?
        )?
    };

    if let Some(path) = cmd.config_txt_patch_file {
        let patch = ConfigTxtFile::parse(
            &file::read_to_string(path).await?
        )?;

        config_txt.extend(patch);
    } 

    if let Some(model) = &cmd.hardware_model {
        config_txt.filter_to_hardware(model);
    }

    println!("Writing /etc/image-id...");
    {
        let id = format!("name:{}\nsha256:{}\n", cmd.image.file_name().unwrap(), base_radix::hex_encode(&hasher.finish()));
        file::write(root_dir.path().join("etc/image-id"), id).await?;
    }

    if cmd.generate_first_boot {
        println!("Writing /etc/machine-id...");

        let mut id = [0u8; 16];
        crypto::random::secure_random_bytes(&mut id).await?;
        file::write(root_dir.path().join("etc/machine-id"), format!("{}\n", base_radix::hex_encode(&id))).await?;

        create_or_update_symlink(
            "/etc/machine-id",
            root_dir.path().join("var/lib/dbus/machine-id")
        ).await?;

        println!("Generating SSH host keys...");

        {
            let status = std::process::Command::new("/bin/bash")
                .args(&[project_path!("pkg/rpi/imager/gen_ssh_keys.sh").as_str(), root_dir.path().as_str()])
                .status()?;

            if !status.success() {
                return Err(err_msg("Failed to generate SSH key"));
            }
        }
    }

    if let Some(path) = &cmd.ssh_public_key {
        println!("Adding SSH authorized_keys...");

        let data = format!("\n{}\n", file::read_to_string(path).await?.trim());

        let user_dirs = file::read_dir(root_dir.path().join("home"))?;
        if user_dirs.len() != 1 {
            return Err(err_msg(
                "Expected the image to contain exactly one user directory in /home",
            ));
        }

        let dest = root_dir
            .path()
            .join("home")
            .join(user_dirs[0].name())
            .join(".ssh/authorized_keys");
        if !file::exists(&dest).await? {
            return Err(err_msg("No authorized_keys file setup for user."));
        }

        file::append(&dest, data).await?;
    }

    if cmd.ip_address.is_some() {
        println!("Configuring static ip...");

        let ip_addr = cmd.ip_address.as_ref().unwrap();
        let netmask = cmd.netmask.as_ref().unwrap();
        let gateway = cmd.gateway.as_ref().unwrap();

        match cmd.network_config_type.unwrap_or(NetworkConfigType::Networkd) {
            NetworkConfigType::Ifupdown => {
                let interfaces_file = root_dir.path().join("etc/network/interfaces");

                if !file::exists(&interfaces_file).await? {
                    return Err(err_msg("/etc/network/interfaces doesn't exist in the image. Most likely it was built without the 'ifupdown' package."));
                }

                file::append(
                    interfaces_file,
                    format!(
                        "
                        allow-hotplug eth0
                        iface eth0 inet static
                        address {ip_addr}
                        netmask {netmask}
                        gateway {gateway}
                        ",
                        ip_addr = ip_addr.to_string(),
                        netmask = netmask.to_string(),
                        gateway = gateway.to_string()
                    ),
                )
                .await?;
            }
            NetworkConfigType::Networkd => {
                // Note that this is the same path as used for the default DHCP config in our custom image so
                // will override the DHCP config.
                let path = root_dir.path().join("etc/systemd/network/10-eth0.network");

                file::write(
                    &path,
                    format!(
                        "
                        [Match]
                        Name=eth0

                        [Network]
                        Address={ip_addr}/{netmask}
                        Gateway={gateway}
                        DNS={gateway}
                        ",
                        ip_addr = ip_addr.to_string(),
                        netmask = netmask_num_bits(&netmask)?,
                        gateway = gateway.to_string()
                    )
                ).await?;
            }
        }


    }

    if cmd.wpa_ssid.is_some() {
        println!("Setting WIFI credentials...");

        let ssid = cmd.wpa_ssid.as_ref().unwrap();
        let pass = cmd.wpa_password.as_ref().unwrap();

        let output = std::process::Command::new("wpa_passphrase")
            .args(&[ssid, pass])
            .output()?;
        if !output.status.success() {
            return Err(format_err!("Failed to generate WPA PSK: {:?}", output));
        }

        let contents = format!("\n{}\n", String::from_utf8(output.stdout)?.trim());

        file::append(
            root_dir
                .path()
                .join("etc/wpa_supplicant/wpa_supplicant.conf"),
            contents,
        )
        .await?;
    }

    println!("Writing config.txt...");
    {
        file::write(
            boot_dir.path().join("config.txt"),
            config_txt.to_string()
        ).await?;
    }

    for (final_path, data) in ext.extra_files {
        println!("Writing {}...", final_path.display());

        let write_path = {
            if let Ok(p) = final_path.strip_prefix("/boot/firmware") {
                boot_dir.path().join(p)

            } else {
                root_dir.path().join(final_path.strip_prefix("/").unwrap())
            }
        };

        file::write(write_path, data).await?;
    }

    println!("Unmount boot filesystem...");
    sys::umount(boot_dir.path().as_str(), UmountFlags::empty())?;

    println!("Unmount root filesystem...");
    sys::umount(root_dir.path().as_str(), UmountFlags::empty())?;

    println!("Done!");

    Ok(())
}

fn netmask_num_bits(ip: &net::ip::IPAddress) -> Result<usize> {
    let v = match ip {
        net::ip::IPAddress::V4(v) => u32::from_be_bytes(*v),
        _ => return Err(err_msg("Expected netmask to be ipv4"))
    };

    // TODO: Verify there are no other bits in the address aside from leading ones.
    let n = v.leading_ones() as usize;

    Ok(n)
}

// TODO: Dedup this.
pub async fn create_or_update_symlink<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(
    original: P,
    link_path: P2,
) -> Result<()> {
    let original = original.as_ref();
    let link_path = link_path.as_ref();

    if let Some(parent) = link_path.parent() {
        file::create_dir_all(parent).await?;
    }

    // TODO: Check this.
    if let Ok(_) = file::symlink_metadata(&link_path).await {
        file::remove_file(&link_path).await?;
    }

    file::symlink(original, link_path).await?;

    Ok(())
}
