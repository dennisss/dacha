#![feature(type_alias_impl_trait)]
#![no_std]
#![no_main]

/*
cargo run --bin builder -- build //pkg/nordic:nordic_blink --config=//pkg/nordic:nrf52840

cargo run --bin flasher

cargo run --bin nordic_log_reader
*/

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
#[macro_use]
extern crate logging;

use core::arch::asm;

use nordic::gpio::GPIO;
use nordic::protocol::ProtocolUSBThread;
use nordic::radio_socket::RadioSocket;
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use peripherals::raw::{PinDirection, PinLevel};

static RADIO_SOCKET: RadioSocket = RadioSocket::new();

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    // {
    //     let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31,
    // 115200);     log::setup(serial).await;
    // }

    ProtocolUSBThread::start(
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        &RADIO_SOCKET,
        timer.clone(),
    );

    log!("Started up!");

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

    let mut counter: u32 = 0;

    loop {
        blink_pin.write(PinLevel::Low);
        timer.wait_ms(500).await;

        blink_pin.write(PinLevel::High);
        timer.wait_ms(500).await;

        log!(counter + 20);
        counter += 1;
    }
}

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    //

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::CrystalOscillator,
        &mut peripherals.clock,
    );

    Blinker::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };

    loop {
        unsafe { asm!("nop") };
    }
}
