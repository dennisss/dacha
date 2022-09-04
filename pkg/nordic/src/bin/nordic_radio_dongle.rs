// Firmware for running in an NRF52840 connected via USB to a host computer
// running the 'nordic_radio_bridge' binary.
//
// The job of this firmware is to receive requests via USB and convert those to
// radio TX/RX packets.
//
// This can be uploaded to either the official NRF52840 Dev Kit (USING_DEV_KIT =
// true) or the official NRF52840 USB Dongle (USING_DEV_KIT = false).

/*
cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle

openocd -f board/nordic_nrf52_dk.cfg -c init -c "reset init" -c halt -c "nrf5 mass_erase" -c "program built/pkg/nordic/nordic_radio_dongle verify" -c reset -c exit
*/

#![feature(
    lang_items,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types
)]
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
#[macro_use]
extern crate logging;

use core::arch::asm;

use nordic::ecb::ECB;
use nordic::gpio::*;
use nordic::protocol::protocol_usb_thread_fn;
use nordic::radio::Radio;
use nordic::radio_activity_led::setup_radio_activity_leds;
use nordic::radio_socket::{RadioController, RadioControllerThread, RadioSocket};
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic_proto::usb_descriptors::*;

static RADIO_SOCKET: RadioSocket = RadioSocket::new();

const USING_DEV_KIT: bool = true;

define_thread!(Main, main_thread_fn);
async fn main_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    log!("Starting up!");

    let mut timer = Timer::new(peripherals.rtc0);
    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    let mut radio_controller = RadioController::new(
        &RADIO_SOCKET,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );

    let tx_pin = if USING_DEV_KIT {
        gpio.pin(pins.P0_13)
    } else {
        gpio.pin(pins.P0_12)
    };
    let rx_pin = if USING_DEV_KIT {
        gpio.pin(pins.P0_14)
    } else {
        gpio.pin(pins.P1_09)
    };
    setup_radio_activity_leds(tx_pin, rx_pin, timer.clone(), &mut radio_controller);

    RadioControllerThread::start(radio_controller);

    RadioDongleUSBThread::start(
        RADIO_DONGLE_USB_DESCRIPTORS,
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        &RADIO_SOCKET,
        timer.clone(),
    );
}

define_thread!(
    RadioDongleUSBThread,
    protocol_usb_thread_fn,
    descriptors: RadioDongleUSBDescriptors,
    usb: USBDeviceController,
    radio_socket: &'static RadioSocket,
    timer: Timer
);

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::CrystalOscillator,
        &mut peripherals.clock,
    );

    Main::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };
    loop {
        unsafe { asm!("nop") };
    }
}
