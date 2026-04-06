#![feature(type_alias_impl_trait, impl_trait_in_assoc_type)]
#![no_std]
#![no_main]

/*
cargo run --bin builder -- build //pkg/nordic:nordic_blink --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_blink uf2-dfu --usb_device_id=8888:0001

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

use executor::CriticalSection;
use nordic::gpio::GPIO;
use nordic::protocol::protocol_usb_thread_fn;
use nordic::radio_socket::RadioSocket;
use nordic::rtc::RTC;
use nordic::uarte::UARTE;
use nordic::controller::PeripheralsController;
use nordic::usb::controller::USBDeviceController;
use nordic_wire::usb_descriptors::*;
use peripherals::raw::{PinDirection, PinLevel};
use nordic::gpiote::GPIOPortWaiter;
use nordic::gpio::Resistor;

static RADIO_SOCKET: RadioSocket = RadioSocket::new();

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = RTC::new(peripherals.rtc0);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    /*
    // TODO: Make the radio socket optional.
    BlinkUSBThread::start(
        BLINK_USB_DESCRIPTORS,
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        Some(&RADIO_SOCKET),
        None,
        timer.clone(),
    );

    log!("Started up!");
    */

    let mut blink_pin = {
        // if USING_DEV_KIT {
        // gpio.pin(pins.P0_15)
        //     .set_direction(PinDirection::Output)
        //     .write(PinLevel::Low);

        gpio.pin(pins.P0_06)
        // } else {
        //     gpio.pin(pins.P0_06)
        // }
    };

    blink_pin.reset();

    loop {
        blink_pin.reset();
        blink_pin.set_direction(PinDirection::Output).write(PinLevel::Low);
        timer.wait_ms(1000).await;

        blink_pin.reset();
        timer.wait_ms(1000).await;
    }
}

define_thread!(
    BlinkUSBThread,
    protocol_usb_thread_fn,
    descriptors: BlinkUSBDescriptors,
    usb: USBDeviceController,
    radio_socket: Option<&'static RadioSocket>,
    peripherals_controller: Option<&'static PeripheralsController>,
    rtc: RTC
);


const RAM_SIZE: u32 = 32 * 1024;

entry!(main);
fn main() -> () {

    // TODO: Disable interrupts first.

    reset_stack(nordic::ram::RAM_START_ADDRESS + RAM_SIZE, main_inner);
}

extern "C" fn main_inner() -> ! {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    // TODO: Use standard interrupt system
    let cs = CriticalSection::new();

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::ram::configure_retained_ram(0, RAM_SIZE, &mut peripherals.power);

    // This doesn't seem to help.
    // peripherals.radio.power.write_disabled();

    // This seems to increase power usage.
    peripherals.power.dcdcen.write_enabled();

    peripherals.power.tasks_lowpwr.write_trigger();


    // nordic::clock::reference_hfclk();
    nordic::clock::init_low_freq_clk(
        // TODO: Default to using the RC
        nordic::clock::LowFrequencyClockSource::CrystalOscillator,
        &mut peripherals.clock,
    );

    // TODO: Do this consistently outside of the bootloader.
    peripherals.nvmc.icachecnf.write_with(|v| v.set_cacheen_with(|v| v.set_enabled()));

    Blinker::start();

    // Enable interrupts.
    drop(cs);

    loop {
        unsafe { asm!("wfi") };
    }
}

/// Sets the stack pointer to new_sp and then jumps to 'f'.
fn reset_stack(new_sp: u32, f: extern "C" fn() -> !) -> ! {
    unsafe {
        asm!(
            "mov sp, {sp}",
            "bx {ep}",
            sp = in(reg) new_sp,
            ep = in(reg) f,
            options(noreturn)
        );
    }
}

