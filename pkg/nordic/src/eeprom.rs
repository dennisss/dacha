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

use core::result::Result;

use crate::twim::{TWIMError, TWIM};

const PAGE_SIZE: usize = 64;

pub struct EEPROM {
    periph: TWIM,
    address: u8,
}

impl EEPROM {
    // Returns the total number of bytes that can be stored in this EEPROM.
    pub fn total_size(&self) -> usize {
        0x8000
    }

    /// Returns the size of a single page in the EEPROM. This would be the
    /// largest/smallest amount of data that can be written in one operation.
    pub fn page_size(&self) -> usize {
        PAGE_SIZE
    }

    pub async fn read(&mut self, offset: usize, data: &mut [u8]) -> Result<(), TWIMError> {
        // TODO: Check that the offset + data.len() < total_size
        let offset = offset.to_be_bytes();
        self.periph
            .write_then_read(self.address, Some(&offset[..]), Some(data))
            .await
    }

    /// TODO: Support doing other things on the port while we wait for an ACK
    pub async fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), TWIMError> {
        let mut buf = [0u8; 2 + PAGE_SIZE];
        *array_mut_ref![buf, 0, 2] = (offset as u16).to_be_bytes();
        buf[2..(2 + data.len())].copy_from_slice(data);

        // TODO: Also need to change the WRITE PROTECT pin.

        self.periph.write(self.address, &buf).await?;

        // Wait for the device to acnkowledge a write to know
        // TODO: Have a maximum amount of time to do this.
        while let Err(_) = self.periph.write(self.address, &[]).await {
            // TODO: Sleep
            continue;
        }

        // TODO: wait for

        // TODO: Re-enable WRITE PROTECT.

        Ok(())
    }
}
