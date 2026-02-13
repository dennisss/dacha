#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::collections::HashMap;

use base_error::*;
use file::LocalPath;

/*
cargo run --bin v4l2

cargo run --bin builder -- build //third_party/v4l2:v4l2 --config=//pkg/builder/config:rpi64

scp -r -i ~/.ssh/id_cluster built/third_party/v4l2/v4l2 cluster-user@10.1.1.3:~/
*/



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
    let mut device_num_to_path = HashMap::new();

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

        device_num_to_path.insert(dev.device_num(), dev.path().as_str().to_string());

        // if dev.
    }


    for mut dev in v4l2::SubDevice::list().await? {
        println!("====");
        println!("Sub Device: {}", dev.path().as_str());

        println!("Controls:");
        for control in dev.list_controls()? {
            // TODO: Also print if it is disabled.
            println!("- {}", control.to_string()?);
        }

        device_num_to_path.insert(dev.device_num(), dev.path().as_str().to_string());
    }

    println!("{:?}", device_num_to_path);

    for mut dev in v4l2::MediaDevice::list().await? {
        println!("====");
        println!("Media: {}", dev.path().as_str());

        dev.print_device_info()?;

        for entity in dev.list_entities()? {
            println!("- Entity: {} : {:?}", entity.name()?, entity.typ());

            if let Some(dev_num) = entity.device_num() {
                println!("  - dev num: {:?}", dev_num);
                let mut path = "<unknown>";
                if let Some(p) = device_num_to_path.get(&dev_num) {
                    path = p.as_str();
                }

                println!("  - device: {}", path); 
            }


            for pad in entity.pads() {
                println!(" - pad: {} : {:?}", pad.index(), pad.flags());
            }

        }
    }


    // TODO: Configure the 'power_line_frequency'

    // TODO: Must verify the device has the streaming capability and the streaming
    // params has the timeperframe capability.

    Ok(())
}
