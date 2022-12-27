#![no_std]
#![no_main]
#![feature(lang_items, asm)]

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

/*
Reference clocks code:
- https://github.com/raspberrypi/pico-sdk/blob/bfcbefafc5d2a210551a4d9d80b4303d4ae0adf7/src/rp2_common/hardware_clocks/clocks.c

Reference runtime code:
- https://github.com/raspberrypi/pico-sdk/blob/bfcbefafc5d2a210551a4d9d80b4303d4ae0adf7/src/rp2_common/pico_runtime/runtime.c
*/

const XOSC_BASE: u32 = 0x40024000;
const XOSC_CTRL: *mut u32 = (XOSC_BASE + 0x00) as *mut u32;
const XOSC_STATUS: *mut u32 = (XOSC_BASE + 0x04) as *mut u32;
const XOSC_DORMANT: *mut u32 = (XOSC_BASE + 0x08) as *mut u32;
const XOSC_STARTUP: *mut u32 = (XOSC_BASE + 0x0c) as *mut u32;
const XOSC_COUNT: *mut u32 = (XOSC_BASE + 0x1c) as *mut u32;

const CRYSTAL_FREQ: u32 = 12 * MHZ;

const PLL_SYS_BASE: u32 = 0x40028000;
const PLL_USB_BASE: u32 = 0x4002c000;

const ROSC_CTRL: *mut u32 = (0x40060000 + 0x00) as *mut u32;

const RESETS_BASE: u32 = 0x4000c000;

const RESET: *mut u32 = (RESETS_BASE + 0x00) as *mut u32;

const RESET_DONE: *mut u32 = (RESETS_BASE + 0x08) as *mut u32;

const CLK_SYS_RESUS_CTRL: *mut u32 = (0x40008000 + 0x78) as *mut u32;

// Based on RP2040 datasheet section 2.16.6.
unsafe fn xosc_init() {
    write_volatile(XOSC_CTRL, 0xaa0); // 1_15MHz
    write_volatile(XOSC_STARTUP, 50);

    // Enable it.
    write_volatile(XOSC_CTRL, read_volatile(XOSC_CTRL) | (0xfab << 12));

    // Wait until stable.
    while read_volatile(XOSC_STATUS) & (1 << 31) == 0 {
        asm!("nop");
    }
}

unsafe fn pll_init(base_addr: u32, ref_div: u32, vco_freq: u32, post_div1: u32, post_div2: u32) {
    let reg_cs = (base_addr + 0x00) as *mut u32;
    let reg_pwr = (base_addr + 0x04) as *mut u32;
    let reg_fbdiv_int = (base_addr + 0x08) as *mut u32;
    let reg_prim = (base_addr + 0x0c) as *mut u32;

    let ref_mhz: u32 = (CRYSTAL_FREQ / MHZ) / ref_div;
    let fbdiv: u32 = vco_freq / (ref_mhz * MHZ);

    let pdiv = (post_div1 << 16) | (post_div2 << 12);

    // Reset PLL
    {
        let bit = {
            if base_addr == PLL_SYS_BASE {
                1 << 12
            } else {
                1 << 13
            }
        };

        write_volatile(RESET, read_volatile(RESET) & bit);
        write_volatile(RESET, read_volatile(RESET) & !bit);
        while read_volatile(RESET_DONE) & bit != bit {
            asm!("nop");
        }
    }

    write_volatile(reg_cs, ref_div);
    write_volatile(reg_fbdiv_int, fbdiv);

    // Power on.
    write_volatile(
        reg_pwr,
        read_volatile(reg_pwr)
            & !(
                1 << 0 | // PD
                1 << 5
                // VCOPD
            ),
    );

    // Wait for lock
    loop {
        if read_volatile(reg_cs) & (1 << 31) != 0 {
            break;
        }

        asm!("nop");
    }

    write_volatile(reg_prim, pdiv);

    write_volatile(
        reg_pwr,
        read_volatile(reg_pwr)
            & !(
                1 << 3
                // POSTDIVPD
            ),
    );
}

#[derive(PartialEq, Clone, Copy)]
enum ClockIndex {
    GPOut0 = 0,
    GPOut1 = 1,
    GPOut2 = 2,
    GPOut3 = 3,
    ClkRef = 4,
    ClkSys = 5,
    ClkPeri = 6,
    ClkUSB = 7,
    ClkADC = 8,
    ClkRTC = 9,
}

struct Clock {
    ctrl: *mut u32,
    div: *mut u32,
    selected: *mut u32,
}

impl Clock {
    fn get(index: ClockIndex) -> Self {
        let ctrl = (0x40008000 + 12 * (index as u32)) as *mut u32;
        let div = (0x40008000 + 12 * (index as u32) + 4) as *mut u32;
        let selected = (0x40008000 + 12 * (index as u32) + 8) as *mut u32;
        Self {
            ctrl,
            div,
            selected,
        }
    }
}

unsafe fn clock_configure(clk_index: ClockIndex, src: u32, auxsrc: u32, src_freq: u32, freq: u32) {
    let clock = Clock::get(clk_index);

    let div = (((src_freq as u64) << 8) / (freq as u64)) as u32;

    if div > read_volatile(clock.div) {
        write_volatile(clock.div, div);
    }

    // Disable clock (does nothing for clk_sys and clk_ref)
    write_volatile(clock.ctrl, read_volatile(clock.ctrl) & !(1 << 11));

    // Set aux mux
    write_volatile(
        clock.ctrl,
        (read_volatile(clock.ctrl) & !(0b111 << 5)) | (auxsrc << 5),
    );

    if clk_index == ClockIndex::ClkSys || clk_index == ClockIndex::ClkRef {
        write_volatile(clock.ctrl, (read_volatile(clock.ctrl) & !0b11) | src);

        while read_volatile(clock.selected) != (1 << src) {
            asm!("nop");
        }
    }

    // Enable clock
    write_volatile(clock.ctrl, read_volatile(clock.ctrl) | (1 << 11));

    write_volatile(clock.div, div);
}

const MHZ: u32 = 1_000_000;

pub fn init() {
    // See https://github.com/raspberrypi/pico-sdk/blob/bfcbefafc5d2a210551a4d9d80b4303d4ae0adf7/src/rp2_common/pico_runtime/runtime.c#L63
    unsafe {
        write_volatile(
            RESET,
            1 << 0 | // ADC
                1 << 1 | // BUSCTL
                1 << 2 | // DMA
                1 << 3 | // I2C0
                1 << 4 | // I2C1
                1 << 5 | // IO_BANK0
                // 1 << 6 | // IO_QSPI
                1 << 7 | // JTAG
                1 << 8 | // PADS_BANK0
                // 1 << 9 | // PADS_QSPI
                1 << 10 | // PIO0
                1 << 11 | // PIO1
                // 1 << 12 | // PLL_SYS
                // 1 << 13 | // PLL_USB
                1 << 14 | // PWM
                1 << 15 | // RTC
                1 << 16 | // SPI0
                1 << 17 | // SPI1
                // 1 << 18 | // SYSCFG
                1 << 19 | // SYSINFO
                1 << 20 | // TBMAN
                1 << 21 | // TIMER
                1 << 22 | // UART0
                1 << 23 | // UART1
                1 << 24 | // USBCTRL
                0,
        )
    };

    let reset2 = 1 << 0 | // ADC
        // 1 << 1 | // BUSCTL
        // 1 << 2 | // DMA
        // 1 << 3 | // I2C0
        // 1 << 4 | // I2C1
        // 1 << 5 | // IO_BANK0
        // 1 << 6 | // IO_QSPI
        // 1 << 7 | // JTAG
        // 1 << 8 | // PADS_BANK0
        // 1 << 9 | // PADS_QSPI
        // 1 << 10 | // PIO0
        // 1 << 11 | // PIO1
        // 1 << 12 | // PLL_SYS
        // 1 << 13 | // PLL_USB
        // 1 << 14 | // PWM
        1 << 15 | // RTC
        1 << 16 | // SPI0
        1 << 17 | // SPI1
        // 1 << 18 | // SYSCFG
        // 1 << 19 | // SYSINFO
        // 1 << 20 | // TBMAN
        // 1 << 21 | // TIMER
        1 << 22 | // UART0
        1 << 23 | // UART1
        // 1 << 24 | // USBCTRL
        0;

    // all - some

    unsafe { write_volatile(RESET, reset2) };

    let expected_done = 0x01ffffff & !reset2; // All bits that we expect to be done resetting

    for i in 0..100000 {
        unsafe { asm!("nop") };
    }
    // loop {
    //     let resets_done = unsafe { read_volatile(RESET_DONE) };
    //     if resets_done & expected_done == expected_done {
    //         break;
    //     }

    //     unsafe { asm!("nop") };
    // }

    unsafe {
        // Disable resus
        write_volatile(CLK_SYS_RESUS_CTRL, 0);

        xosc_init();

        // Switch clk_sys and clk_ref away from aux
        {
            let clk_sys = Clock::get(ClockIndex::ClkSys);
            write_volatile(clk_sys.ctrl, read_volatile(clk_sys.ctrl) & !1);
            while read_volatile(clk_sys.selected) != 1 {
                asm!("nop");
            }

            // Use ROSC
            let clk_ref = Clock::get(ClockIndex::ClkRef);
            write_volatile(clk_ref.ctrl, read_volatile(clk_ref.ctrl) & !0b11);
            while read_volatile(clk_ref.selected) != 1 {
                asm!("nop");
            }
        }

        pll_init(PLL_SYS_BASE, 1, 1500 * MHZ, 6, 2);
        pll_init(PLL_USB_BASE, 1, 480 * MHZ, 5, 2);

        // Configure clocks
        // CLK_REF = XOSC (12MHz) / 1 = 12MHz
        clock_configure(
            ClockIndex::ClkRef,
            0x2, // XOSC
            0,   // No aux mux
            12 * MHZ,
            12 * MHZ,
        );

        // CLK SYS = PLL SYS (125MHz) / 1 = 125MHz
        clock_configure(
            ClockIndex::ClkSys,
            0x1, // AUX
            0x0, // PLL_SYS
            125 * MHZ,
            125 * MHZ,
        );

        // CLK USB = PLL USB (48MHz) / 1 = 48MHz
        clock_configure(
            ClockIndex::ClkUSB,
            0,   // No GLMUX
            0x0, // PLL_USB
            48 * MHZ,
            48 * MHZ,
        );

        // CLK ADC = PLL USB (48MHZ) / 1 = 48MHz
        clock_configure(
            ClockIndex::ClkADC,
            0,    // No GLMUX
            0x00, // PLL_USB
            48 * MHZ,
            48 * MHZ,
        );

        // CLK RTC = PLL USB (48MHz) / 1024 = 46875Hz
        clock_configure(
            ClockIndex::ClkRTC,
            0, // No GLMUX
            0, // PLL_USB
            48 * MHZ,
            46875,
        );

        // CLK PERI = clk_sys. Used as reference clock for Peripherals. No dividers so
        // just select and enable Normally choose clk_sys or clk_usb
        clock_configure(
            ClockIndex::ClkPeri,
            0,
            0, // CLK_SYS
            125 * MHZ,
            125 * MHZ,
        );

        // stack guard?
    }

    unsafe { write_volatile(RESET, 0) };

    // unsafe { write_volatile(ROSC_CTRL, 0xd1e << 12) };
}
