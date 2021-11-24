#![no_std]
#![no_main]
#![feature(lang_items, asm)]

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

type InterruptHandler = unsafe extern "C" fn() -> ();

// TODO: Need code for RAM initialization.

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
    default_interrupt, // PendSV
    default_interrupt, // Systick
    default_interrupt, // IRQ0
    default_interrupt, // IRQ1
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

const GPIO_P0: u32 = 0x50000000;

const GPIO_P0_OUT: *mut u32 = (GPIO_P0 + 0x504) as *mut u32;
const GPIO_P0_OUTSET: *mut u32 = (GPIO_P0 + 0x508) as *mut u32;
const GPIO_P0_OUTCLR: *mut u32 = (GPIO_P0 + 0x50C) as *mut u32;

const GPIO_P0_DIR: *mut u32 = (GPIO_P0 + 0x514) as *mut u32;

const CLOCK: u32 = 0x40000000;
const TASKS_LFCLKSTART: *mut u32 = (CLOCK + 0x008) as *mut u32;
const TASKS_LFCLKSTOP: *mut u32 = (CLOCK + 0x00C) as *mut u32;

const LFCLKRUN: *mut u32 = (CLOCK + 0x414) as *mut u32;
const LFCLKSTAT: *mut u32 = (CLOCK + 0x418) as *mut u32;
const LFCLKSRC: *mut u32 = (CLOCK + 0x518) as *mut u32;

// NOTE: The RTC is initially stopped.
// Peripheral id = 11
const RTC0: u32 = 0x4000B000;
const RTC0_TASKS_START: *mut u32 = (RTC0 + 0x000) as *mut u32;

const RTC0_EVENTS_TICK: *mut u32 = (RTC0 + 0x100) as *mut u32;
const RTC0_EVENTS_COMPARE0: *mut u32 = (RTC0 + 0x140) as *mut u32;

const RTC0_COUNTER: *mut u32 = (RTC0 + 0x504) as *mut u32;

const RTC0_INTENSET: *mut u32 = (RTC0 + 0x304) as *mut u32;
const RTC0_INTENCLR: *mut u32 = (RTC0 + 0x308) as *mut u32;

const RTC0_EVTEN: *mut u32 = (RTC0 + 0x340) as *mut u32;
const RTC0_EVTENSET: *mut u32 = (RTC0 + 0x344) as *mut u32;

const RTC0_CC0: *mut u32 = (RTC0 + 0x540) as *mut u32;

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

unsafe fn init_rtc0() {
    write_volatile(RTC0_TASKS_START, 1);

    // Wait for the first tick to know the RTC has started.
    let initial_count = read_volatile(RTC0_COUNTER);
    while initial_count == read_volatile(RTC0_COUNTER) {
        asm!("nop")
    }
}

static mut INTERRUPT_TRIGGERD: bool = false;

unsafe extern "C" fn interrupt_rtc() -> () {
    write_volatile(GPIO_P0_OUTSET, 1 << 14);

    write_volatile(RTC0_EVENTS_TICK, 0);

    asm!("nop");
    asm!("nop");
    asm!("nop");
    asm!("nop");
    INTERRUPT_TRIGGERD = true;
}

unsafe fn delay_1s() {
    INTERRUPT_TRIGGERD = false;

    let initial_count = read_volatile(RTC0_COUNTER);
    let target_count = initial_count + 32768;

    write_volatile(RTC0_EVENTS_TICK, 0);

    write_volatile(RTC0_CC0, target_count);
    // write_volatile(RTC0_EVENTS_COMPARE0, 0); // TODO: Is this needed?
    write_volatile(RTC0_EVTENSET, 1 << 16 | 1); // Just enable for CC0

    write_volatile(RTC0_INTENSET, 1 << 16 | 1);

    // write_volatile(RTC0_EVENTS_TICK, 1);

    // while !INTERRUPT_TRIGGERD {
    //     asm!("nop")
    // }

    while read_volatile(RTC0_COUNTER) < target_count {
        asm!("nop")
    }
}

// TODO: Switch back to returning '!'
fn main() -> () {
    unsafe {
        init_low_freq_clk();
        init_rtc0();

        write_volatile(GPIO_P0_DIR, 1 << 14 | 1 << 15);

        // Enable interrupts.
        asm!("cpsie i"); // cpsid to disable

        // There are NVIC_ISERi
        // TOD:
        for i in 0..8 {
            write_volatile((0xE000E100u32 + i) as *mut u32, 0xffffffff);
        }

        loop {
            // write_volatile(GPIO_P0_OUTCLR, 1 << 14);

            delay_1s();

            // write_volatile(GPIO_P0_OUTSET, 1 << 14);

            delay_1s();
        }
    }
}
