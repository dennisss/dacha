extern crate common;
extern crate nordic_tools;
extern crate usb;
#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;
use executor::sleep;
use nordic_tools::usb_radio::USBRadio;

#[derive(Args)]
struct Args {
    usb: usb::DeviceSelector,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut usb = USBRadio::find(&args.usb).await?;

    loop {
        let entries = usb.read_log_entries().await?;

        for entry in &entries {
            println!("{}", entry.text());
        }

        if entries.is_empty() {
            executor::sleep(Duration::from_millis(10)).await;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    executor::run(run())?
}
