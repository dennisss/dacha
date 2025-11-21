// Firmware for running in an NRF52840 connected via USB to a host computer
// running the 'nordic_radio_bridge' binary.
//
// The job of this firmware is to receive requests via USB and convert those to
// radio TX/RX packets.
//
// This can be uploaded to either the official NRF52840 Dev Kit (USING_DEV_KIT =
// true) or the official NRF52840 USB Dongle (USING_DEV_KIT = false).

/*

cargo run --bin builder -- build //pkg/nordic:nordic_bootloader --config=//pkg/nordic:nrf52840_bootloader

cargo run --bin flasher -- built/pkg/nordic/nordic_bootloader blackmagic-swd

cargo run --bin builder -- build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_radio_dongle uf2-dfu --usb_device_id=8888:


TODO: Want a unique USB descriptor serial number per device since we will start having many of these.
*/

#![feature(
    lang_items,
    type_alias_impl_trait,
    impl_trait_in_assoc_type,
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

use executor::singleton::Singleton;
use nordic::controller::PeripheralsController;
use nordic::ecb::ECB;
use nordic::gpio::*;
use nordic::protocol::protocol_usb_thread_fn;
use nordic::pwm::PWMConfig;
use nordic::radio::Radio;
use nordic::radio_activity_led::setup_radio_activity_leds;
use nordic::radio_socket::{RadioController, RadioControllerThread, RadioSocket};
use nordic::rtc::RTC;
use nordic::temp::Temp;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic::idle::idle_loop;
use peripherals_proto::peripherals::PeripheralRequest;
use nordic_wire::usb_descriptors::*;
use protobuf::Message;

static RADIO_SOCKET: RadioSocket = RadioSocket::new();

static PERIPHERALS_CONTROLLER: Singleton<PeripheralsController> = Singleton::uninit();

const USING_DEV_KIT: bool = false;

define_thread!(Main, main_thread_fn);
async fn main_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    log!("Starting up!");

    let mut rtc = RTC::new(peripherals.rtc0);
    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    let peripheral_controller = PERIPHERALS_CONTROLLER
        .set(PeripheralsController::new(
            rtc.clone(),
            peripherals.pwm0,
            peripherals.pwm1,
            peripherals.pwm2,
            peripherals.pwm3,
            peripherals.spim0,
            peripherals.spim1,
            peripherals.spim2,
            peripherals.spim3,
            peripherals.gpiote,
            peripherals.temp,
            peripherals.uarte0,
            peripherals.timer0,
            peripherals.timer1,
            peripherals.ppi,
            peripherals.saadc,
        ))
        .await;

    peripheral_controller.start();

    /*
    // Hardcoded settings for the HL15 fan controller board.
    // TODO: Stop hard coding this.
    {
        let pwm_pins: &'static [u32] = &[12, 26, 32 + 8, 24];

        let tachometer_pins: &'static [u32] = &[11, 4, 7, 28, 14, 16, 25, 20];

        for (i, pin) in pwm_pins.iter().cloned().enumerate() {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(i as u32);
            req.configure_pwm_mut();
            req.configure_pwm_mut().set_pin(pin);
            req.configure_pwm_mut().set_inverted(true);
            req.configure_pwm_mut()
                .set_default_value(((u16::MAX as f32) * 0.5) as u32);
            req.configure_pwm_mut().set_frequency(25000 as u32);
            req.configure_pwm_mut().set_timeout_millis(10000 as u32);
            peripheral_controller.execute(&req).await;
        }

        for (i, pin) in tachometer_pins.iter().cloned().enumerate() {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index((pwm_pins.len() + i) as u32);
            req.configure_gpio_mut().set_is_input(true);
            req.configure_gpio_mut().set_pin(pin);
            req.configure_gpio_mut().set_pull_up(true);
            peripheral_controller.execute(&req).await;
        }

        {
            let mut req = PeripheralRequest::default();
            req.set_peripheral_index(0 as u32);
            req.finalize_config_mut();
            peripheral_controller.execute(&req).await;
        }
    }
    */

    let mut radio_controller = RadioController::new(
        &RADIO_SOCKET,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );

    /*
    // TODO: Make this more scalable.

    // TODO: Prevent these from being used by the PeripheralsController?
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
    setup_radio_activity_leds(tx_pin, rx_pin, rtc.clone(), &mut radio_controller);
    */

    RadioControllerThread::start(radio_controller);

    RadioDongleUSBThread::start(
        RADIO_DONGLE_USB_DESCRIPTORS,
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        &RADIO_SOCKET,
        Some(peripheral_controller),
        rtc.clone(),
    );
}

define_thread!(
    RadioDongleUSBThread,
    protocol_usb_thread_fn,
    descriptors: RadioDongleUSBDescriptors,
    usb: USBDeviceController,
    radio_socket: &'static RadioSocket,
    peripherals_controller: Option<&'static PeripheralsController>,
    rtc: RTC
);

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);

    // TODO: Switch back to external once I get rid of boards that use P0.00/P0.01
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::RCOscillator,
        &mut peripherals.clock,
    );

    nordic::enable_fpu();

    Main::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };
    idle_loop()
}
