#[macro_use]
extern crate macros;

use std::fmt::Write;

use common::errors::*;

#[derive(Args)]
struct Args {
    #[arg(default = false)]
    verbose: bool,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let ctx = usb::Context::create()?;

    let devices = ctx.enumerate_devices().await?;

    for dev in devices {
        // TODO: If a manufacturer is not available, look up from a database of known
        // vendors.

        let mut manufacturer = dev.manufacturer().await?.unwrap_or_default();
        if !manufacturer.is_empty() {
            manufacturer = format!("[{}] ", manufacturer);
        }

        let mut product = dev.product().await?.unwrap_or_default();
        if !product.is_empty() {
            product = format!("{} ", product);
        }

        let mut serial = dev.serial().await?.unwrap_or_default();
        if !serial.is_empty() {
            serial = format!("({})", serial);
        }

        let desc = dev.device_descriptor()?;

        println!(
            "Bus {:3}, Dev {:3}, Id {:04x}:{:04x} | {}{}{}",
            dev.bus_num(),
            dev.dev_num(),
            desc.idVendor,
            desc.idProduct,
            manufacturer,
            product,
            serial
        );

        if args.verbose {
            println!("- sysfs path: {}", dev.sysfs_dir().as_str());

            for driver in dev.driver_devices().await? {
                println!("- {:?}: {}", driver.typ, driver.path.as_str());
            }
        }
    }

    Ok(())
}
