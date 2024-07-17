#[macro_use]
extern crate macros;

use std::time::Duration;

use base_error::*;
use file::LocalPath;

async fn print_stream_info(dev: &v4l2::Device, stream: &v4l2::UnconfiguredStream) -> Result<()> {
    let formats = stream.list_formats().await?;

    println!("Formats:");
    for format in formats {
        println!("  - {:?}", format);
        println!(
            "    - Frame Sizes: {:?}",
            dev.list_frame_sizes(format.pixelformat.0).await?
        );
    }

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    for mut dev in v4l2::Device::list().await? {
        println!("====");
        println!("Device: {}", dev.path().as_str());

        dev.print_capabiliites().await?;

        println!("Controls:");
        for control in dev.list_controls().await? {
            // TODO: Also print if it is disabled.
            println!("- {}", control.to_string()?);
        }

        // TODO: Also list any extended controls.

        // println!("Inputs: {:?}", dev.list_inputs().await?);

        if dev.supports_capture_stream() {
            let stream = dev.new_capture_stream()?;

            println!("Capture Stream:");
            print_stream_info(&dev, &stream).await?;
        }

        if dev.supports_output_stream() {
            let stream = dev.new_output_stream()?;

            println!("Output Stream:");

            print_stream_info(&dev, &stream).await?;
        }

        // if dev.
    }

    // TODO: Configure the 'power_line_frequency'

    // TODO: Must verify the device has the streaming capability and the streaming
    // params has the timeperframe capability.

    Ok(())
}
