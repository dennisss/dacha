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

// extern crate alloc;

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
use peripherals::raw::register::{RegisterRead, RegisterWrite};
use peripherals::raw::rtc0::RTC0;

use nordic::ecb::ECB;
use nordic::gpio::*;
use nordic::log;
use nordic::log::num_to_slice;
use nordic::radio::Radio;
use nordic::radio_socket::{RadioController, RadioSocket};
use nordic::rng::Rng;
use nordic::temp::Temp;
use nordic::timer::Timer;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic::usb::default_handler::USBDeviceDefaultHandler;

/*
Allocator design:
- Current horizon pointer (initialized at end of static RAM)
    - Increment horizon pointer when we want to allocate more memory
    -> do need to

*/

// TODO: Split into a separate file (e.g. entry.rs)

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

/*
Notes:
- Interrupt handlers must be at least 4 clock cycles long to ensure that the interrupt flags are cleared and it doesn't immediately reoccur
*/

/*
Example in ext/nRF5_SDK_17.0.2_d674dde/examples/peripheral/radio/receiver/main.c
*/

/*
Waiting for an interrupt:
- Need:
    - EVENTS_* register
    - Need INTEN register / field.
    - Need the interrupt number (for NVIC)
*/

const USING_DEV_KIT: bool = true;

static RADIO_SOCKET: Singleton<RadioSocket> = Singleton::uninit();

/*
define_thread!(
    Monitor,
    monitor_thread_fn,
    uarte0: UARTE0,
    timer: Timer,
    temp: Temp,
    rng: Rng
);
async fn monitor_thread_fn(uarte0: UARTE0, mut timer: Timer, mut temp: Temp, mut rng: Rng) {
    let mut serial = UARTE::new(uarte0);

    let mut buf = [0u8; 256];
    loop {
        timer.wait_ms(2000).await;

        let t = temp.measure().await;

        let mut rand = [0u32; 2];
        rng.generate(&mut rand).await;

        serial.write(b"Temperature is: ").await;
        serial.write(num_to_slice(t).as_ref()).await;
        serial.write(b" | ").await;
        serial.write(num_to_slice(rand[0]).as_ref()).await;
        serial.write(b" | ").await;
        serial.write(num_to_slice(rand[1]).as_ref()).await;
        serial.write(b"\n").await;
    }
}
*/

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);

    let temp = Temp::new(peripherals.temp);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    /*
    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        SerialEcho::start(serial, timer.clone());
    }
    */

    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        log::setup(serial).await;
    }

    log!(b"Started up!\n");

    // Helper::start(timer.clone());

    // log!(b"Done!\n");

    // TODO: Which Send/Sync requirements are needed of these arguments?
    // Echo::start(
    //     peripherals.uarte0,
    //     timer.clone(),
    //     temp,
    //     Rng::new(peripherals.rng),
    // );

    let radio_socket = RADIO_SOCKET.set(RadioSocket::new()).await;

    let radio_controller = RadioController::new(
        radio_socket,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );

    RadioThread::start(radio_controller);

    USBThread::start(
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        radio_socket,
    );

    // if !USING_DEV_KIT {
    //     EchoRadioThread::start(radio_socket, timer.clone());
    // }

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

define_thread!(
    USBThread,
    usb_thread_fn,
    usb: USBDeviceController,
    radio_socket: &'static RadioSocket
);
async fn usb_thread_fn(mut usb: USBDeviceController, radio_socket: &'static RadioSocket) {
    usb.run(nordic::protocol::ProtocolUSBHandler::new(radio_socket))
        .await;
}

define_thread!(
    RadioThread,
    radio_thread_fn,
    radio_controller: RadioController
);
async fn radio_thread_fn(radio_controller: RadioController) {
    radio_controller.run().await;
}

// define_thread!(
//     EchoRadioThread,
//     echo_radio_thread_fn,
//     radio_socket: &'static RadioSocket,
//     timer: Timer
// );
// async fn echo_radio_thread_fn(radio_socket: &'static RadioSocket, timer:
// Timer) {     let mut packet_buffer = PacketBuffer::new();

//     loop {
//         XXX: Must set the
//         if radio_socket.dequeue_rx(&mut packet_buffer).await {
//             log!(b"Echo packet\n");
//             radio_socket.enqueue_tx(&mut packet_buffer).await;
//         }

//         timer.wait_ms(2).await;
//     }
// }

/*
Next steps:
- Nordic things to improve
    - Improve USB handler to return error results in order to handle the Disconnected/Reset events.
    - Implement GPIO (will enable using with EEPROM)
    - Implement global timeouts.
    - Fix interrupt

- Want to extend with encryption.
    - Need
- So want to

- Implement commands:
    - RadioSend
    - RadioReceive
- Us these to implement a remote 'texting' app.
    - though bot
*/

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
