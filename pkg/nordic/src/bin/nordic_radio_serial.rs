// Firmware for running in an NRF52840 which acts as a remote serial/UART port.
//
// - Sending packets to this device will transmit bytes over UART
// - Bytes received over UART by this device will be
//
// This is compatible with the board at `//doc/uplift_desk/board`:
// - RX Input: (MT): P0.31
// - TX Output: (MR): P0.29
// - Deprecated
//   - EEPROM SDA: P0.02
//   - EEPROM SCL: P1.13
//   - EEPROM WP: P1.10
//
// We assume pins are pulled up externally as needed.

/*
cargo run --bin builder -- build //pkg/nordic:nordic_radio_serial --config=//pkg/nordic:nrf52840

cargo run --bin flasher -- built/pkg/nordic/nordic_radio_serial --usb_product_id=1
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
extern crate macros;

use core::arch::asm;

use executor::singleton::Singleton;
use nordic::params::ParamsStorage;
use nordic::uarte::UARTEWriter;
use nordic_proto::packet::PacketBuffer;
use peripherals::storage::BlockStorage;

use nordic::config_storage::NetworkConfigStorage;
use nordic::ecb::ECB;
use nordic::gpio::*;
use nordic::protocol::protocol_usb_thread_fn;
use nordic::radio::Radio;
use nordic::radio_activity_led::setup_radio_activity_leds;
use nordic::radio_socket::{RadioController, RadioControllerThread, RadioSocket};
use nordic::timer::Timer;
use nordic::twim::TWIM;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic_proto::usb_descriptors::*;

static RADIO_SOCKET: RadioSocket = RadioSocket::new();
static PARAMS_STORAGE: Singleton<ParamsStorage> = Singleton::uninit();

const USING_DEV_KIT: bool = true;

define_thread!(
    ForwardingThread,
    forwarding_thread_fn,
    serial: UARTE,
    timer: Timer
);
async fn forwarding_thread_fn(serial: UARTE, mut timer: Timer) {
    enum Event {
        /// A remote packet has been received over the radio.
        RadioPacketAvailable,

        /// The entire serial receive buffer is full and needs to be emptied.
        SerialReceiveBufferFull,

        /// A long time has occured since the last event was received.
        /// This is used to cancel serial reads if the receive buffer was only
        /// partially filled.
        Timeout,
    }

    let (mut serial_reader, mut serial_writer) = serial.split();

    let mut serial_buf = [0u8; 64];

    loop {
        let mut serial_read = serial_reader.begin_read(&mut serial_buf);

        loop {
            let e = race!(
                executor::futures::map(RADIO_SOCKET.wait_for_rx(), |_| Event::RadioPacketAvailable),
                executor::futures::map(serial_read.wait(), |_| Event::SerialReceiveBufferFull),
                // NOTE: It takes ~60ms at 9600 baud to fill up 64 bytes.
                executor::futures::map(timer.wait_ms(200), |_| Event::Timeout),
            )
            .await;

            match e {
                Event::RadioPacketAvailable => {
                    let mut packet = PacketBuffer::new();
                    if !RADIO_SOCKET.dequeue_rx(&mut packet).await {
                        continue;
                    }

                    serial_writer.write(packet.data()).await;
                }
                Event::SerialReceiveBufferFull => {
                    drop(serial_read);
                    send_radio_packet(&serial_buf, &mut serial_writer).await;
                    // Restart the read.
                    break;
                }
                Event::Timeout => {
                    if !serial_read.is_empty() {
                        let n = serial_read.cancel().await;
                        send_radio_packet(&serial_buf[0..n], &mut serial_writer).await;
                        break;
                    }
                }
            }
        }
    }
}

async fn send_radio_packet(data: &[u8], writer: &mut UARTEWriter) {
    let mut packet = PacketBuffer::new();
    packet.set_counter(0);
    packet.resize_data(data.len());
    packet.data_mut().copy_from_slice(data);

    // Send to the first link if configured
    {
        let config_guard = RADIO_SOCKET.lock_network_config().await;
        let config = match config_guard.get() {
            Some(v) => v,
            None => return,
        };

        let link = match config.links().get(0) {
            Some(l) => l,
            None => return,
        };
        packet.remote_address_mut().copy_from_slice(link.address());
    }

    let _ = RADIO_SOCKET.enqueue_tx(&mut packet).await;
}

define_thread!(Main, main_thread_fn);
async fn main_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut serial = UARTE::new(peripherals.uarte0, pins.P0_29, pins.P0_31, 9600);

    let mut timer = Timer::new(peripherals.rtc0);
    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    ForwardingThread::start(serial, timer.clone());

    let params_storage = {
        PARAMS_STORAGE
            .set(ParamsStorage::create(peripherals.nvmc).unwrap())
            .await
    };

    RADIO_SOCKET
        .configure_storage(params_storage)
        .await
        .unwrap();

    let mut radio_controller = RadioController::new(
        &RADIO_SOCKET,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );

    // On the NRF523840 USB Dongle:
    // P0_06 words as mono
    // P0_08 works as red.
    // P0_12 (doesn't work.)
    // P1_09 (doesn't work.)
    let tx_pin = if USING_DEV_KIT {
        gpio.pin(pins.P0_13)
    } else {
        gpio.pin(pins.P0_06)
    };
    let rx_pin = if USING_DEV_KIT {
        gpio.pin(pins.P0_14)
    } else {
        gpio.pin(pins.P0_08)
    };
    setup_radio_activity_leds(tx_pin, rx_pin, timer.clone(), &mut radio_controller);

    RadioControllerThread::start(radio_controller);

    RadioSerialUSBThread::start(
        RADIO_SERIAL_USB_DESCRIPTORS,
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        &RADIO_SOCKET,
        timer.clone(),
    );
}

define_thread!(
    RadioSerialUSBThread,
    protocol_usb_thread_fn,
    descriptors: RadioSerialUSBDescriptors,
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
