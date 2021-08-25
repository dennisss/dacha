use common::errors::*;

use crate::spi::{SPIDevice, SPI};

/*
MX25L25645GMI
- SPI Mode 0 (or Mode 3)
- MSB first
- Max speed of 50Mhz for regular read operations.
- Capacity: 33,554,432 x 8 bits
- 3 or 4 byte memory addresses supported.

- How to enable writing
    - Ensure the WP# pin is set to a high logic level (to disable write protection).
    - Send the WREN command
    - Set BP0-BP3 bits to zero
    - Follow figure 25 in datasheet
*/

const MACRONIX_MANUFACTURER_ID: u8 = 0xC2;

const READ_COMMAND_ID: u8 = 0x03;
const REMS_COMMAND_ID: u8 = 0x90;
const RDID_COMMAND_ID: u8 = 0x9F;

const EN4B_COMMAND_ID: u8 = 0xB7;

/// Interface for reading/writing a flash memory chip.
///
/// NOTE: Currently only the MX25L25645GMI is supported.
pub struct FlashChip {
    spi: SPIDevice,
}

impl FlashChip {
    pub fn open(mut spi: SPIDevice) -> Result<Self> {
        const DUMMY: u8 = 0;

        {
            let send = &[RDID_COMMAND_ID];

            // Response is [manufacter id, memory type, memory density]
            let mut receive = [0u8; 3];
            spi.transfer(send, &mut receive)?;
            assert_eq!(&receive[..], &[MACRONIX_MANUFACTURER_ID, 0x20, 0x19]);
        }

        // TODO: Check this.
        {
            // Send the manufacter id at index 0 first followed by the device id.
            let addr = 0;

            let send = &[REMS_COMMAND_ID, DUMMY, DUMMY, addr];

            // Response is the manufacturer id and device id
            let mut receive = [0u8; 2];
            spi.transfer(send, &mut receive)?;
            assert_eq!(&receive[..], &[MACRONIX_MANUFACTURER_ID, 0x18]);
        }

        Ok(Self { spi })
    }

    pub fn capacity(&self) -> usize {
        33554432
    }

    pub fn read_all(&mut self) -> Result<Vec<u8>> {
        // TODO: Must check status register to ensure ready (See section 8 on page 13)
        // RDSR: 0x05 command and read 1 byte status register.

        // Enter 4-byte mode
        self.spi.transfer(&[EN4B_COMMAND_ID], &mut [])?;

        let mut buf = vec![0u8; self.capacity()];

        const PAGE_SIZE: usize = 4096;
        assert_eq!(buf.len() % PAGE_SIZE, 0);

        for i in 0..(self.capacity() / PAGE_SIZE) {
            println!("{} / {}", i + 1, buf.len() / PAGE_SIZE);

            let addr = PAGE_SIZE * i;

            let mut send = [0u8; 5];
            send[0] = READ_COMMAND_ID;
            send[1..].copy_from_slice(&(addr as u32).to_be_bytes());

            let receive = &mut buf[addr..(addr + PAGE_SIZE)];
            self.spi.transfer(&send, receive)?;
        }

        Ok(buf)
    }
}
