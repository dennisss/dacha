#[macro_use]
extern crate macros;

use std::time::Duration;
use std::{collections::HashSet, time::Instant};

use base_units::ByteCount;
use common::aligned::AlignedVec;
use common::{
    errors::*,
    io::{Readable, Writeable},
};
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use file::temp::TempDir;
use file::{LocalFileOpenOptions, LocalPathBuf};
use storage::devices::*;
use sys::{MountFlags, UmountFlags};

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "write")]
    Write(WriteCommand),
}

#[derive(Args)]
struct WriteCommand {
    image: LocalPathBuf,
    disk: LocalPathBuf,
    ssh_public_key: Option<LocalPathBuf>,
    wpa_ssid: Option<String>,
    wpa_password: Option<String>,
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

async fn run_write_command(cmd: WriteCommand) -> Result<()> {
    // Command validation (goal is to error out early)
    {
        if !file::exists(&cmd.image).await? {
            return Err(format_err!("No image found at \"{:?}\"", cmd.image));
        }

        if !file::exists(&cmd.disk).await? {
            return Err(format_err!("No disk found at \"{:?}\"", cmd.disk));
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
    }

    let mut image_file = file::LocalFile::open(&cmd.image)?;
    let image_meta = image_file.metadata().await?;
    println!(
        "[Image] Size: {:?}",
        ByteCount::from(image_meta.len() as usize)
    );

    // NOTE: After the image is written, the 'partitions' field of this will become
    // invalid.
    let disk_entry = BlockDevice::list()
        .await?
        .into_iter()
        .find(|disk| &format!("/dev/{}", disk.name) == cmd.disk.as_str())
        .ok_or_else(|| format_err!("Disk \"{:?}\" is not a block device", cmd.disk))?;

    if !disk_entry.removable {
        return Err(err_msg("Attempting to write to a non-removable disk?"));
    }

    if disk_entry.size >= 200 * 1024 * 1024 * 1024 {
        return Err(err_msg(
            "Disk is very large, are you sure you want to write to it?",
        ));
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

    if image_meta.len() as usize > disk_entry.size {
        return Err(err_msg("Image is too large to write to the disk"));
    }

    // Ensure that all references to the device are unmounted before we start
    // writing to it.
    {
        let mut device_paths = HashSet::<String>::default();
        device_paths.insert(format!("/dev/{}", disk_entry.name));
        for partition in disk_entry.partitions {
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
            sys::umount(&path, UmountFlags::empty())?;
        }
    }

    const BUFFER_SIZE: usize = 4096 * 16; // 64 KiB

    println!("Opening disk...");

    let mut disk_file = file::LocalFile::open_with_options(
        &cmd.disk,
        &LocalFileOpenOptions::new()
            .write(true)
            .direct(true)
            .exclusive(true),
    )?;

    println!("Starting write...");

    let block_size = disk_entry.logical_block_size;
    let mut buffer = AlignedVec::<u8>::new(BUFFER_SIZE, block_size);

    let mut offset = 0;

    let mut progress = ProgressTracker::new(image_meta.len() as usize);

    // TODO: Use ioctl BLKSSZGET to get the logical block size for disk I/O.

    let mut hasher = SHA256Hasher::default();

    while offset < (image_meta.len() as usize) {
        let n = core::cmp::min(BUFFER_SIZE, (image_meta.len() as usize) - offset);
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

    disk_file.sync_all().await?;

    // TODO: Verify the contents on disk.

    // Allow the other sub-processes that we are using to access the disk.
    drop(disk_file);

    println!("Re-sync...");

    // TODO: Make this work with file::write
    // (probably doesn't work as the file is not seekable).
    unsafe {
        let s = std::ffi::CString::new(format!("/sys/block/{}/device/rescan", disk_entry.name))
            .unwrap();

        let fd =
            sys::OpenFileDescriptor::new(sys::open(s.as_ptr(), sys::O_WRONLY | sys::O_CLOEXEC, 0)?);

        let mut buf = b"1";

        let n = sys::write(*fd, buf.as_ptr(), 1)?;

        assert_eq!(n, 1);
    }

    println!("Expanding root partition...");

    // Expand the root '/' partition to fill the entire disk.
    //
    // Example command: sudo parted -s /dev/sdb "resizepart 2 -1" quit
    {
        let status = std::process::Command::new("parted")
            .args(&["-s", cmd.disk.as_str(), "resizepart 2 -1", "quit"])
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

    println!("Mounting root filesystem...");

    let root_dir = TempDir::create()?;
    {
        // TODO: Re-lookup the partitions list from BlockDevices::list()
        let dev_name = format!("{}2", &cmd.disk.as_str());
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

    println!("Writing /etc/image-id...");
    {
        let id = format!("sha256:{}\n", base_radix::hex_encode(&hasher.finish()));
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

    println!("Unmount root filesystem...");
    sys::umount(root_dir.path().as_str(), UmountFlags::empty())?;

    println!("Done!");

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Write(cmd) => run_write_command(cmd).await,
    }
}
