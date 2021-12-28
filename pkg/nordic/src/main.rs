#![no_std]
#![no_main]
#![feature(
    lang_items,
    asm,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler
)]

extern crate alloc;

#[macro_use]
extern crate executor;
extern crate peripherals;
#[macro_use]
extern crate common;
extern crate crypto;
#[macro_use]
extern crate macros;

/*
Old binary uses 2763 flash bytes.
Currently we use 3078 flash bytes if we don't count offsets
*/

mod ccm;
mod ecb;
mod eeprom;
mod gpio;
mod log;
mod pins;
mod proto;
mod rng;
mod storage;
mod temp;
mod timer;
mod twim;
mod uarte;
mod usb;
mod usb_descriptor;

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use peripherals::raw::clock::CLOCK;
use peripherals::raw::radio::RADIO;
use peripherals::raw::rtc0::RTC0;
use peripherals::raw::{EventState, Interrupt, PinDirection, RegisterRead, RegisterWrite};
// use crate::peripherals::
use peripherals::raw::uarte0::UARTE0;

use crate::rng::Rng;
use crate::temp::Temp;
use crate::timer::Timer;
use crate::uarte::UARTE;
use crate::usb::USBDevice;

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

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

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

async fn send_packet(radio: &mut RADIO, message: &[u8], receiving: bool) {
    // TODO: Just have a global buffer given that only one that can be copied at a
    // time anyway.
    let mut data = [0u8; 256];
    data[0] = message.len() as u8;
    data[1..(1 + message.len())].copy_from_slice(message);

    // NOTE: THe POWER register is 1 at boot so we shouldn't need to turn on the
    // peripheral.

    radio
        .packetptr
        .write(unsafe { core::mem::transmute(&data) });

    radio.frequency.write_with(|v| v.set_frequency(5)); // 0 // Exactly 2400 MHz
    radio.txpower.write_0dbm(); // TODO: +8 dBm (max power)
    radio.mode.write_nrf_1mbit();

    // 1 LENGTH byte (8 bits). 0 S0, S1 bits. 8-bit preamble.
    radio
        .pcnf0
        .write_with(|v| v.set_lflen(8).set_plen_with(|v| v.set_8bit()));
    // write_volatile(RADIO_PCNF0, 8);

    // MAXLEN=255. STATLEN=0, BALEN=2 (so we have 3 byte addresses), little endian
    radio.pcnf1.write_with(|v| v.set_maxlen(255).set_balen(2));

    radio.base0.write(0xAABBCCDD);
    radio.prefix0.write_with(|v| v.set_ap0(0xEE));

    radio.txaddress.write(0); // Transmit on address 0

    // Receive from address 0
    radio
        .rxaddresses
        .write_with(|v| v.set_addr0_with(|v| v.set_enabled()));

    // Copies the 802.15.4 mode.
    radio.crccnf.write_with(|v| {
        v.set_len_with(|v| v.set_two())
            .set_skipaddr_with(|v| v.set_ieee802154())
    });
    radio.crcpoly.write(0x11021);
    radio.crcinit.write(0);

    if receiving {
        // data[0] = 0;

        radio.tasks_rxen.write_trigger();
        while !radio.state.read().is_rxidle() {
            unsafe { asm!("nop") };
        }

        radio.events_end.write_notgenerated();

        // Start receiving
        radio.tasks_start.write_trigger();

        while !radio.state.read().is_rx() {
            unsafe { asm!("nop") };
        }

        // write_volatile(RADIO_TASKS_STOP, 1);

        while radio.state.read().is_rx() && radio.events_end.read().is_notgenerated() {
            unsafe { asm!("nop") };
        }

        return;
    }

    radio.events_ready.write_notgenerated();
    radio.intenset.write_with(|v| v.set_ready());

    // Ramp up the radio
    // TODO: If currnetly in the middle of disabling, wait for that to finish before
    // attempting to starramp up.
    // TODO: Also support switching from rx to tx and vice versa.
    radio.tasks_txen.write_trigger();

    while !radio.state.read().is_txidle() && radio.events_ready.read().is_notgenerated() {
        executor::interrupts::wait_for_irq(Interrupt::RADIO).await;
    }
    radio.events_ready.write_notgenerated();

    assert!(radio.state.read().is_txidle());

    radio.events_end.write_notgenerated();

    // Start transmitting.
    radio.tasks_start.write_trigger();

    while radio.events_end.read().is_notgenerated() {
        unsafe { asm!("nop") };
    }

    // Mode: Nrf_250Kbit

    // Use STATE

    // TASKS_TXEN = 1 to start transmit mode

    // Wait for started

    // TASKS_START

    /*
    If receiving check RXMATCH for address and CRCSTATUS for whether it was googd
    */
}

const USING_DEV_KIT: bool = true;
const RECEIVING: bool = false;

static mut HELLO: [u8; 5] = [4, 1, 2, 3, 4];

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

    // f(&buf[(buf.len() - num_digits)..]);
}

define_thread!(
    Echo,
    echo_thread_fn,
    uarte0: UARTE0,
    timer: Timer,
    temp: Temp,
    rng: Rng
);
async fn echo_thread_fn(uarte0: UARTE0, mut timer: Timer, mut temp: Temp, mut rng: Rng) {
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

        // b"hello world this is long\n"
        // serial.write(b"Hi there\n").await;

        // serial.read_exact(&mut buf[0..8]).await;
    }
}

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

    // TODO: Which Send/Sync requirements are needed of these arguments?
    // Echo::start(
    //     peripherals.uarte0,
    //     timer.clone(),
    //     temp,
    //     Rng::new(peripherals.rng),
    // );

    USBThread::start(USBDevice::new(peripherals.usbd, peripherals.power));

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

        send_packet(&mut peripherals.radio, b"hello", RECEIVING).await;
        if !RECEIVING {
            timer.wait_ms(100).await;
        }

        if USING_DEV_KIT {
            peripherals.p0.outset.write_with(|v| v.set_pin14());
        } else {
            peripherals.p0.outset.write_with(|v| v.set_pin6());
        }

        send_packet(&mut peripherals.radio, b"world", RECEIVING).await;
        if !RECEIVING {
            timer.wait_ms(100).await;
        }
    }
}

define_thread!(USBThread, usb_thread_fn, usb: USBDevice);
async fn usb_thread_fn(mut usb: USBDevice) {
    usb.run().await;
}
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
