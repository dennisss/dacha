#![feature(type_alias_impl_trait)]
#![no_std]
#![no_main]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate executor;
extern crate peripherals;
#[macro_use]
extern crate common;
#[macro_use]
extern crate nordic;

use core::arch::asm;

use nordic::gpio::GPIO;
use nordic::log;
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use peripherals::raw::{PinDirection, PinLevel};

//
//
//
//
//

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        log::setup(serial).await;
    }

    log!(b"Started up!\n");

    let mut blink_pin = {
        // if USING_DEV_KIT {
        gpio.pin(pins.P0_15)
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);

        gpio.pin(pins.P0_14)
        // } else {
        //     gpio.pin(pins.P0_06)
        // }
    };

    blink_pin.set_direction(PinDirection::Output);

    loop {
        blink_pin.write(PinLevel::Low);
        timer.wait_ms(500).await;

        blink_pin.write(PinLevel::High);
        timer.wait_ms(500).await;
    }
}

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(&mut peripherals.clock);

    Blinker::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };

    loop {
        unsafe { asm!("nop") };
    }
}
