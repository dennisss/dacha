#![no_std]
#![no_main]

#[macro_use]
extern crate nordic;
extern crate peripherals;
#[macro_use]
extern crate common;

use core::arch::asm;

use executor::cond_value::*;
use executor::mutex::*;
use nordic::gpio::GPIO;
use nordic::pins::PeripheralPinHandle;
use nordic::spi::SPIHost;
use peripherals::raw::ppi::PPI;
use peripherals::raw::timer0::TIMER0_REGISTERS;

/*

Number of pins needed:
- Per stepper:
    - 1 step
    - 1 dir
- Shared across steppers
    - 1 interrupt
    - 1 enable
    - SDI
    - SCK
    - CS
    - SDO
- Other pins:
    - 1 Servo PWM
    - 1 Z stop
    - 1 analog in if this is a

Other peripherals:
- Endstops: At least 1 (pull up and wait for ground)
- Servo PWM : x1
- Temperature : x2 for 10K thermistors
- 12V high power

*/

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(&mut peripherals.clock);

    // Main::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };
    loop {
        unsafe { asm!("nop") };
    }
}
