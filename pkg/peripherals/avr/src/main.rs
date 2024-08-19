#![no_std]
#![feature(asm_experimental_arch, abi_avr_interrupt)]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

/*
Our pin usage:
- PB1 as LED PWM 1
- PB4 (OC1B) ('Timer/Counter1') as LED PWM 2 output.
    - Pull down to ground
- PB0 (SDA)

EEPROM contents
- Crystal calibration byte
- LED1 address
- LED2 addres

Default clock on ATTiny85:
- RC Oscillator
    - CKSEL[3:0] = 0010
    - This sets it to 8MHz
- Divided by 8 to get a 1MHz system clock.

PLL
- Input is the 8MHz RC oscillator
- Input optionally divided by 2 if PLLCSR.LSM = 1
    - By default, this is disabled
- Then multiplies the input clock by 8x
- To turn up on PLL, need to set PLLCSR.PPLE = 1 and wait for PLLCSR.PLOCK to be 1

UART
- Normally high but falls low for the first start bit.
- UART via USI
    - http://www.technoblogy.com/show?RPY
    - https://becomingmaker.com/usi-serial-uart-attiny85/
    - Tuning the oscillator: https://becomingmaker.com/tuning-attiny-oscillator/

I2C
- Start Condition
    - Master leaves SCL high and pulls SDA low.
- Each byte is ACKed by the slave by pulling SDA low on each 9th bit.
*/

// TODO: Move to a different file.
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

// Note that I/O registers are mutated in 'data space' (write_volatile compiled
// code uses an 'sts' instead of an 'out' instruction) which is interleaved with
// other memory so we need to offset the addresses.
const IO_BASE: u16 = 0x20;

const DDRB: *mut u8 = (IO_BASE + 0x17) as *mut u8;
const PORTB: *mut u8 = (IO_BASE + 0x18) as *mut u8;

const PLLCSR: *mut u8 = (IO_BASE + 0x27) as *mut u8;

const TCCR1: *mut u8 = (IO_BASE + 0x30) as *mut u8;

const GTCCR: *mut u8 = (IO_BASE + 0x2C) as *mut u8;

const OCR1B: *mut u8 = (IO_BASE + 0x2B) as *mut u8;

const OCR1C: *mut u8 = (IO_BASE + 0x2D) as *mut u8;

const USIDR: *mut u8 = (IO_BASE + 0x0F) as *mut u8;
const USIBR: *mut u8 = (IO_BASE + 0x10) as *mut u8;
const USISR: *mut u8 = (IO_BASE + 0x0E) as *mut u8;
const USICR: *mut u8 = (IO_BASE + 0x0D) as *mut u8;

//

// GTCCR

//

#[interrupt(attiny85)]
fn PCINT0() {
    //
}

fn sleep() {
    for i in 0..1_000 {
        unsafe {
            asm!("nop");
        }
    }
}

#[no_mangle]
fn main() {
    // PWM setup for PB4 (OC1B) ('Timer/Counter1')
    // - Counter runs at 1Mhz so given we use it as an 8-bit counter, the PWM wave
    //   is ~3.9kHz.
    unsafe {
        // Disable PLL.
        // Timer/Counter1 uses system clock as the the counter prescaler input clock.
        core::ptr::write_volatile(PLLCSR, 0);

        // Set PB4 to have OUTPUT pin direction.
        core::ptr::write_volatile(DDRB, 1 << 4);

        core::ptr::write_volatile(
            TCCR1, 0b0001, /* Use 'system clock * 1' as counter clock */
        );

        core::ptr::write_volatile(GTCCR, {
            0b1 << 6 | // PWM1B : Enable PWM mode.
            0b01 << 4 // COM1B[1:0] : OC1B set on 0x00 counter. cleared on
                      // compare value.
        });

        // Default to completely off.
        core::ptr::write_volatile(OCR1B, 0);

        // Reset the counter when it reaches 255.
        core::ptr::write_volatile(OCR1C, 0xff);
    }

    unsafe {
        loop {
            for i in 0..255 {
                core::ptr::write_volatile(OCR1B, i);
                sleep();
            }

            for i in (0..255).rev() {
                core::ptr::write_volatile(OCR1B, i);
                sleep();
            }

            // Write pin output high.
            // // core::ptr::write_volatile(PORTB, 1 << 3);

            // Write pin output low.
            // // core::ptr::write_volatile(PORTB, 0);

            // sleep();
        }

        loop {
            asm!("nop");
        }
    }
}
