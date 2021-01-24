// Based on /usr/lib/avr/include/avr/iom32u4.h
// Could also grab from https://github.com/avr-rust/avr-mcu/blob/master/packs/atmega/ATmega32U4.atdf

pub mod adc;
pub mod arena_stack;
// pub mod fixed_array;
pub mod interrupts;
pub mod pins;
pub mod progmem;
pub mod registers;
pub mod serial;
mod subroutines;
pub mod thread;
pub mod usb;
mod waker;

pub use crate::avr::adc::*;
pub use crate::avr::interrupts::*;
use crate::avr::registers::*;
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

fn usb_control_thread() {
    loop {
        // Issues:
        // - If endpoints are reset while waiting for the RXSTPI intterupt or
        //   another, we won't have the interrupt enabled anymore so will never
        //   get ti

        // TODO: On any async delay, we need to remember to re-configure the
        // current endpoint

        // Wait for RXSTPI

        // Read SETUP packet

        // Clear RXSTPI and TXINI

        // Looping over data to send:
        // Wait for TXINI
        // Send response data

        // Wait for RXOUTI
        // Clear to finish?
    }
}

pub fn init() {
    // TODO: Consider what we need to do on events from resumes from sleep or resets
    // in order to ensure that the initial state is consistent.

    // NOTE: We assume that we start with the external clock used

    // TODO: Should I freeze the USB clock first? I guess not needed if USB
    // controller is not on yet.
    // TODO: Configure the PLL and check PLL lock?
    unsafe {
        // Enable clock prescaler changes.
        avr_write_volatile(CLKPR, 1 << 7);
        // Ensure that no pre-scaling is performed (clk_i/o and clk_adc are equal to the
        // main system clock).
        avr_write_volatile(CLKPR, 0);

        // Starting with 16Mhz external clock.

        // Output system 'clock / 2' from PLL pre-scaler
        avr_write_volatile(PLLCSR, 1 << 4);

        // Connect PLL to pre-scaler
        // Divide input to generate 96Mhz PLL
        // Divide by 2 for USB 48MhZ clock
        // Divide by 1.5 for 64Mhz high speed timer clock.
        avr_write_volatile(PLLFRQ, 0b01101010);

        // Enable PLL
        avr_write_volatile(PLLCSR, avr_read_volatile(PLLCSR) | 1 << 1);

        // Wait for PLL lock
        while avr_read_volatile(PLLCSR) & 0b1 == 0 {}

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
}

/*
pub fn usb_init() {
    // TODO: Somewhere use UESTA1X::CTRLDIR

    // TODO: May want to set 'EPRST6:0 - Endpoint FIFO Reset Bits' upon resets

    // Other interesting bits: RSTDT, STALLRQ

    // Up to 256 in double bank mode
    unsafe {
        // NOTE: DPRAM is 832 bytes

        // Enable USB pad regulator
        avr_write_volatile(UHWCON, 0b1);
        // Enable USB controller
        avr_write_volatile(USBCON, 1 << 7); // TODO: OTGPADE?

        USB_EP0.configure(
            USBEndpointType::Control,
            USBEndpointDirection::OutOrControl,
            USBEndpointSize::B64,
            USBEndpointBanks::One,
        );

        USB_EP1.configure(
            USBEndpointType::Interrupt,
            USBEndpointDirection::In,
            USBEndpointSize::B128,
            USBEndpointBanks::Double,
        );

        USB_EP2.configure(
            USBEndpointType::Interrupt,
            USBEndpointDirection::OutOrControl,
            USBEndpointSize::B128,
            USBEndpointBanks::Double,
        );

        // USB full speed
        // Do not reset on USB connection.
        // Not DETACHed
        avr_write_volatile(UDCON, 0);

        // Enable 'End of Reset' interrupt.
        // NOTE: When this happens, we need to make sure to clear the flag.
        avr_write_volatile(UDIEN, 1 << 3);
    }

    // TODO: Redo endpoint configs on End of Reset interrupts.

    // TODO: If RXOUTI is triggered, we need to verify that a CRC error didn't
    // also occur (rather drop the data)
}
*/

// If

/*
Control Write (receiving data):
- Get RXOUTI interrupt whenever we have data to receive ()
- Wait for NAKINI


Control Read
- First Unset TXINI after getting setup packet
- Wait for TCINI to go high in order to write data


*/

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
