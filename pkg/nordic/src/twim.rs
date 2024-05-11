use common::errors::*;
use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::twim0::TWIM0;
use peripherals::raw::{Interrupt, InterruptState};

use crate::pins::{connect_pin, PeripheralPin};

/// NOTE: Requires a HFCLK.
pub struct TWIM {
    periph: TWIM0,
}

#[derive(Clone, Copy, Debug, Errable)]
#[repr(u32)]
pub enum TWIMError {
    Overrun,
    AddressNotAcknowledged,
    DataNotAcnkowledged,
    UnsupportedBaudrate,
}

impl TWIM {
    pub fn new<SCLPin: PeripheralPin, SDAPin: PeripheralPin>(
        mut periph: TWIM0,
        scl: SCLPin,
        sda: SDAPin,
        frequency: usize,
    ) -> Self {
        connect_pin(scl, &mut periph.psel.scl);
        connect_pin(sda, &mut periph.psel.sda);

        match frequency {
            100_000 => periph.frequency.write_k100(),
            250_000 => periph.frequency.write_k250(),
            400_000 => periph.frequency.write_k400(),
            _ => panic!(), // TODO: Return an error
        };

        periph.inten.write_with(|v| {
            v.set_stopped(InterruptState::Enabled)
                .set_error(InterruptState::Enabled)
        });
        periph.enable.write_enabled();

        Self { periph }
    }

    // TODO: Support vectorized read
    pub async fn read(&mut self, address: u8, data: &mut [u8]) -> Result<(), TWIMError> {
        self.write_then_read(address, None, Some(data)).await
    }

    /// TODO: support vectorized write.
    pub async fn write(&mut self, address: u8, data: &[u8]) -> Result<(), TWIMError> {
        self.write_then_read(address, Some(data), None).await
    }

    pub async fn write_then_read(
        &mut self,
        address: u8,
        write_data: Option<&[u8]>,
        read_data: Option<&mut [u8]>,
    ) -> Result<(), TWIMError> {
        self.periph.events_error.write_notgenerated();
        self.periph.events_stopped.write_notgenerated();

        // NOTE: Writing 1 (received) clears the value.
        self.periph.errorsrc.write_with(|v| {
            v.set_overrun_with(|v| v.set_received())
                .set_anack_with(|v| v.set_received())
                .set_dnack_with(|v| v.set_received())
        });

        self.periph.shorts.write_with(|v| {
            // If reading after writing, immediately call STARTRX after LASTTX, otherwise
            // stop after writing.
            if read_data.is_some() {
                v.set_lasttx_startrx_with(|v| v.set_enabled());
            } else {
                v.set_lasttx_stop_with(|v| v.set_enabled());
            }

            // If reading, always follow it with stopping.
            v.set_lastrx_stop_with(|v| v.set_enabled())
        });

        self.periph.address.write(address as u32);

        let mut buf = [0u8; 1];

        // TODO: Verify that write_data and read_data is in ram.

        if let Some(write_data) = write_data.as_ref() {
            self.periph.txd.ptr.write(unsafe {
                core::mem::transmute::<*const u8, u32>(if write_data.len() == 0 {
                    buf.as_ptr()
                } else {
                    write_data.as_ptr()
                })
            });
            self.periph.txd.maxcnt.write(write_data.len() as u32);
        }

        if let Some(read_data) = read_data.as_ref() {
            self.periph
                .rxd
                .ptr
                .write(unsafe { core::mem::transmute::<*const u8, u32>(read_data.as_ptr()) });
            self.periph.rxd.maxcnt.write(read_data.len() as u32);
        }

        crate::events::flush_events_clear();

        if let Some(write_data) = write_data.as_ref() {
            self.periph.tasks_starttx.write_trigger();

            // The LASTTX event is nopt triggered with zero length inputs.
            // See https://devzone.nordicsemi.com/f/nordic-q-a/37665/twim-clock-pin-is-pull-low-after-sending-zero-bytes-data
            if write_data.is_empty() {
                self.periph.tasks_stop.write_trigger();
            }
        } else if read_data.is_some() {
            self.periph.tasks_startrx.write_trigger();
        } else {
            return Ok(());
        }

        let mut transfer = TWIMTransfer {
            periph: &mut self.periph,
            running: true,
        };

        // Wait until we've stopped.
        while transfer.periph.events_stopped.read().is_notgenerated() {
            // If we see an error, trigger a stop
            if transfer.periph.events_error.read().is_generated() {
                transfer.periph.events_error.write_notgenerated();
                transfer.periph.tasks_stop.write_trigger();
            }

            executor::interrupts::wait_for_irq(Interrupt::SPIM0_SPIS0_TWIM0_TWIS0_SPI0_TWI0).await;
        }

        transfer.periph.events_stopped.write_notgenerated();
        transfer.running = false;

        let errorsrc = transfer.periph.errorsrc.read();
        if errorsrc.anack().is_received() {
            return Err(TWIMError::AddressNotAcknowledged);
        }
        if errorsrc.dnack().is_received() {
            return Err(TWIMError::DataNotAcnkowledged);
        }
        if errorsrc.overrun().is_received() {
            return Err(TWIMError::Overrun);
        }

        // TODO: Verify TXD.AMoUNT and RXD.AMOUNT

        Ok(())
    }
}

struct TWIMTransfer<'a> {
    periph: &'a mut TWIM0,
    running: bool,
}

impl<'a> Drop for TWIMTransfer<'a> {
    fn drop(&mut self) {
        self.cancel_blocking();
    }
}

impl<'a> TWIMTransfer<'a> {
    fn cancel_blocking(&mut self) {
        if !self.running {
            return;
        }

        self.periph.tasks_stop.write_trigger();
        while self.periph.events_stopped.read().is_notgenerated() {}
        self.periph.events_stopped.write_notgenerated();
    }
}
