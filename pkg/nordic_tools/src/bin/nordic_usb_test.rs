extern crate common;
extern crate usb;
#[macro_use]
extern crate macros;

use std::fmt::Write;
use std::thread::sleep;
use std::time::Duration;

use common::errors::*;
use usb::descriptor_iter::DescriptorIter;
use usb::DescriptorSet;

#[executor_main]
async fn main() -> Result<()> {
    let desc = nordic_proto::usb_descriptors::PROTOCOL_USB_DESCRIPTORS;

    let iter = DescriptorIter::new(desc.config_bytes(0).unwrap());

    for d in iter {
        let d = d?;
        println!("{:#?}", d);
    }

    return Ok(());

    let ctx = usb::Context::create()?;

    // TODO: These should always have timeouts.

    let mut dev = ctx.open_device(0x8888, 0x0001).await?;

    println!("Opened");

    dev.reset()?;

    println!("Write");
    dev.write_interrupt(0x02, b"ABC").await?;
    sleep(Duration::from_secs(1));

    println!("Write 2");
    dev.write_interrupt(0x02, b"Oranges!").await?;

    let mut data = vec![];
    data.resize(64, 0);

    println!("Read");
    let n = dev.read_interrupt(0x81, &mut data).await?;

    println!("N: {}", n);
    println!("{:?}", common::bytes::Bytes::from(&data[0..n]));

    Ok(())
}
