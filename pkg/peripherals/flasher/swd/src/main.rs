#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::thread::sleep;
use std::time::{Duration, Instant};

use common::errors::*;
use peripherals::gpio::*;
use flasher_swd::*;


#[derive(Args)]
struct Args {
    clk_pin: u32,
    io_pin: u32,
    reset_pin: Option<u32>,
    firmware_path: file::LocalPathBuf,
    target: McuTarget,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let data = file::read(args.firmware_path).await?;

    let gpio = GPIOChip::default_chip()?;

    let mut clk_pin = gpio.pin(args.clk_pin)?;
    let mut io_pin = gpio.pin(args.io_pin)?;

    let mut reset_pin = match args.reset_pin {
        Some(v) => Some(gpio.pin(v)?),
        None => None
    };


    println!("Start");

    let mut swd = SWDProgrammer::create(clk_pin, io_pin)?;

    if let Some(pin) = &mut reset_pin {
        pin.configure(GPIOLineFlags::OUTPUT)?;
        pin.write(false)?;
    }

    println!("Code: {}", swd.probe()?);

    println!("Init debug...");
    swd.init_debug()?;
    println!("=> Done!");

    println!("Flashing...");

    let s = Instant::now();

    swd.flash_chip(McuTarget::STM32F411, &data)?;

    let e = Instant::now();

    println!("Flash {} bytes in {:?}", data.len(), e - s);

    println!("Resetting...");

    swd.reset_core()?;

    if let Some(pin) = &mut reset_pin {
        pin.write(true)?;
    }

    println!("Done!");


    Ok(())
}

