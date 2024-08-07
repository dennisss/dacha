extern crate common;
extern crate usb;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    let ctx = usb::Context::create()?;

    let devices = ctx.enumerate_devices().await?;

    for dev in devices {
        let desc = dev.device_descriptor()?;
        if desc.idVendor == 0x0fd9 {
            let mut device = dev.open().await?;

            let lang_ids = device.read_languages().await?;
            println!("Languages: {:04x?}", lang_ids);

            println!(
                "Manufacturer: {}",
                device.read_string(desc.iManufacturer, lang_ids[0]).await?
            );
            println!(
                "Product: {}",
                device.read_string(desc.iProduct, lang_ids[0]).await?
            );

            device.close()?;

            println!("DONE!");
            break;
        }
    }

    drop(ctx);
    executor::sleep(std::time::Duration::from_millis(2000)).await;

    Ok(())
}
