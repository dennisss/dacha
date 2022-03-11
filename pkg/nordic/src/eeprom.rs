/*
24LC256:
- 0x8000 bytes.
- 400 kHz clock.
- 64 byte pages
- 5ms to write a page.
    - 2K pullup to VCC (10K too much)

- 7-bit address is: 1010XXX

- Write operation
    - Payload is [address high] [address low] [byte]
    - Page-write is the same thing, but send more than 1 byte.
- Polling
    - Send address with write byte enabled.
    - Device will not acknowledge if still writing
- Reads read at the current address
    - Addresses are set by doing write operations without sending bytes

*/

use core::arch::asm;
use core::future::Future;

use common::errors::*;
use peripherals::eeprom::EEPROM;

use crate::gpio::{GPIOPin, PinDirection, PinLevel};
use crate::log;
use crate::twim::{TWIMError, TWIM};

pub struct Microchip24XX256 {
    periph: TWIM,
    address: u8,
    write_protect: GPIOPin,
}

impl Microchip24XX256 {
    pub const PAGE_SIZE: usize = 64;

    pub fn new(periph: TWIM, address: u8, mut write_protect: GPIOPin) -> Self {
        write_protect
            .set_direction(PinDirection::Output)
            .write(PinLevel::High);

        Self {
            periph,
            address,
            write_protect,
        }
    }

    // TODO: Return an error if reading or writing beyond end of EEPROM

    async fn read_impl(&mut self, offset: usize, data: &mut [u8]) -> Result<()> {
        // TODO: Check that the offset + data.len() < total_size
        let offset = (offset as u16).to_be_bytes();
        self.periph
            .write_then_read(self.address, Some(&offset[..]), Some(data))
            .await?;
        Ok(())
    }

    /// TODO: Support doing other things on the port while we wait for an ACK
    async fn write_impl(&mut self, offset: usize, data: &[u8]) -> Result<()> {
        let write_guard = WriteEnabledGuard::new(&mut self.write_protect);

        let mut buf = [0u8; 2 + Self::PAGE_SIZE];
        *array_mut_ref![buf, 0, 2] = (offset as u16).to_be_bytes();
        buf[2..(2 + data.len())].copy_from_slice(data);

        self.periph.write(self.address, &buf).await?;

        // TODO: If the write future is cancelled, we still need to mark the EEPROM as
        // writing so that it can be ACK'ed later.

        // Wait for the device to acknowledge a write to know
        // TODO: Have a maximum amount of time to do this.
        while let Err(_) = self.periph.write(self.address, &[]).await {
            // TODO: Sleep
            continue;
        }

        drop(write_guard);

        Ok(())
    }
}

impl EEPROM for Microchip24XX256 {
    type ReadFuture<'a> = impl Future<Output = Result<()>> + 'a;
    type WriteFuture<'a> = impl Future<Output = Result<()>> + 'a;

    fn total_size(&self) -> usize {
        0x8000
    }

    fn page_size(&self) -> usize {
        Self::PAGE_SIZE
    }

    fn read<'a>(&'a mut self, offset: usize, data: &'a mut [u8]) -> Self::ReadFuture<'a> {
        self.read_impl(offset, data)
    }

    fn write<'a>(&'a mut self, offset: usize, data: &'a [u8]) -> Self::WriteFuture<'a> {
        self.write_impl(offset, data)
    }
}

struct WriteEnabledGuard<'a> {
    write_protect: &'a mut GPIOPin,
}

impl<'a> Drop for WriteEnabledGuard<'a> {
    fn drop(&mut self) {
        // Re-enable write protect.
        self.write_protect.write(PinLevel::High);
    }
}

impl<'a> WriteEnabledGuard<'a> {
    pub fn new(write_protect: &'a mut GPIOPin) -> Self {
        write_protect.write(PinLevel::Low);
        // Must wait for propagation.
        for i in 0..200 {
            unsafe { asm!("nop") };
        }

        Self { write_protect }
    }
}
