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

async fn line_reader(sender: channel::Sender<String>) -> Result<()> {
    loop {
        let mut line = String::new();
        common::async_std::io::stdin().read_line(&mut line).await?;
        sender.send(line).await;
    }
}

async fn run() -> Result<()> {
    let network_config = NetworkConfig::parse_text(
        r#"
        address: "\xE7\xE7\xE7\xE7"
        last_packet_counter: 1

        links {
            address: "\xE8\xE8\xE8\xE8"
            key: "\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
            iv: "\x00\x00\x00\x00\x00"
        }
        "#,
    )?;

    let network_config_proto = network_config.serialize()?;

    let ctx = usb::Context::create()?;
    let mut device = ctx.open_device(0x8888, 0x0001).await?;

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
    }

    // let n =

    let (sender, receiver) = channel::bounded(1);

    let reader_task = task::spawn(async move {
        println!("{:?}", line_reader(sender).await);
    });

    loop {
        let mut buf = [0u8; 64];

        println!("R>");
        let n = device
            .read_control(
                SetupPacket {
                    bmRequestType: 0b11000000,
                    bRequest: ProtocolUSBRequestType::Receive.to_value(),
                    wValue: 0,
                    wIndex: 0,
                    wLength: buf.len() as u16,
                },
                &mut buf,
            )
            .await?;

        println!("R<");

        if n > 0 {
            println!("Got {}", n);
        }

        if let Ok(v) = receiver.try_recv() {
            println!("> {}", v);

            device
                .write_control(
                    SetupPacket {
                        bmRequestType: 0b01000000,
                        bRequest: ProtocolUSBRequestType::Send.to_value(),
                        wValue: 0,
                        wIndex: 0,
                        wLength: v.as_bytes().len() as u16,
                    },
                    v.as_bytes(),
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
