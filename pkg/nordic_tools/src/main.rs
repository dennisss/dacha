#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate nordic_proto;
extern crate protobuf;
extern crate usb;

use std::sync::Arc;
use std::time::Duration;

use common::async_std::{channel, task};
use common::errors::*;
use nordic_proto::packet::PacketBuffer;
use nordic_proto::proto::net::*;
use nordic_proto::usb::ProtocolUSBRequestType;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use usb::descriptors::SetupPacket;

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

    // let mut device = ctx.open_device(0x8888, 0x0001).await?;

    let network_config = {
        if args.num == 1 {
            NetworkConfig::parse_text(
                r#"
                address: "\xE7\xE7\xE7\xE7"
                last_packet_counter: 1
        
                links {
                    address: "\xE8\xE8\xE8\xE8"
                    key: "\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
                    iv: "\x00\x00\x00\x00\x00"
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
                    key: "\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
                    iv: "\x00\x00\x00\x00\x00"
                }
                "#,
            )?
        } else {
            return Err(err_msg("Unknown device number"));
        }
    };

    let network_config_proto = network_config.serialize()?;

    println!("Device opened!");

    device.reset()?;

    println!("Device reset!");

    println!("WRITING PROTO: {}", network_config_proto.len());
    println!("{:?}", network_config_proto);

    device
        .write_control(
            SetupPacket {
                bmRequestType: 0b01000000,
                bRequest: ProtocolUSBRequestType::SetNetworkConfig.to_value(),
                wValue: 0,
                wIndex: 0,
                wLength: network_config_proto.len() as u16,
            },
            &network_config_proto,
        )
        .await?;

    {
        let mut read_buffer = [0u8; 256];
        let n = device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolUSBRequestType::GetNetworkConfig.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: read_buffer.len() as u16,
                },
                &mut read_buffer,
            )
            .await?;

        println!("Got Proto of size: {}", n);

        let mut proto_new = NetworkConfig::parse(&read_buffer[0..n])?;
        println!("{:?}", proto_new);
    }

    // let n =

    let (sender, receiver) = channel::bounded(1);

    let reader_task = task::spawn(async move {
        println!("{:?}", line_reader(sender).await);
    });

    loop {
        {
            let mut packet_buffer = PacketBuffer::new();

            println!("R>");
            let n = device
                .read_control(
                    SetupPacket {
                        bmRequestType: 0b11000000,
                        bRequest: ProtocolUSBRequestType::Receive.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: packet_buffer.raw_mut().len() as u16,
                    },
                    packet_buffer.raw_mut(),
                )
                .await?;

            println!("R<");

            if n > 0 {
                println!("Got {}", n);
                println!("{:?}", packet_buffer.data());
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

            device
                .write_control(
                    SetupPacket {
                        bmRequestType: 0b01000000,
                        bRequest: ProtocolUSBRequestType::Send.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: packet_buffer.as_bytes().len() as u16,
                    },
                    packet_buffer.as_bytes(),
                )
                .await?;

            println!("<");
        }

        // Try receiving

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
