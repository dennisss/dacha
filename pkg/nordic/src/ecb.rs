use core::arch::asm;
use core::result::Result;

use crypto::ccm::BlockCipherBuffer;
use peripherals::raw::register::{RegisterRead, RegisterWrite};
use peripherals::raw::Interrupt;

const BLOCK_SIZE: usize = 16; // 128-bit blocks

pub struct ECB {
    periph: peripherals::raw::ecb::ECB,
}

#[repr(C)]
pub struct ECBData {
    pub key: [u8; BLOCK_SIZE],
    pub plaintext: [u8; BLOCK_SIZE],
    pub ciphertext: [u8; BLOCK_SIZE],
}

impl ECB {
    pub fn new(mut periph: peripherals::raw::ecb::ECB) -> Self {
        periph.events_endecb.write_notgenerated();
        periph.events_errorecb.write_notgenerated();
        // periph
        //     .intenset
        //     .write_with(|v| v.set_endecb().set_errorecb());

        Self { periph }
    }

    pub fn encrypt(&mut self, data: &mut ECBData) {
        self.periph.events_endecb.write_notgenerated();
        self.periph.events_errorecb.write_notgenerated();

        self.periph
            .ecbdataptr
            .write(unsafe { core::mem::transmute(data) });

        self.periph.tasks_startecb.write_trigger();

        // According to the NRF52 data sheet, it takes ~7.2us to encrypt (or ~760 CPU
        // cycles at 64MHz). It's probably not worth bailing out to do other work at the
        // expense of increased async complexity.
        while self.periph.events_endecb.read().is_notgenerated()
            && self.periph.events_errorecb.read().is_notgenerated()
        {
            unsafe { asm!("nop") };
            // executor::interrupts::wait_for_irq(Interrupt::ECB).await;
        }

        let failed = self.periph.events_errorecb.read().is_generated();
        assert_no_debug!(!failed);

        // TODO: Do we need to do this? If we reset them at the beginning of we
        // disable the interrupt at the end of wait_for_irg, will the
        // interrupt still be pending?
        // self.periph.events_endecb.write_notgenerated();
        // self.periph.events_errorecb.write_notgenerated();
    }
}

// Software implementation of AES-CCM built on top of the ECB peripheral on the
// NRF chips.
//
// See https://datatracker.ietf.org/doc/html/rfc3610
pub struct AES128BlockBuffer<'a> {
    ecb: &'a mut ECB,
    data: ECBData,
}

impl<'a> AES128BlockBuffer<'a> {
    pub fn new(key: &[u8; BLOCK_SIZE], ecb: &'a mut ECB) -> Self {
        Self {
            ecb,
            data: ECBData {
                key: key.clone(),
                // TODO: Only need to append the nonce in the middle.
                plaintext: [0u8; BLOCK_SIZE],
                ciphertext: [0u8; BLOCK_SIZE],
            },
        }
    }
}

impl<'a> BlockCipherBuffer for AES128BlockBuffer<'a> {
    fn plaintext(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.plaintext
    }

    fn plaintext_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        &mut self.data.plaintext
    }

    fn plaintext_mut_ciphertext(&mut self) -> (&mut [u8; BLOCK_SIZE], &[u8; BLOCK_SIZE]) {
        (&mut self.data.plaintext, &self.data.ciphertext)
    }

    fn encrypt(&mut self) {
        self.ecb.encrypt(&mut self.data);
    }

    fn ciphertext(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.ciphertext
    }
}
