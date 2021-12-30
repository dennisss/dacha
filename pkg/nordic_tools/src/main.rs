#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
extern crate usb;

use std::sync::Arc;
use std::time::Duration;

use common::async_std::{channel, task};
use common::errors::*;
use usb::descriptors::SetupPacket;

enum_def_with_unknown!(ProtocolUSBRequestType u8 =>
    Send = 1,
    Receive = 2
);

async fn line_reader(sender: channel::Sender<String>) -> Result<()> {
    loop {
        let mut line = String::new();
        common::async_std::io::stdin().read_line(&mut line).await?;
        sender.send(line).await;
    }
}

async fn run() -> Result<()> {
    let ctx = usb::Context::create()?;
    let mut device = ctx.open_device(0x8888, 0x0001).await?;

    let (sender, receiver) = channel::bounded(1);

    let reader_task = task::spawn(async move {
        println!("{:?}", line_reader(sender).await);
    });

    loop {
        let mut buf = [0u8; 64];

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
                .await;
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
