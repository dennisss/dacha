#![no_std]
#![no_main]
#![feature(lang_items, asm, type_alias_impl_trait, inherent_associated_types)]

#[macro_use]
extern crate executor;
extern crate peripherals;

/*
Old binary uses 2763 flash bytes.
Currently we use 3078 flash bytes if we don't count offsets
*/

mod interrupts;
mod registers;

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

// use crate::registers::*;
use peripherals::clock::CLOCK;
use peripherals::radio::RADIO;
use peripherals::rtc0::RTC0;
use peripherals::{EventState, Interrupt, PinDirection, RegisterRead, RegisterWrite};
// use crate::peripherals::

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

/// NOTE: This function assumes that RTC0 is currently stopped.
fn init_rtc0(rtc0: &mut RTC0) {
    rtc0.prescaler.write(0); // Explictly request a 32.7kHz tick.
    rtc0.tasks_start.write_trigger();

    // Wait for the first tick to know the RTC has started.
    let initial_count = rtc0.counter.read();
    while initial_count == rtc0.counter.read() {
        unsafe { asm!("nop") };
    }
}

/*
Waiting for an interrupt:
- Need:
    - EVENTS_* register
    - Need INTEN register / field.
    - Need the interrupt number (for NVIC)
*/

async fn delay_1s(rtc0: &mut RTC0) {
    let initial_count = rtc0.counter.read();
    let target_count = initial_count + (32768 / 4);

    // To produce an interrupt must have bits set:
    // - EVTEN
    // - INTEN
    // - And EVENT must be set.

    // write_volatile(RTC0_EVENTS_TICK, 0);

    //
    rtc0.cc[0].write_with(|v| v.set_compare(target_count));

    rtc0.events_compare[0].write_notgenerated();

    // Enable interrupt on COMPARE0.
    // NOTE: We don't need to set EVTEN
    rtc0.intenset.write_with(|v| v.set_compare0());

    // write_volatile(RTC0_EVTENSET, 1 << 16 | 1); // Just enable for CC0

    // write_volatile(RTC0_INTENSET, 1 << 16 | 1);

    // write_volatile(RTC0_EVENTS_TICK, 1);

    // // Set PENDSVSET
    // write_volatile(NVIC_ICSR, 1 << 28);
    // asm!("isb");

    crate::interrupts::wait_for_irq(Interrupt::RTC0).await;

    // TODO: Explicitly wait for EVENTS_COMPARE0 to be set?

    // Clear event so that the interrupt doesn't happen again.
    rtc0.events_compare[0].write_notgenerated();

    // TODO: Unset the interrupt.

    // while !INTERRUPT_TRIGGERD {
    //     asm!("nop")
    // }

    // while read_volatile(RTC0_COUNTER) < target_count {
    //     asm!("nop")
    // }
}

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
        crate::interrupts::wait_for_irq(Interrupt::RADIO).await;
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

define_thread!(Blinker, BlinkerThreadFn);
async fn BlinkerThreadFn() {
    let mut peripherals = peripherals::Peripherals::new();

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

    // Enable interrupts.
    unsafe { asm!("cpsie i") }; // cpsid to disable

    loop {
        if USING_DEV_KIT {
            peripherals.p0.outclr.write_with(|v| v.set_pin14());
        } else {
            peripherals.p0.outclr.write_with(|v| v.set_pin6());
        }

        send_packet(&mut peripherals.radio, b"hello", RECEIVING).await;
        if !RECEIVING {
            delay_1s(&mut peripherals.rtc0).await;
        }

        if USING_DEV_KIT {
            peripherals.p0.outset.write_with(|v| v.set_pin14());
        } else {
            peripherals.p0.outset.write_with(|v| v.set_pin6());
        }

        send_packet(&mut peripherals.radio, b"world", RECEIVING).await;
        if !RECEIVING {
            delay_1s(&mut peripherals.rtc0).await;
        }
    }
}

// TODO: Switch back to returning '!'
fn main() -> () {
    unsafe {
        zero_bss();
        init_data();
    }

    let mut peripherals = peripherals::Peripherals::new();

    init_high_freq_clk(&mut peripherals.clock);
    init_low_freq_clk(&mut peripherals.clock);
    init_rtc0(&mut peripherals.rtc0);

    Blinker::start();
    loop {
        unsafe { asm!("nop") };
    }
}
