// CLI utility

#[macro_use]
extern crate macros;

use common::errors::*;
use media_camera::camera_manager::CameraManager;
use video::mp4::{MP4Builder, MP4BuilderOptions};

#[derive(Args)]
struct Args {
    // TODO: make it more explicit that this is a positional argument with #[arg(positional)]
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "list")]
    List(ListCommand),

    #[arg(name = "record")]
    Record(RecordCommand),
}

#[derive(Args)]
struct ListCommand {}

#[derive(Args)]
struct RecordCommand {
    camera_id: String,
}

#[executor_main]
async fn main() -> Result<()> {
    // libcamera::disable_logging();

    let args = common::args::parse_args::<Args>()?;

    let camera_manager = CameraManager::create()?;

    let mut entries = camera_manager.list().await?;

    match args.command {
        Command::List(_) => {
            for (id, entry) in entries {
                println!("\"{}\"\n    {}", id, entry.name().await?);
            }
        }
        Command::Record(cmd) => {
            let entry = entries
                .remove(&cmd.camera_id)
                .ok_or_else(|| err_msg("Unknown camera with given id"))?;

            let mut subscriber = camera_manager.open(entry).await?;
            let format = subscriber.format().await?;

            let mut mp4_builder = MP4Builder::new(
                format.width,
                format.height,
                format.frame_rate,
                MP4BuilderOptions::default(),
            )?;

            for i in 0..(5 * format.frame_rate) {
                if i % format.frame_rate == 0 {
                    println!("{}", i / format.frame_rate);
                }

                let frame = subscriber.recv().await?;

                // TODO: Propagate the frame timestamps.
                mp4_builder.append(frame.data.data().unwrap(), None, false)?;
            }

            mp4_builder.append(&[], None, true)?;

            let mut out = vec![];
            while let Some(event) = mp4_builder.consume() {
                out.extend_from_slice(&event.data);
            }

            file::write("image.mp4", out).await?;
        }
    }

    Ok(())
}
