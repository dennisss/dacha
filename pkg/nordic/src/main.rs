#![feature(
    lang_items,
    asm,
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
extern crate macros;
#[macro_use]
extern crate nordic;

/*
Old binary uses 2763 flash bytes.
Currently we use 3078 flash bytes if we don't count offsets
*/

/*
General workflow:
- Start up
- Read from EEPROM to see if there are initial values of the
    - Increment counter by 100
- Over USB we can do SetNetworkConfig to reconfigure it and set the counter to 0
    - The host should use GetNetworkConfig to not accidentally reset a counter if the keys han't changed.
    - NOTE: Before this happens, we can't run the RadioSocket thread with a bad config.
- Every 100 packets, save the counter to EEPROM before sending it.
- Eventually, every 10 seconds, save to EEPROM the last packet counter received from each remote link


Threads for the serial implementation:
1. The serial reader:
    - Reads data from
    - Buffers data until we see the first 32 bytes or 1ms has passed since the first byte in a batch.
    - Once done, it enqueues a packet to be sent.
    - For now, no ack is really needed.
    -
2. The radio thread waits for new entries in the packet list
    - Technically for doing receiving, it could just pull an arbitrary number of bytes from the buffer.
    - To implement ACK
        - If a response is needed, what's the point of having an ACK in the protocol?

3. The radio thread also sometimes receives packets.
    - These get pushed into th

Alternative strategy:
- Poll for

Scenarios for which we want to optimize:
- Using just

*/

use core::arch::asm;

use executor::singleton::Singleton;
use peripherals::eeprom::EEPROM;
use peripherals::raw::register::{RegisterRead, RegisterWrite};
use peripherals::raw::rtc0::RTC0;
use peripherals::storage::BlockStorage;

use nordic::config_storage::NetworkConfigStorage;
use nordic::ecb::ECB;
use nordic::eeprom::Microchip24XX256;
use nordic::gpio::*;
use nordic::log;
use nordic::log::num_to_slice;
use nordic::protocol::ProtocolUSBThread;
use nordic::radio::Radio;
use nordic::radio_socket::{RadioController, RadioControllerThread, RadioSocket};
use nordic::rng::Rng;
use nordic::temp::Temp;
use nordic::timer::Timer;
use nordic::twim::TWIM;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic::usb::default_handler::USBDeviceDefaultHandler;

/*
Allocator design:
- Current horizon pointer (initialized at end of static RAM)
    - Increment horizon pointer when we want to allocate more memory
    -> do need to

*/

/*
Dev kit LEDs
    P0.13
    P0.14
    P0.15
    P0.16

    active low

Dongle LEDS
    Regular:
        P0.06
    RGB
        P0.08
        P1.09
        P0.12


    active low
*/

const USING_DEV_KIT: bool = true;

static RADIO_SOCKET: RadioSocket = RadioSocket::new();
static BLOCK_STORAGE: Singleton<BlockStorage<Microchip24XX256>> = Singleton::uninit();

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);

    let temp = Temp::new(peripherals.temp);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        log::setup(serial).await;
    }

    log!(b"Started up!\n");

    // WP 3, SCL 4, SDA 28

    {
        // TODO: Set these pins as Input with S0D1 drive strength,

        // addr = 80

        /*
        for i in 0..127 {
            log!(nordic::log::num_to_slice(i as u32).as_ref());
            log!(b"\n");

            match twim.read(i, &mut []).await {
                Ok(_) => {
                    // log!(b"GOOD: ");
                }
                Err(_) => {}
            }
        }
        */

        /*
        if let Err(e) = eeprom.write(0, b"ABCDE").await {
            log!(b"WRITE FAIL\n");
        }

        let mut buf = [0u8; 5];
        if let Err(e) = eeprom.read(0, &mut buf).await {
            log!(b"READ FAIL\n");
        }

        // TODO: Also verify read and write from arbitrary non-zero locations.

        log!(b"READ:\n");
        log!(&buf);
        log!(b"\n");
        */
    }

    // Helper::start(timer.clone());

    //

    // TODO: Which Send/Sync requirements are needed of these arguments?
    // Echo::start(
    //     peripherals.uarte0,
    //     timer.clone(),
    //     temp,
    //     Rng::new(peripherals.rng),
    // );

    // /*
    let radio_socket = &RADIO_SOCKET;

    let radio_controller = RadioController::new(
        radio_socket,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );

    let block_storage = {
        let mut twim = TWIM::new(peripherals.twim0, pins.P0_04, pins.P0_28, 100_000);
        let mut eeprom = Microchip24XX256::new(twim, 0b1010000, gpio.pin(pins.P0_03));
        BLOCK_STORAGE.set(BlockStorage::new(eeprom)).await
    };

    RADIO_SOCKET
        .configure_storage(NetworkConfigStorage::open(block_storage).await.unwrap())
        .await
        .unwrap();

    RadioControllerThread::start(radio_controller);

    ProtocolUSBThread::start(
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        radio_socket,
    );

    log!(b"Ready!\n");

    let mut blink_pin = {
        if USING_DEV_KIT {
            gpio.pin(pins.P0_15)
                .set_direction(PinDirection::Output)
                .write(PinLevel::Low);

            gpio.pin(pins.P0_14)
        } else {
            gpio.pin(pins.P0_06)
        }
    };

    blink_pin.set_direction(PinDirection::Output);

    loop {
        blink_pin.write(PinLevel::Low);
        timer.wait_ms(500).await;

        blink_pin.write(PinLevel::High);
        timer.wait_ms(500).await;
    }
}

// TODO: Configure the voltage supervisor.

// TODO: Switch back to returning '!'

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
