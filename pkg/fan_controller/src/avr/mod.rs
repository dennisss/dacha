// Based on /usr/lib/avr/include/avr/iom32u4.h
// Could also grab from https://github.com/avr-rust/avr-mcu/blob/master/packs/atmega/ATmega32U4.atdf

pub mod adc;
// pub mod fixed_array;
pub mod channel;
pub mod debug;
pub mod interrupts;
mod libc;
pub mod mutex;
pub mod pins;
pub mod progmem;
pub mod registers;
mod subroutines;
pub mod thread;
pub mod usart;
pub mod usb;
mod waker;

#[macro_use]
pub mod assert;

pub use crate::avr::adc::*;
pub use crate::avr::channel::*;
pub use crate::avr::interrupts::*;
use crate::avr::registers::*;
pub use crate::avr::usart::*;
pub use crate::avr::usb::*;
use core::future::Future;

// NOTE: PLL must be disabled before going into low power mode

/*
Usaully aside from the

Configs:
- EP0: 64 bytes (1 bank)
- EP1: IN 64 bytes (2 bank) (so total size is 128)
- EP2: OUT 64 bytes (2 bank) (so total size is 128)


IN is dev -> host:
- This isn't very time sensitive
- In the other direction, we need to

- Can first write to EEPROM and then actually do reload in order to ensure that all settings are consistent
    - Don't have enough RAM to do an in-place swap of config
    - But if reads are sparse, then we cna
*/

/*
    Typically we will have data
*/

/*
    Let's have one thread per Endpoint!

*/

#[cfg(target_arch = "avr")]
pub unsafe fn disable_interrupts() {
    llvm_asm!("cli");
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn disable_interrupts() {}

/// NOTE: This is unsafe as user code should use the thread abstraction which
/// operates under the assumption that there are no context switches.
#[cfg(target_arch = "avr")]
pub unsafe fn enable_interrupts() {
    llvm_asm!("sei");
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn enable_interrupts() {}

const FRZCLK: u8 = 5;
const IVCE: u8 = 0;
const UVREGE: u8 = 0;
const CLKPCE: u8 = 7;
const PINDIV: u8 = 4;
const PLLE: u8 = 1;
const PLOCK: u8 = 0;

const DETACH: u8 = 0;
const OTGPADE: u8 = 4;
const USBE: u8 = 7;

/// This function should be called as the first thing run on boot.
pub fn init() {
    // TODO: Consider what we need to do on events from resumes from sleep or resets
    // in order to ensure that the initial state is consistent.

    // NOTE: We assume that we start with the external clock used

    /*
        Other things to disable:
        - Reset all pin states
        - Reset all timers.

        - Clear all intterrupt
    */

    // TODO: Should I freeze the USB clock first? I guess not needed if USB
    // controller is not on yet.
    unsafe {
        ///////////////////////////
        // Step 1: Disable basically all interrupts and peripherals so that we in a well
        // defined state. This is mainly needed as the bootloader may have used these.

        disable_interrupts();

        // Make sure interrupt handlers are read from the main program and not from the
        // bootloader.
        // Also keep pull-ups enabled.
        avr_write_volatile(MCUCR, 1 << IVCE);
        avr_write_volatile(MCUCR, 0);

        // Reset UHWCON to default value
        // - Disable USB pad regulator
        avr_write_volatile(UHWCON, 0);

        // Reset USBCON to default value
        // - Disable USB controller
        // - Freeze USB clock
        avr_write_volatile(USBCON, 1 << FRZCLK);

        // Reset UDCON
        // - USB detached.
        avr_write_volatile(UDCON, 1 << DETACH);

        // Disable and clear USB general interrupts.
        avr_write_volatile(UDIEN, 0);
        avr_write_volatile(UDINT, 0);

        // Disable and clear external interrupts
        avr_write_volatile(EIMSK, 0);
        avr_write_volatile(EIFR, 0);

        // Disable and clear pin change interrupts.
        // avr_write_volatile(PCMSK0, 0);
        // avr_write_volatile(PCICR, 0);
        // avr_write_volatile(PCIFR, 0);

        // TODO: Disable a lot more interrupts.

        // TODO: Clear USB memory? or will the sliding make everything work.

        // Disable PLL
        avr_write_volatile(PLLCSR, 0);
        avr_write_volatile(PLLFRQ, 0);

        ////////
        // Step 2: Actually setting things up.

        // Enable clock prescaler changes.
        avr_write_volatile(CLKPR, 1 << CLKPCE);
        // Ensure that no pre-scaling is performed (clk_i/o and clk_adc are equal to the
        // main system clock).
        avr_write_volatile(CLKPR, 0);

        // Starting with 16Mhz external clock.

        // Output system 'clock / 2' from PLL pre-scaler
        avr_write_volatile(PLLCSR, 1 << PINDIV);

        // Connect PLL to pre-scaler
        // Divide input to generate 96Mhz PLL
        // Divide by 2 for USB 48MhZ clock
        // Divide by 1.5 for 64Mhz high speed timer clock.
        avr_write_volatile(PLLFRQ, 0b01101010);

        // Enable PLL
        avr_write_volatile(PLLCSR, avr_read_volatile(PLLCSR) | (1 << PLLE));

        // Wait for PLL lock
        while (avr_read_volatile(PLLCSR) & (1 << PLOCK)) == 0 {}

        // Timer 0: All pins are in normal operation without PWM
        // Run in CTC mode so that the counter resets when hitting OCR0A.
        // TODO: Will an interrupt still be triggered at the TOP value in CTC mode?
        avr_write_volatile(TCCR0A, 0);

        // Timer 0: Divide system clock / 64
        avr_write_volatile(TCCR0B, 0b011);

        // TODO: Clear the initial value of the timer?

        // Timer 0: Output Compare A: Interrupt every 1ms.
        avr_write_volatile(OCR0A, 250);
        // TODO: OCR0B

        // Timer 0: Output Compare A: Generate an interrupt
        avr_write_volatile(TIMSK0, 1 << 1);
    }

    usb_init();
}

// TOOD: Need to support USB suspend.
fn usb_init() {
    // TODO: Somewhere use UESTA1X::CTRLDIR

    // TODO: May want to set 'EPRST6:0 - Endpoint FIFO Reset Bits' upon resets

    // Other interesting bits: RSTDT, STALLRQ

    // Up to 256 in double bank mode
    unsafe {
        // NOTE: DPRAM is 832 bytes

        // TODO: Don't need to alter the power/clock state during a USB reset.

        // Enable USB pad regulator
        avr_write_volatile(UHWCON, 1 << UVREGE);

        // Enable USB controller and unfreeze clock
        // NOTE: Must be done as separate commands
        avr_write_volatile(USBCON, 1 << USBE | 1 << FRZCLK | 1 << OTGPADE); // TODO: OTGPADE?
        avr_write_volatile(USBCON, 1 << USBE | 1 << OTGPADE);

        // TODO: Reset each endpoint: EPRSTx
    }

    // TODO: Redo endpoint configs on End of Reset interrupts.

    // TODO: If RXOUTI is triggered, we need to verify that a CRC error didn't
    // also occur (rather drop the data)
}

#[no_mangle]
#[inline(never)]
pub async fn delay_ms(mut time_ms: u16) {
    while time_ms > 0 {
        InterruptEvent::OutputCompareOA.to_future().await;
        time_ms -= 1;
    }
}

// EICRA (INT3:0) and EICRB (INT6)
// . Note that recognition of falling or rising edge interrupts on INT6 requires
// the presence of an I/O clock, described in “System Clock and Clock Options”
// on page 27.
