use crate::avr::interrupts::*;
use crate::avr::registers::*;

pub struct EEPROM {}

impl EEPROM {
    async fn wait_idle() {
        // TODO: Needto register for EEPROM interrupts.

        loop {
            // TODO: Technically must wait for flash to finish if writing
            let currently_writing = unsafe { avr_read_volatile(EECR) } & (1 << 1) != 0; // EEPE is set.
            if currently_writing {
                // NOTE: This assumes that when this event is triggered, everyone is notified of
                // it
                InterruptEvent::EepromReady.to_future().await;
            } else {
                break;
            }
        }
    }

    pub async fn write_byte(addr: u16, value: u8) {
        Self::wait_idle().await;

        unsafe {
            avr_write_volatile(EEARL, addr as u8);
            avr_write_volatile(EEARH, (addr >> 8) as u8);
            avr_write_volatile(EEDR, value);
            avr_write_volatile(EECR, 1 << 2); // EEMPE
            avr_write_volatile(EECR, 1 << 1); // EEPE
        }

        InterruptEvent::EepromReady.to_future().await;
    }

    pub async fn read_byte(addr: u16) -> u8 {
        Self::wait_idle().await;

        unsafe {
            avr_write_volatile(EEARL, addr as u8);
            avr_write_volatile(EEARH, (addr >> 8) as u8);
            avr_write_volatile(EECR, 1 << 0); // EERE
            avr_read_volatile(EEDR)
        }
    }
}
