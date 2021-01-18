use crate::avr::interrupts::*;
use crate::avr::registers::*;
use core::ptr::{read_volatile, write_volatile};

pub struct EEPROM {}

impl EEPROM {
    async fn lock() {
        loop {
            // TODO: Technically must wait for flash to finish if writing
            let currently_writing = unsafe { read_volatile(EECR) } & (1 << 1) != 0; // EEPE is set.
            if currently_writing {
                // NOTE: This assumes that when this event is triggered, everyone is notified of
                // it
                InterruptEvent::EepromReady.await;
            } else {
                break;
            }
        }
    }

    pub async fn write_byte(addr: u16, value: u8) {
        Self::lock().await;

        unsafe {
            write_volatile(EEARL, addr as u8);
            write_volatile(EEARH, (addr >> 8) as u8);
            write_volatile(EEDR, value);
            write_volatile(EECR, 1 << 2); // EEMPE
            write_volatile(EECR, 1 << 1); // EEPE
        }

        InterruptEvent::EepromReady.await;
    }

    pub async fn read_byte(addr: u16) -> u8 {
        Self::lock().await;

        unsafe {
            write_volatile(EEARL, addr as u8);
            write_volatile(EEARH, (addr >> 8) as u8);
            write_volatile(EECR, 1 << 0); // EERE
            read_volatile(EEDR)
        }
    }
}
