#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};
use std::time::Duration;

use base_error::*;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use peripherals_service::config::*;
use peripherals_service::device::*;

/*
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:

cargo run --bin peripheral_tester
*/

#[executor_main]
async fn main() -> Result<()> {

    let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

    let config = configs.remove(&"nrf52840_dongle")
        .ok_or_else(|| err_msg("No config with the given name"))?;

    let (mut device, _) = PeripheralsDevice::create(&config).await?;

    loop {
        let start = Instant::now();
        let t = device.get_clock_time().await?;
        let end = Instant::now();

        println!("{} : {:?}", t, end - start);

        executor::sleep(Duration::from_millis(1000)).await?;
    }

    loop {
        device.gpio_write("led", false).await?;
        executor::sleep(Duration::from_millis(1000)).await?;

        device.gpio_write("led", true).await?;
        executor::sleep(Duration::from_millis(1000)).await?;
    }


    Ok(())
}
