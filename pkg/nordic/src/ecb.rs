use core::result::Result;

use peripherals::raw::{Interrupt, RegisterRead, RegisterWrite};

pub struct ECB {
    periph: peripherals::raw::ecb::ECB,
}

#[repr(C)]
pub struct ECBData {
    pub key: [u8; 16],
    pub plaintext: [u8; 16],
    pub ciphertext: [u8; 16],
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
        assert!(!failed);

        // TODO: Do we need to do this? If we reset them at the beginning of we
        // disable the interrupt at the end of wait_for_irg, will the
        // interrupt still be pending?
        // self.periph.events_endecb.write_notgenerated();
        // self.periph.events_errorecb.write_notgenerated();
    }
}
