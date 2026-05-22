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

    println!("Halting core...");
    swd.halt_core()?;

    // Release reset to let the chip exit hardware reset, but it will 
    // remain halted because we just set the C_HALT flag via SWD!
    if let Some(pin) = &mut reset_pin {
        pin.write(true)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    println!("Flashing...");

    let s = Instant::now();

    swd.flash_chip(args.target, &data)?;

    let e = Instant::now();

    println!("Flash {} bytes in {:?}", data.len(), e - s);

    println!("Verifying...");

    let s_verify = Instant::now();
    swd.verify_flash(&data)?;
    let e_verify = Instant::now();

    println!("Verified {} bytes in {:?}", data.len(), e_verify - s_verify);

    println!("Resetting...");

    swd.reset_core()?;
    
    println!("Releasing pins...");
    swd.release_pins()?;
    
    if let Some(pin) = &mut reset_pin {
        pin.configure(GPIOLineFlags::INPUT)?;
    }

    println!("Done!");


    Ok(())
}

