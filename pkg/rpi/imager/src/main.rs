#[macro_use]
extern crate macros;

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
use file::{LocalFileOpenOptions, LocalPath, LocalPathBuf};
use storage::devices::*;
use storage::partition::mbr;
use sys::{MountFlags, UmountFlags};

const BUFFER_SIZE: usize = 4096 * 16; // 64 KiB

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "write")]
    Write(WriteCommand),

    #[arg(name = "extract")]
    Extract(ExtractCommand),

    #[arg(name = "rewrite-config-txt")]
    RewriteConfigTxt(RewriteConfigTxtCommand)
}

#[derive(Args)]
struct WriteCommand {
    image: LocalPathBuf,
    disk: String,
    ssh_public_key: Option<LocalPathBuf>,
    wpa_ssid: Option<String>,
    wpa_password: Option<String>,

    // When these are set, a static ip will be assigned to the ethernet port.
    ip_address: Option<net::ip::IPAddress>,
    netmask: Option<net::ip::IPAddress>,
    gateway: Option<net::ip::IPAddress>,
    network_config_type: Option<NetworkConfigType>,

    hardware_model: Option<HardwareModel>,

    /// Extra lines to append to the end of the config.txt file.
    config_txt_patch_file: Option<LocalPathBuf>,

    #[arg(default = false)]
    no_confirm: bool,
}

#[derive(Args)]
enum NetworkConfigType {
    #[arg(name = "networkd")]
    Networkd,

    #[arg(name = "ifupdown")]
    Ifupdown,
}

#[derive(Args)]
enum HardwareModel {
    #[arg(name = "pi4")]
    Pi4,

    #[arg(name = "pi5")]
    Pi5,

    #[arg(name = "cm4")]
    Cm4,

    #[arg(name = "cm5-regular")]
    Cm5Regular,

    #[arg(name = "cm5-lite")]
    Cm5Lite
}

impl HardwareModel {
    fn config_txt_filter(&self) -> &'static str {
        match self {
            Self::Pi4 => "pi4",
            Self::Pi5 => "pi5",
            Self::Cm4 => "cm4",
            Self::Cm5Regular | Self::Cm5Lite => "cm5",
        }
    }

    fn device_tree(&self) -> &'static str {
        match self {
            Self::Pi4 => "bcm2711-rpi-4-b.dtb",
            Self::Pi5 => "bcm2712-d-rpi-5-b.dtb",
            Self::Cm4 => "bcm2711-rpi-cm4.dtb",
            Self::Cm5Regular => "bcm2712-rpi-cm5-cm5io.dtb",
            Self::Cm5Lite => "bcm2712-rpi-cm5l-cm5io.dtb",
        }
    }

    fn kernel(&self) -> &'static str {
        match self {
            Self::Pi4 | Self::Cm4 => "kernel8.img",
            Self::Pi5 | Self::Cm5Regular | Self::Cm5Lite => "kernel_2712.img",
        }
    }
}

#[derive(Args)]
struct ExtractCommand {
    image: LocalPathBuf,
    output_dir: LocalPathBuf,
}

struct ProgressTracker {
    start_time: Instant,
    total_bytes: usize,

    last_time: Instant,
    last_percentage: usize,
    last_written_bytes: usize,
}

impl ProgressTracker {
    fn new(total_bytes: usize) -> Self {
        let t = Instant::now();
        Self {
            start_time: t.clone(),
            total_bytes,

            last_time: t.clone(),
            last_percentage: 0,
            last_written_bytes: 0,
        }
    }

    fn update(&mut self, written_bytes: usize) {
        let percent = (100 * written_bytes) / self.total_bytes;
        if percent == self.last_percentage {
            return;
        }

        let time = Instant::now();

        let rate = ((written_bytes - self.last_written_bytes) as f64)
            / (time - self.last_time).as_secs_f64();
        println!("=> {}% [{:?}/s]", percent, ByteCount::from(rate as usize));

        if percent == 100 {
            println!("Done! Took: {:?}", time - self.start_time);
        }

        self.last_percentage = percent;
        self.last_written_bytes = written_bytes;
        self.last_time = time;
    }
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

async fn run_write_command(cmd: WriteCommand) -> Result<()> {

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

    // BTRFS likes keeping track of all the partitions it has ever mounted so it will
    // get annoyed if mounting an old version of a filesystem it has previously modified. 
    //
    // NOTE: We don't randomize the MBR/GPT UUID since that is referenced in the fstab and
    // cmdline.txt files.
    println!("Randomize BTRFS UUID...");
    {
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
            Some("btrfs"),
            MountFlags::empty(),
            None,
        )?;
    }

    // Example command: sudo btrfs filesystem resize max /media/dennis/rootfs
    println!("Expanding root filesystem...");
    {
        let status = std::process::Command::new("btrfs")
            .args(&["filesystem", "resize", "max", root_dir.path().as_str()])
            .status()?;

        if !status.success() {
            return Err(err_msg("Failed to resize root file system"));
        }
    }

    println!("Reading config.txt...");
    let mut config_txt = {
        ConfigTxtFile::parse(
            &file::read_to_string(boot_dir.path().join("config.txt")).await?
        )?
    };

    if let Some(model) = &cmd.hardware_model {
        config_txt.filter_to_hardware(model);
    }

    if let Some(path) = cmd.config_txt_patch_file {
        let patch = ConfigTxtFile::parse(
            &file::read_to_string(path).await?
        )?;

        config_txt.lines.extend(patch.lines);
    } 

    println!("Writing /etc/image-id...");
    {
        let id = format!("name:{}\nsha256:{}\n", cmd.image.file_name().unwrap(), base_radix::hex_encode(&hasher.finish()));
        file::write(root_dir.path().join("etc/image-id"), id).await?;
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

async fn run_extract_command(cmd: ExtractCommand) -> Result<()> {
    let tmp_dir = file::temp::TempDir::create()?;

    let mut image_path = cmd.image.to_owned();

    // Verify that the image isn't compressed
    {
        if cmd.image.extension() == Some("img") {
            // Good
        } else if cmd.image.extension() == Some("gz") {
            let mut image_file = file::LocalFile::open(&cmd.image)?;

            let gzip_file = compression::gzip::GzipFile::open(image_file).await?;
            let size = gzip_file.uncompressed_size();
            println!("[GZip] Pre-extracting image... Inner Size: {:?}", ByteCount::from(size));
    
            let mut reader = gzip_file.data_reader();

            image_path = tmp_dir.path().join("image.img");

            let mut out_file = file::LocalFile::open_with_options(
                &image_path, &file::LocalFileOpenOptions::new().create(true).write(true))?;

            let mut progress = ProgressTracker::new(size);            
            reader.pipe_with_progress(&mut out_file, &mut |v| progress.update(v)).await?;
        } else {
            return Err(format_err!(
                "Unsupported image format in file: {}",
                cmd.image.as_str()
            ));
        }
    }

    let mut image_file = file::LocalFile::open(&image_path)?;

    // Find the root partition in the image.
    let (part_offset, part_size) = {
        let mut first_sector = [0u8; storage::LOGICAL_BLOCK_SIZE];
        image_file.read_exact(&mut first_sector).await?;

        let mbr = mbr::parse_mbr(&first_sector)?;

        let expected_partitions = &[
            mbr::PartitionType::FAT32_LBA,
            mbr::PartitionType::LinuxFilesystem,
        ];

        for (i, entry) in mbr.partition_entries.iter().enumerate() {
            let expected_type = expected_partitions
                .get(i)
                .cloned()
                .unwrap_or(mbr::PartitionType::Empty);
            if expected_type != entry.partition_type {
                return Err(err_msg("Unexpected partition layout in image"));
            }
        }

        let partition = &mbr.partition_entries[1];
        let offset = partition.first_absolute_sector_lba as usize * storage::LOGICAL_BLOCK_SIZE;
        let size = partition.num_sectors as usize * storage::LOGICAL_BLOCK_SIZE;

        (offset, size)
    };

    // Mount image as a loop device

    let loop_ctl = file::LocalFile::open_with_options(
        "/dev/loop-control",
        LocalFileOpenOptions::new().read(true).write(true),
    )?;

    let loop_num =
        unsafe { sys::ioctl(loop_ctl.as_raw_fd(), sys::bindings::LOOP_CTL_GET_FREE, 0)? };

    let loop_path = LocalPathBuf::from(format!("/dev/loop{}", loop_num));
    println!(
        "Mounting root partition block device to {}",
        loop_path.as_str()
    );

    let loop_file = file::LocalFile::open_with_options(
        &loop_path,
        LocalFileOpenOptions::new().read(true).write(true),
    )?;

    unsafe {
        let mut loop_config = sys::bindings::loop_config::default();
        loop_config.fd = image_file.as_raw_fd() as u32;
        loop_config.block_size = 512;
        loop_config.info.lo_offset = part_offset as u64;
        loop_config.info.lo_sizelimit = part_size as u64;
        loop_config.info.lo_flags =
            sys::bindings::LO_FLAGS_READ_ONLY as u32 | sys::bindings::LO_FLAGS_AUTOCLEAR as u32;

        sys::ioctl(
            loop_file.as_raw_fd(),
            sys::bindings::LOOP_CONFIGURE,
            core::mem::transmute(&loop_config),
        )?;
    }

    let root_dir = TempDir::create()?;
    println!(
        "Mounting root filesystem to {}...",
        root_dir.path().as_str()
    );
    {
        sys::mount(
            Some(loop_path.as_str()),
            root_dir.path().as_str(),
            Some("btrfs"),
            MountFlags::MS_RDONLY | MountFlags::MS_NODEV | MountFlags::MS_NOSUID,
            None,
        )?;
    }

    if file::exists(&cmd.output_dir).await? {
        println!("Deleting old data...");
        file::remove_dir_all(&cmd.output_dir).await?;
    }

    // TODO: Verify no '/etc/machine-id' or SSH host keys are already present.

    println!("Copying files...");
    file::create_dir_all(cmd.output_dir.parent().unwrap()).await?;
    file::run_copy_command(file::CopyCommand {
        from: root_dir.path().to_owned(),
        to: cmd.output_dir.clone(),
        recursive: true,
        preserve_metadata: false,
        symlink_root: Some(root_dir.path().to_owned()),
        skip_permission_denied: true,
    })
    .await?;

    println!("Unmount root filesystem...");
    sys::umount(root_dir.path().as_str(), UmountFlags::empty())?;

    // NOTE: The loop device will auto-close

    Ok(())
}

#[derive(Args)]
struct RewriteConfigTxtCommand {
    input_path: LocalPathBuf,
    hardware_model: Option<HardwareModel>
}

impl RewriteConfigTxtCommand {
    async fn run(self) -> Result<()> {
        let data = file::read_to_string(&self.input_path).await?;
        let mut config = ConfigTxtFile::parse(&data)?;

        if let Some(model) = self.hardware_model {
            config.filter_to_hardware(&model);
        }

        println!("{}", config.to_string());

        Ok(())
    }
}

struct ConfigTxtFile {
    lines: Vec<ConfigTxtLine>
}

enum ConfigTxtLine {
    Filter(String),
    KeyValue(String, String)
}

impl ConfigTxtFile {
    fn parse(data: &str) -> Result<Self> {
        let mut lines = vec![];

        for mut line in data.lines() {
            line = line.trim();
            if line.starts_with("#") || line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("[") {
                if let Some(rest) = rest.strip_suffix("]") {
                    lines.push(ConfigTxtLine::Filter(rest.to_string()));
                    continue;
                }
            }

            let (key, value) = line.split_once("=")
                .ok_or_else(|| format_err!("Invalid config.txt key/value line: {}", line))?;

            lines.push(ConfigTxtLine::KeyValue(key.to_string(), value.to_string()));
        }

        Ok(Self {
            lines
        })
    }

    // TODO: Support removing the disable-wifi options if wireless is being setup.

    // TODO: Implement deduping (though some things like dtoverlays can't be deduped).

    fn filter_to_hardware(&mut self, model: &HardwareModel) {
        let mut current_section = "all".to_string();
    
        self.lines.retain(|line| {
            if let ConfigTxtLine::Filter(filter) = line {
                current_section = filter.clone();
                return false;
            } 

            current_section == "all" || current_section == model.config_txt_filter()
        });

        self.lines.insert(0, ConfigTxtLine::KeyValue("kernel".to_string(), model.kernel().to_string()));
        self.lines.insert(1, ConfigTxtLine::KeyValue("device_tree".to_string(), model.device_tree().to_string()));
    }

    fn to_string(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                ConfigTxtLine::Filter(v) => out.push_str(&format!("[{}]\n", v)),
                ConfigTxtLine::KeyValue(k, v) => out.push_str(&format!("{}={}\n", k, v))
            }
        }

        out
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Write(cmd) => run_write_command(cmd).await,
        Command::Extract(cmd) => run_extract_command(cmd).await,
        Command::RewriteConfigTxt(cmd) => cmd.run().await
    }
}
