#[macro_use]
extern crate macros;

use std::time::Duration;
use std::{collections::HashSet, time::Instant};

use base_units::ByteCount;
use common::{
    errors::*,
    io::{Readable, Writeable},
};
use file::temp::TempDir;
use file::{LocalFileOpenOptions, LocalPath, LocalPathBuf};
use storage::devices::*;
use storage::partition::mbr;
use sys::{MountFlags, UmountFlags};

use rpi_imager::*;

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
struct ExtractCommand {
    image: LocalPathBuf,
    output_dir: LocalPathBuf,
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


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Write(cmd) => run_write_command(cmd).await,
        Command::Extract(cmd) => run_extract_command(cmd).await,
        Command::RewriteConfigTxt(cmd) => cmd.run().await
    }
}
