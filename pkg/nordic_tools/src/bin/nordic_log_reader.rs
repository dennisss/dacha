extern crate common;
extern crate nordic_tools;
extern crate usb;

use std::time::Duration;

use common::async_std::task;
use common::async_std::task::sleep;
use common::errors::*;
use nordic_tools::usb_radio::USBRadio;

async fn run() -> Result<()> {
    let mut usb = USBRadio::find(Some("any")).await?;

    loop {
        let entries = usb.read_log_entries().await?;

        for entry in &entries {
            println!("{}", entry.text());
        }

        if entries.is_empty() {
            common::async_std::task::sleep(Duration::from_millis(10)).await;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
