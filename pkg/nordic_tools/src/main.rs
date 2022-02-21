#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate nordic_proto;
extern crate protobuf;
extern crate usb;

mod usb_radio;

use std::sync::Arc;
use std::time::Duration;

use common::async_std::{channel, task};
use common::errors::*;
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::net::*;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use usb_radio::USBRadio;

/*
Example packet:
20, 232, 232, 232, 232, 3, 0, 0, 0, 64, 72, 89, 33, 13, 87, 94, 124, 249, 158, 53, 0,

^ Why is the last byte of the tag usually 0?
*/

#[derive(Args)]
struct Args {
    num: usize,
    usb: String,
}

async fn line_reader(sender: channel::Sender<String>) -> Result<()> {
    loop {
        let mut line = String::new();
        common::async_std::io::stdin().read_line(&mut line).await?;
        sender.send(line).await;
    }
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let ctx = usb::Context::create()?;

    let mut device = {
        let mut found_device = None;

        for dev in ctx.enumerate_devices().await? {
            let desc = dev.device_descriptor()?;
            if desc.idVendor != 0x8888 || desc.idProduct != 0x0001 {
                continue;
            }

            let id = format!("{}:{}", dev.bus_num(), dev.dev_num());
            println!("Device: {}", id);

            if id == args.usb {
                found_device = Some(dev.open().await?);
            }
        }

        found_device.ok_or_else(|| err_msg("No device selected"))?
    };

    println!("Device opened!");

    device.reset()?;
    println!("Device reset!");

    let mut radio = USBRadio::new(device);

    // let mut device = ctx.open_device(0x8888, 0x0001).await?;

    let network_config = {
        if args.num == 1 {
            NetworkConfig::parse_text(
                r#"
                address: "\xE7\xE7\xE7\xE7"
                last_packet_counter: 1
        
                links {
                    address: "\xE8\xE8\xE8\xE8"
                    key: "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"
                    iv: "\x11\x12\x13\x14\x15"
                }
                "#,
            )?
        } else if args.num == 2 {
            NetworkConfig::parse_text(
                r#"
                address: "\xE8\xE8\xE8\xE8"
                last_packet_counter: 1
        
                links {
                    address: "\xE7\xE7\xE7\xE7"
                    key: "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"
                    iv: "\x11\x12\x13\x14\x15"
                }
                "#,
            )?
        } else {
            return Err(err_msg("Unknown device number"));
        }
    };

    radio.set_config(&network_config).await?;

    println!("get_config: {:?}", radio.get_config().await?);

    let (sender, receiver) = channel::bounded(1);

    let reader_task = task::spawn(async move {
        println!("{:?}", line_reader(sender).await);
    });

    loop {
        // TODO: If we get a packet, continue reading up to some number of frames until
        // the device's buffer is empty.

        {
            let mut packet_buffer = PacketBuffer::new();

            let start_time = std::time::Instant::now();

            let maybe_packet = radio.recv_packet().await?;

            let end_time = std::time::Instant::now();

            println!("{:?}", end_time.duration_since(start_time));

            if let Some(packet) = maybe_packet {
                println!("From: {:02x?}", packet.remote_address());
                println!("{:?}", common::bytes::Bytes::from(packet.data()));
            }
        }

        if let Ok(v) = receiver.try_recv() {
            println!("> {}", v);

            let mut packet_buffer = PacketBuffer::new();
            packet_buffer
                .remote_address_mut()
                .copy_from_slice(network_config.links()[0].address());
            packet_buffer.resize_data(v.len());
            packet_buffer.data_mut().copy_from_slice(v.as_bytes());

            radio.send_packet(&packet_buffer).await?;

            println!("<");
        }

        task::sleep(Duration::from_millis(1000)).await;
    }

    // Two threads:
    // 1. Read

    // device.write_control(pkt, data)

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
