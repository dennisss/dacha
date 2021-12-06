#![no_std]
#![no_main]
#![feature(lang_items, asm, type_alias_impl_trait)]

#[macro_use]
extern crate executor;

mod interrupts;
mod registers;

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

use crate::registers::*;

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

unsafe fn init_high_freq_clk() {
    // Init HFXO (must be started to use RADIO)
    write_volatile(EVENTS_HFCLKSTARTED, 0);
    write_volatile(TASKS_HFCLKSTART, 1);
    while read_volatile(EVENTS_HFCLKSTARTED) == 0 {
        asm!("nop")
    }
}

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

async unsafe fn delay_1s() {
    let initial_count = read_volatile(RTC0_COUNTER);
    let target_count = initial_count + (32768 / 4);

    // To produce an interrupt must have bits set:
    // - EVTEN
    // - INTEN
    // - And EVENT must be set.

    // write_volatile(RTC0_EVENTS_TICK, 0);

    //
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

    crate::interrupts::wait_for_irq(crate::interrupts::IRQn::RTC0).await;

    // Clear event so that the interrupt doesn't happen again.
    write_volatile(RTC0_EVENTS_COMPARE0, 0);

    // TODO: Unset the interrupt.

    // while !INTERRUPT_TRIGGERD {
    //     asm!("nop")
    // }

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

async unsafe fn send_packet(message: &[u8], receiving: bool) {
    // TODO: Just have a global buffer given that only one that can be copied at a
    // time anyway.
    let mut data = [0u8; 256];
    data[0] = message.len() as u8;
    data[1..(1 + message.len())].copy_from_slice(message);

    // NOTE: THe POWER register is 1 at boot so we shouldn't need to turn on the
    // peripheral.

    write_volatile(RADIO_PACKETPTR, core::mem::transmute(&data));

    write_volatile(RADIO_FREQUENCY, 5); // 0 // Exactly 2400 MHz
    write_volatile(RADIO_TXPOWER, 0); // 8 // +8 dBm (max power)
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

    if receiving {
        // data[0] = 0;

        write_volatile(RADIO_TASKS_RXEN, 1);
        while read_volatile(RADIO_STATE) != RadioState::RxIdle as u32 {
            asm!("nop");
        }

        write_volatile(RADIO_EVENTS_END, 0);

        // Start receiving
        write_volatile(RADIO_TASKS_START, 1);

        while read_volatile(RADIO_STATE) != RadioState::Rx as u32 {
            asm!("nop");
        }

        // write_volatile(RADIO_TASKS_STOP, 1);

        while read_volatile(RADIO_STATE) == RadioState::Rx as u32
            && read_volatile(RADIO_EVENTS_END) == 0
        {
            asm!("nop");
        }

        return;
    }

    write_volatile(RADIO_EVENTS_READY, 0);
    write_volatile(RADIO_INTENSET, 1 << 0); // Enable interrupt for READY event.

    // Ramp up the radio
    // TODO: If currnetly in the middle of disabling, wait for that to finish before
    // attempting to starramp up.
    // TODO: Also support switching from rx to tx and vice versa.
    write_volatile(RADIO_TASKS_TXEN, 1);

    while read_volatile(RADIO_EVENTS_READY) == 0 {
        crate::interrupts::wait_for_irq(crate::interrupts::IRQn::RADIO).await;
    }
    write_volatile(RADIO_EVENTS_READY, 0);
    assert!(read_volatile(RADIO_STATE) == RadioState::TxIdle as u32);

    write_volatile(RADIO_EVENTS_READY, 0);

    write_volatile(RADIO_EVENTS_END, 0);

    // Start transmitting.
    write_volatile(RADIO_TASKS_START, 1);

    // EVENTS_END

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

const USING_DEV_KIT: bool = true;
const RECEIVING: bool = false;

static mut HELLO: [u8; 5] = [4, 1, 2, 3, 4];

define_thread!(Blinker, BlinkerThreadFn);
async fn BlinkerThreadFn() {
    unsafe {
        if USING_DEV_KIT {
            write_volatile(GPIO_P0_DIR, 1 << 14 | 1 << 15);
        } else {
            write_volatile(GPIO_P0_DIR, 1 << 6);
        }

        // Enable interrupts.
        asm!("cpsie i"); // cpsid to disable

        // Enable external interrupt 11
        // write_volatile(NVIC_ISER0 as *mut u32, 1 << 11);

        loop {
            if USING_DEV_KIT {
                write_volatile(GPIO_P0_OUTCLR, 1 << 14);
            } else {
                write_volatile(GPIO_P0_OUTCLR, 1 << 6);
            }

            send_packet(b"hello", RECEIVING).await;
            if !RECEIVING {
                delay_1s().await;
            }

            if USING_DEV_KIT {
                write_volatile(GPIO_P0_OUTSET, 1 << 14);
            } else {
                write_volatile(GPIO_P0_OUTSET, 1 << 6);
            }

            send_packet(b"world", RECEIVING).await;
            if !RECEIVING {
                delay_1s().await;
            }
        }
    }
}

// TODO: Switch back to returning '!'
fn main() -> () {
    unsafe {
        zero_bss();
        init_data();

        init_high_freq_clk();
        init_low_freq_clk();
        init_rtc0();

        if HELLO[0] != 4 {
            loop {
                unsafe { asm!("nop") };
            }
        }
    }

    Blinker::start();
    loop {
        unsafe { asm!("nop") };
    }
}
