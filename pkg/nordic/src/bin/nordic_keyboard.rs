#![feature(type_alias_impl_trait, generic_associated_types)]
#![no_std]
#![no_main]

/*
cargo run --bin builder -- build //pkg/nordic:nordic_keyboard --config=//pkg/nordic:nrf52833

cargo run --bin flasher -- built/pkg/nordic/nordic_keyboard

The correct CRC32: 771496135

Write 00008000 - 0000fdb0
Write 0000fdb0 - 00010bd4

35796 35796
Diff 34812: 0 vs 2
Diff 35412: 0 vs 7

cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52833_bootloader
cargo run --bin flasher -- built/pkg/nordic/nordic_bootloader

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
use core::future::Future;

use executor::mutex::Mutex;
use executor::singleton::Singleton;
use logging::Logger;
use nordic::gpio::{GPIOPin, Resistor, GPIO};
use nordic::protocol::ProtocolUSBHandler;
use nordic::radio_socket::RadioSocket;
use nordic::spi::*;
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic::usb::controller::*;
use nordic::usb::default_handler::USBDeviceDefaultHandler;
use nordic::usb::handler::USBDeviceHandler;
use nordic::usb::handler::USBError;
use nordic::usb::send_buffer::USBDeviceSendBuffer;
use nordic_proto::usb_descriptors::*;
use peripherals::raw::{PinDirection, PinLevel};
use usb::descriptors::{SetupPacket, StandardRequestType};
use usb::hid::*;

/*
    Must Have GET_REPORT, GET_IDLE, SET_IDLE, GET_PROTOCOL, SET_PROTOCOL
- Note: Protocol defaults to non-boot

Sets should also have bmRequestType be 0b00100001
Gets should also have bmRequestType be 0b10100001

wIndex equal to interface

TODO: Must also make sure that interfaces are configured correctly.

NOTE: In the boot protocol, the report should always be 8 bytes long as the key codes should always be equal to the usage ids as the BIOS doesn't read the report descriptor.

TODO: Still need to support GET_REPORT for polling the state.

*/

/*
Test bench pinout:
    29 is neopixel in
    30 is anode
    31 is cathode
*/

/*
Final PCB pinout:

- Y pins:
    - P0.15
    - P0.17
    - P0.20
    - P0.09 (ensure NFC disabled)
    - P0.10 (ensure NFC disabled)
    - P0.03
- X shift registers (use 16 bits)
    - Serial Data: P0.31
    - Register Clock: P0.00
    - Serial Clock: P0.01
    - Only set the bit high for the currently polled column



*/

/*

*/

async fn write_neopixels() {
    /*
    let mut spi = SPIHost::new(
        peripherals.spim0,
        8_000_000,
        pins.P0_29,
        pins.P0_07, // Not connected
        pins.P0_08, // Not connected
        gpio.pin(pins.P0_09), // Not connected
        SPIMode::Mode0,
    );
    let colors = [
        expand_color(0x00FF0000),
        expand_color(0x0001FF01),
        expand_color(0x000000FF),
        expand_color(0x00000000),
        expand_color(0x00010101),
    ];
    */
    // color[10..(10+48)].copy_from_slice(&expand_color(0x00FF0000));
    // color[(10+48)..(10+48+48)].copy_from_slice(&expand_color(0x0000FF00));

    // spi.transfer(&colors[i % colors.len()][..], &mut []).await;
}

/*
Next steps:
- Why are the LEDs not turning off?
- Perform a latency test of the keyboard.
- Consider looking into replacing with an OLED screen
- For the V2 case, add some horizontal steel reinforcement plates so that it doens't bend as much.
*/

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::RCOscillator,
        &mut peripherals.clock,
    );

    nordic::keyboard::Main::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };

    loop {
        unsafe { asm!("nop") };
    }
}
