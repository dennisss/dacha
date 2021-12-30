use peripherals::raw::uarte0::UARTE0;
use peripherals::raw::{Interrupt, PinDirection, RegisterRead, RegisterWrite};

pub struct UARTE {
    periph: UARTE0,
}

impl UARTE {
    pub fn new(mut periph: UARTE0) -> Self {
        periph.enable.write_enabled();

        periph.baudrate.write_baud115200();
        periph.config.write_with(|v| v); // Defaults to 8N1
        periph.psel.txd.write_with(|v| {
            v.set_connect_with(|v| v.set_connected())
                .set_port(0)
                .set_pin(30)
        });
        periph.psel.rxd.write_with(|v| {
            v.set_connect_with(|v| v.set_connected())
                .set_port(0)
                .set_pin(31)
        });

        Self { periph }
    }

    pub async fn write(&mut self, data: &[u8]) {
        // Stop any previous transmision.
        {
            self.periph.events_txstopped.write_notgenerated();
            self.periph.intenset.write_with(|v| v.set_txstopped()); // TODO: Clear me
            self.periph.tasks_stoptx.write_trigger();
            while self.periph.events_txstopped.read().is_notgenerated() {
                executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
            }
            self.periph.intenclr.write_with(|v| v.set_txstopped());
            self.periph.events_txstopped.write_notgenerated();
        }

        // NOTE: EasyDMA can only allow data in RAM.
        let mut buf = [0u8; 256];
        buf[0..data.len()].copy_from_slice(data);

        // TODO: If the future is cancelled, we need to stop the transmision to avoid
        // sending undefined bytes.

        self.periph
            .txd
            .ptr
            .write(unsafe { core::mem::transmute(buf.as_ptr()) });
        self.periph.txd.maxcnt.write(data.len() as u32);

        // Wait till done
        {
            self.periph.events_endtx.write_notgenerated();
            self.periph.intenset.write_with(|v| v.set_endtx()); // TODO: Clear me
            self.periph.tasks_starttx.write_trigger();
            while self.periph.events_endtx.read().is_notgenerated() {
                executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
            }
            self.periph.intenclr.write_with(|v| v.set_endtx());
            self.periph.events_endtx.write_notgenerated();
        }

        // assert_eq!(self.periph.txd.amount.read(), data.len() as u32);
    }

    pub async fn read_exact(&mut self, data: &mut [u8]) {
        // TODO: Cancel previous transmission

        self.periph
            .rxd
            .ptr
            .write(unsafe { core::mem::transmute(data.as_ptr()) });
        self.periph.rxd.maxcnt.write(data.len() as u32);

        // Wait till done
        {
            self.periph.events_endrx.write_notgenerated();
            self.periph.intenset.write_with(|v| v.set_endrx()); // TODO: Clear me
            self.periph.tasks_startrx.write_trigger();
            while self.periph.events_endrx.read().is_notgenerated() {
                executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
            }
            self.periph.events_endrx.write_notgenerated();
        }

        // TODO: Must wait for both the error and end conditions.
    }

    // TODO: We can implement a read_until which uses a shortcut to immediately
    // start reading the next byte once one it done.

    // Ideally we could support timeout based reading.
    // ^ We can call STOPRX to force the RXTO event to be triggered
    // NOTE: ENDRX is always generated after STOPRX if applicable.
}
