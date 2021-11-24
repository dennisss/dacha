#![no_std]
#![no_main]
#![feature(lang_items, asm)]

extern crate pico_core;

use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

#[link_section = ".boot_loader"]
#[used]
pub static BOOT_LOADER: [u8; 256] = *include_bytes!("../../../third_party/pico-boot2.bin");

/*
export PICO_SDK_PATH=/home/dennis/workspace/pico-sdk

cargo install cargo-binutils
rustup component add llvm-tools-preview

cargo build --package rp2040 --target thumbv6m-none-eabi --release

cargo objdump --package rp2040 --target thumbv6m-none-eabi  --release -- -d --no-show-raw-insn


00200420

cargo objdump --release -- -d --no-show-raw-insn


Boot 2:
=> r0 = Start of user flash
=> r1 = Address of vtor offset register
=> Set vtor to start of user flash
=> Load r0 and r1 from first 8 bytes of



*/

/*

End of ram is 0x20042000

After the second stage, entry point will be at 0x10000100

TODO: Once I enter the main function, should I reset the stack pointer to the end.

Tiny 2040 uses W25Q64JVXGIQ and cryscatal osillator

- Start XOSC: 2.16.6 : init osc
- Switch system clock to XOSC: 2.15.6.1

First should switch to using XOSC

What to setup after second stage:
- PLL: See 2.18.3
- Voltage Regulator

Disable ROSC after XOSC is available

*/

/*
TODO: Follow the stuff in 'Figure 123'.
- Disable DW_apb_ssi first

SPI_CTRLR0_INST_L = 0
• SPI_CTRLR0_ADDR_L = 32 bits
• SPI_CTRLR0_XIP_CMD = 0xa0 (continuation code on W25Qx devices)


SSI_EN

TODO: Clock

TODO: Enable the XIP

The XIP operation is supported only in enhanced SPI modes (Dual, Quad) of operation. Therefore, the CTRLR0.SPI_FRF
bit should not be programmed to 0

LED is GP18, GP19, GP20 (active low)

TODO: Use PADS_BANK0 registers to support pull up / down configuration.

*/

#[link_section = ".vector_table.reset_vector"]
#[no_mangle]
pub static RESET_VECTOR: unsafe extern "C" fn() -> ! = entry;

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

#[no_mangle]
pub extern "C" fn entry() -> ! {
    main()
}

const IO_BANK0_BASE: u32 = 0x40014000;

const GPIO2_CTRL: *mut u32 = (IO_BANK0_BASE + 0x14) as *mut u32;
const GPIO18_CTRL: *mut u32 = (IO_BANK0_BASE + 0x94) as *mut u32;
const GPIO19_CTRL: *mut u32 = (IO_BANK0_BASE + 0x9c) as *mut u32;
const GPIO20_CTRL: *mut u32 = (IO_BANK0_BASE + 0xa4) as *mut u32;

const PADS_BANK0_BASE: u32 = 0x4001c000;

const GPIO18: *mut u32 = (PADS_BANK0_BASE + 0x4c) as *mut u32;
const GPIO19: *mut u32 = (PADS_BANK0_BASE + 0x50) as *mut u32;
const GPIO20: *mut u32 = (PADS_BANK0_BASE + 0x54) as *mut u32;

const SIO_FUNC: u32 = 5; // F5

const SIO_BASE: u32 = 0xd0000000;
const GPIO_OUT: *mut u32 = (SIO_BASE + 0x10) as *mut u32;
const GPIO_OE: *mut u32 = (SIO_BASE + 0x20) as *mut u32;


/*
1 Reset
2 NMI
3 HardFault
4-10 Reserved
11 SVCall
12-13 Reserved
14 PendSV
15 SysTick, optional
16 External Interrupt(0)
… …
16 + N External Interrupt(N)

*/

//
// 10000108

// 1000035c
// f7010010
// 100001f7

struct PWMChannel {
    csr: *mut u32,
    div: *mut u32,
    ctr: *mut u32,
    cc: *mut u32,
    top: *mut u32,
}

impl PWMChannel {
    fn get(index: u32) -> Self {
        Self {
            csr: (0x40050000 + (index * 20) + 0) as *mut u32,
            div: (0x40050000 + (index * 20) + 4) as *mut u32,
            ctr: (0x40050000 + (index * 20) + 8) as *mut u32,
            cc: (0x40050000 + (index * 20) + 12) as *mut u32,
            top: (0x40050000 + (index * 20) + 16) as *mut u32,
        }
    }
}

#[inline(always)]
fn main() -> ! {
    pico_core::init();

    // Set FUNCSEL to SIO
    unsafe { write_volatile(GPIO18_CTRL, 5) };
    // Output enable on pin 18
    unsafe { write_volatile(GPIO_OE, 1 << 18) };
    // Initially drive low.
    unsafe { write_volatile(GPIO_OUT, 0) };

    /*
    Pin 18 is PWM channel 1A
    So slice 1

    */

    // Currently 1.07Khz 1070 hz
    // So clock post div i 267,500 pre div is 5.3MHz
    // Seems like we are still running an the rosc.

    unsafe {
        let pwm = PWMChannel::get(1);

        // Set FUNCSEL to PWM 1A
        write_volatile(GPIO18_CTRL, 4);
        write_volatile(GPIO2_CTRL, 4);

        // 125MHz / 20 = 6.25Mhz
        // So 250 clock cycles to get a 25KHz vale
        write_volatile(pwm.div, 20 << 4);

        // CTR is not useful.

        write_volatile(pwm.cc, 150);

        write_volatile(pwm.top, 250);

        write_volatile(pwm.csr, 1 << 0); // Enable channel
    }

    loop {
        unsafe { write_volatile(GPIO_OUT, 0) };

        let mut i = 0;
        while i < 500000 {
            unsafe { asm!("nop") };
            i += 1;
        }

        unsafe { write_volatile(GPIO_OUT, 1 << 18) };

        // Drive output high
        // unsafe { core::ptr::write_volatile(GPIO18_CTRL, (0x3 << 12) | (0x3 << 8)) };

        i = 0;
        while i < 500000 {
            unsafe { asm!("nop") };
            i += 1;
        }
    }
}
