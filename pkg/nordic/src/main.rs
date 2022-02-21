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
extern crate crypto;
#[macro_use]
extern crate macros;
extern crate nordic_proto;

#[cfg(feature = "alloc")]
pub mod allocator;
pub mod ecb;
pub mod eeprom;
pub mod gpio;
pub mod log;
pub mod pins;
pub mod protocol;
pub mod radio;
pub mod radio_socket;
pub mod rng;
#[cfg(feature = "alloc")]
pub mod storage;
pub mod temp;
pub mod timer;
pub mod twim;
pub mod uarte;
pub mod usb;

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
use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use executor::singleton::Singleton;
use peripherals::raw::clock::CLOCK;
use peripherals::raw::rtc0::RTC0;
use peripherals::raw::uarte0::UARTE0;
use peripherals::raw::{EventState, Interrupt, PinDirection, RegisterRead, RegisterWrite};

use crate::ecb::ECB;
// use crate::log;
// use crate::num_to_slice;
use crate::radio::Radio;
use crate::radio_socket::{RadioController, RadioSocket};
use crate::rng::Rng;
use crate::temp::Temp;
use crate::timer::Timer;
use crate::uarte::UARTE;
use crate::usb::controller::USBDeviceController;
use crate::usb::default_handler::USBDeviceDefaultHandler;

extern "C" {
    static mut _sbss: u32;
    static mut _ebss: u32;

    static mut _sdata: u32;
    static mut _edata: u32;

    static _sidata: u32;
}

#[inline(never)]
unsafe fn zero_bss() {
    let start = core::mem::transmute::<_, u32>(&_sbss);
    let end = core::mem::transmute::<_, u32>(&_ebss);

    let z: u32 = 0;
    for addr in start..end {
        asm!("strb {}, [{}]", in(reg) z, in(reg) addr);
    }
}

#[inline(never)]
unsafe fn init_data() {
    let in_start = core::mem::transmute::<_, u32>(&_sidata);
    let out_start = core::mem::transmute::<_, u32>(&_sdata);
    let out_end = core::mem::transmute::<_, u32>(&_edata);

    for i in 0..(out_end - out_start) {
        let z = read_volatile((in_start + i) as *mut u8);
        let addr = out_start + i;

        asm!("strb {}, [{}]", in(reg) z, in(reg) addr);
    }
}

/*
Allocator design:
- Current horizon pointer (initialized at end of static RAM)
    - Increment horizon pointer when we want to allocate more memory
    -> do need to

*/

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

// TODO: Split into a separate file (e.g. entry.rs)
#[no_mangle]
pub extern "C" fn entry() -> () {
    main()
}

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

fn init_high_freq_clk(clock: &mut CLOCK) {
    // Init HFXO (must be started to use RADIO)
    clock.events_hfclkstarted.write_notgenerated();
    clock.tasks_hfclkstart.write_trigger();

    while clock.events_hfclkstarted.read().is_notgenerated() {
        unsafe { asm!("nop") };
    }
}

fn init_low_freq_clk(clock: &mut CLOCK) {
    // NOTE: This must be initialized to use the RTCs.

    // TODO: Must unsure the clock is stopped before changing the source.
    // ^ But clock can only be stopped if clock is running.

    // Use XTAL
    clock
        .lfclksrc
        .write_with(|v| v.set_src_with(|v| v.set_xtal()));

    // Start the clock.
    clock.tasks_lfclkstart.write_trigger();

    while clock.lfclkstat.read().state().is_notrunning() {
        unsafe { asm!("nop") };
    }
}

/*
Implementing a global sleeper:
- Take as input RTC0
- Each timeout knows it's start and count.
- Each one will simply set CC[0]
- Just have INTEN always enabled given that no one cares.
*/

/*
Waiting for an interrupt:
- Need:
    - EVENTS_* register
    - Need INTEN register / field.
    - Need the interrupt number (for NVIC)
*/

const USING_DEV_KIT: bool = true;

pub struct NumberSlice {
    buf: [u8; 10],
    len: usize,
}

impl AsRef<[u8]> for NumberSlice {
    fn as_ref(&self) -> &[u8] {
        &self.buf[(self.buf.len() - self.len)..]
    }
}

pub fn num_to_slice(mut num: u32) -> NumberSlice {
    // A u32 has a maximum length of 10 base-10 digits
    let mut buf: [u8; 10] = [0; 10];
    let mut num_digits = 0;
    while num > 0 {
        // TODO: perform this as one operation?
        let r = (num % 10) as u8;
        num /= 10;

        num_digits += 1;

        buf[buf.len() - num_digits] = ('0' as u8) + r;
    }

    if num_digits == 0 {
        num_digits = 1;
        buf[buf.len() - 1] = '0' as u8;
    }

    NumberSlice {
        buf,
        len: num_digits,
    }
}

static RADIO_SOCKET: Singleton<RadioSocket> = Singleton::uninit();

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

// use executor::interrupts::{trigger_pendsv, wait_for_pendsv};

// define_thread!(Helper, helper_fn, timer: Timer);
// async fn helper_fn(mut timer: Timer) {
//     timer.wait_ms(100).await;
//     trigger_pendsv();
//     log!(b"Triggered!\n");
// }

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();

    let mut timer = Timer::new(peripherals.rtc0);

    let temp = Temp::new(peripherals.temp);

    {
        let mut serial = UARTE::new(peripherals.uarte0);
        log::setup(serial).await;
    }

    log!(b"Started up!\n");

    // Helper::start(timer.clone());

    // wait_for_pendsv().await;

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

    // peripherals.p0.dirset.write_with(|v| v.set_pin30());
    // peripherals.p0.outset.write_with(|v| v.set_pin30());

    if USING_DEV_KIT {
        peripherals.p0.dir.write_with(|v| {
            v.set_pin14(PinDirection::Output)
                .set_pin15(PinDirection::Output)
        });
    } else {
        peripherals
            .p0
            .dir
            .write_with(|v| v.set_pin6(PinDirection::Output));
    }

    loop {
        if USING_DEV_KIT {
            peripherals.p0.outclr.write_with(|v| v.set_pin14());
        } else {
            peripherals.p0.outclr.write_with(|v| v.set_pin6());
        }

        timer.wait_ms(500).await;

        if USING_DEV_KIT {
            peripherals.p0.outset.write_with(|v| v.set_pin14());
        } else {
            peripherals.p0.outset.write_with(|v| v.set_pin6());
        }

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
    usb.run(crate::protocol::ProtocolUSBHandler::new(radio_socket))
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
//         if radio_socket.dequeue_rx(&mut packet_buffer).await {
//             log!(b"Echo packet\n");
//             radio_socket.enqueue_tx(&mut packet_buffer).await;
//         }

//         timer.wait_ms(2).await;
//     }
// }

/*
Echo thread:
- Receive
*/

/*
Next steps:
- Nordic things to improve
    - Improve USB handler to return error results in order to handle the Disconnected/Reset events.
    - Implement GPIO (will enable using with EEPROM)
    - Implement global timeouts.
    - Fix interrupt

- We now have the basic 'texting' app.


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
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    unsafe {
        zero_bss();
        init_data();
    }

    let mut peripherals = peripherals::raw::Peripherals::new();

    init_high_freq_clk(&mut peripherals.clock);
    init_low_freq_clk(&mut peripherals.clock);

    Blinker::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };

    loop {
        unsafe { asm!("nop") };
    }
}
