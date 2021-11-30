#![no_std]
#![no_main]
#![feature(lang_items, asm)]

mod registers;

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use crate::registers::*;

type InterruptHandler = unsafe extern "C" fn() -> ();

// TODO: Need code for RAM initialization.

extern "C" {
    static mut _sbss: u32;
    static mut _ebss: u32;

    static mut _sdata: u32;
    static mut _edata: u32;

    static _sidata: u32;
}

unsafe fn zero_bss() {
    let z: u32 = 0;
    for addr in _sbss.._ebss {
        asm!("strb {}, [{}]", in(reg) z, in(reg) addr);
    }
}

#[link_section = ".vector_table.reset_vector"]
#[no_mangle]
pub static RESET_VECTOR: [InterruptHandler; 15 + 20] = [
    entry,             // Reset
    default_interrupt, // NMI
    default_interrupt, // Hard fault
    default_interrupt, // Memory management fauly
    default_interrupt, // Bus fault
    default_interrupt, // Usage fault
    default_interrupt, // reserved 7
    default_interrupt, // reserved 8
    default_interrupt, // reserved 9
    default_interrupt, // reserved 10
    default_interrupt, // SVCall
    default_interrupt, // Reserved for debug
    default_interrupt, // Reserved
    interrupt_pendsv,  // PendSV
    default_interrupt, // Systick
    default_interrupt, // IRQ0
    interrupt_radio,   // IRQ1
    default_interrupt, // IRQ2
    default_interrupt, // IRQ3
    default_interrupt, // IRQ4
    default_interrupt, // IRQ5
    default_interrupt, // IRQ6
    default_interrupt, // IRQ7
    default_interrupt, // IRQ8
    default_interrupt, // IRQ9
    default_interrupt, // IRQ10
    interrupt_rtc,     // IRQ11
    default_interrupt, // IRQ12
    default_interrupt, // IRQ13
    default_interrupt, // IRQ14
    default_interrupt, // IRQ15
    default_interrupt, // IRQ16
    default_interrupt, // IRQ17
    default_interrupt, // IRQ18
    default_interrupt, // IRQ19
];

pub enum IRQn {
    RTC0 = 11,
}

#[no_mangle]
unsafe extern "C" fn default_interrupt() -> () {
    write_volatile(GPIO_P0_OUTSET, 1 << 14);
    loop {
        asm!("nop")
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
*/

/*
Notes:
- Interrupt handlers must be at least 4 clock cycles long to ensure that the interrupt flags are cleared and it doesn't immediately reoccur
*/

unsafe fn init_low_freq_clk() {
    // TODO: Must unsure the clock is stopped before changing the source.
    // ^ But clock can only be stopped if clock is running.

    write_volatile(LFCLKSRC, 1); // Use XTAL
    write_volatile(TASKS_LFCLKSTART, 1); // Start the clock.

    loop {
        let running = read_volatile(LFCLKSTAT) >> 16 == 0b1;
        if running {
            break;
        }
        asm!("nop")
    }
}

/// NOTE: This function assumes that RTC0 is currently stopped.
unsafe fn init_rtc0() {
    write_volatile(RTC0_PRESCALER, 0); // Explictly request a 32.7kHz tick.
    write_volatile(RTC0_TASKS_START, 1);

    // Wait for the first tick to know the RTC has started.
    let initial_count = read_volatile(RTC0_COUNTER);
    while initial_count == read_volatile(RTC0_COUNTER) {
        asm!("nop")
    }
}

static mut INTERRUPT_TRIGGERD: bool = false;

unsafe extern "C" fn interrupt_rtc() -> () {
    // write_volatile(GPIO_P0_OUTSET, 1 << 14);

    // Clear event so that the interrupt doesn't happen again.
    write_volatile(RTC0_EVENTS_COMPARE0, 0);

    asm!("nop");
    asm!("nop");
    asm!("nop");
    asm!("nop");
    INTERRUPT_TRIGGERD = true;
}

unsafe extern "C" fn interrupt_pendsv() -> () {
    asm!("nop");
    asm!("nop");
    asm!("nop");
    asm!("nop");
    INTERRUPT_TRIGGERD = true;
}

unsafe extern "C" fn interrupt_radio() -> () {
    asm!("nop");
    asm!("nop");
    asm!("nop");
    asm!("nop");
}

unsafe fn delay_1s() {
    INTERRUPT_TRIGGERD = false;

    let initial_count = read_volatile(RTC0_COUNTER);
    let target_count = initial_count + (32768 / 4);

    // To produce an interrupt must have bits set:
    // - EVTEN
    // - INTEN
    // - And EVENT must be set.

    // write_volatile(RTC0_EVENTS_TICK, 0);

    write_volatile(RTC0_CC0, target_count);

    write_volatile(RTC0_EVENTS_COMPARE0, 0); // TODO: Is this needed?

    // NOTE: We don't need to set EVTEN
    write_volatile(RTC0_INTENSET, 1 << 16); // Enable interrupt on COMPARE0.

    // write_volatile(RTC0_EVTENSET, 1 << 16 | 1); // Just enable for CC0

    // write_volatile(RTC0_INTENSET, 1 << 16 | 1);

    // write_volatile(RTC0_EVENTS_TICK, 1);

    // // Set PENDSVSET
    // write_volatile(NVIC_ICSR, 1 << 28);
    // asm!("isb");

    while !INTERRUPT_TRIGGERD {
        asm!("nop")
    }

    // while read_volatile(RTC0_COUNTER) < target_count {
    //     asm!("nop")
    // }
}

pub enum RadioState {
    Disabled = 0,
    RxRu = 1,
    RxIdle = 2,
    Rx = 3,
    RxDisable = 4,
    TxRu = 9,
    TxIdle = 10,
    Tx = 11,
    TxDisable = 12,
}

unsafe fn send_packet(message: &[u8]) {
    // TODO: Just have a global buffer given that only one that can be copied at a
    // time anyway.
    let mut data = [0u8; 256];
    data[0] = message.len() as u8;
    data[1..(1 + message.len())].copy_from_slice(message);

    // NOTE: THe POWER register is 1 at boot so we shouldn't need to turn on the
    // peripheral.

    write_volatile(RADIO_PACKETPTR, core::mem::transmute(&data));

    write_volatile(RADIO_FREQUENCY, 0); // Exactly 2400 MHz
    write_volatile(RADIO_TXPOWER, 0x08); // +8 dBm (max power)
    write_volatile(RADIO_MODE, 0); // Nrf_1Mbit

    // 1 LENGTH byte (8 bits). 0 S0, S1 bits. 8-bit preamble.
    write_volatile(RADIO_PCNF0, 8);

    // MAXLEN=255. STATLEN=0, BALEN=2 (so we have 3 byte addresses), little endian
    write_volatile(RADIO_PCNF1, 255 | (2 << 16));

    write_volatile(RADIO_BASE0, 0xAABBCCDD);
    write_volatile(RADIO_PREFIX0, 0xEE);

    write_volatile(RADIO_TXADDRESS, 0); // Trasmit on address 0
    write_volatile(RADIO_RXADDRESSES, 0b1); // Receive from address 0

    // Copies the 802.15.4 mode.
    write_volatile(RADIO_CRCCNF, 0x202);
    write_volatile(RADIO_CRCPOLY, 0x11021);
    write_volatile(RADIO_CRCINIT, 0);

    // Ramp up the radio
    // TODO: If currnetly in the middle of disabling, wait for that to finish before
    // attempting to starramp up.
    // TODO: Also support switching from rx to tx and vice versa.
    write_volatile(RADIO_TASKS_TXEN, 1);

    while read_volatile(RADIO_STATE) != RadioState::TxIdle as u32 {
        asm!("nop");
    }

    write_volatile(RADIO_EVENTS_END, 0);

    // Start transmitting.
    write_volatile(RADIO_TASKS_START, 1);

    while read_volatile(RADIO_EVENTS_END) == 0 {
        asm!("nop");
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

// TODO: Switch back to returning '!'
fn main() -> () {
    unsafe {
        // zero_bss();

        init_low_freq_clk();
        init_rtc0();

        write_volatile(GPIO_P0_DIR, 1 << 14 | 1 << 15);

        // Enable interrupts.
        asm!("cpsie i"); // cpsid to disable

        // Enable external interrupt 11
        write_volatile(NVIC_ISER0 as *mut u32, 1 << 11);

        loop {
            write_volatile(GPIO_P0_OUTCLR, 1 << 14);

            send_packet(b"hello");

            delay_1s();

            write_volatile(GPIO_P0_OUTSET, 1 << 14);

            send_packet(b"world");

            delay_1s();
        }
    }
}
