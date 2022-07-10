#![no_std]

#[macro_use]
extern crate common;

pub mod register;

// #[cfg(target_label = "nrf52840")]
pub mod nrf52840 {
    #![allow(
        dead_code,
        non_snake_case,
        non_camel_case_types,
        unused_imports,
        unused_variables
    )]

    include!(concat!(env!("OUT_DIR"), "/nrf52840.rs"));
}

// #[cfg(target_label = "nrf52840")]
pub use nrf52840::*;

// #[cfg(target_label = "cortex_m")]
pub mod nvic;

// Cortex-M specific
// See https://developer.arm.com/documentation/ddi0439/b/System-Control/Register-summary
pub fn reset() -> ! {
    // TODO: Alternatively on NRF52's, we can set the RESET register in the CTRL-AP
    // block.

    const AIRCR: *mut u32 = 0xE000ED0C as *mut u32;
    unsafe {
        core::ptr::write_volatile(
            AIRCR,
            0x5FA << 16 | // VECTKEYSTAT (requires to allow writes to this register).
            1 << 2, // SYSRESETREQ
        )
    };

    // Never reached
    loop {}
}
