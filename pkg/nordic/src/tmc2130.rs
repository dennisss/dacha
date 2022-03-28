use crate::spi::SPIHost;

/*
SPI Parameters
- 4MHz
- SPI MOde 3
- Chip select active low
- MSB transferred first

- Data gram (MCU to chip)
    - 1 address byte
        - High bit 1 => Write
    - 4 data bytes
- Data gam (chip to MCU)
    - Responds to previous datagram from host
    - 1 byte SPI_STATUs
    - 4 byte data

- Basically send big endian in data.

*/

pub struct TMC2130 {
    spi: SPIHost,
}

impl TMC2130 {
    pub fn new(spi: SPIHost) -> Self {
        Self { spi }
    }

    pub async fn write_register(&mut self, addr: u8, value: u32) {
        let mut write = [0u8; 5];
        let mut read = [0u8; 5];

        write[0] = addr | (1 << 7);

        *array_mut_ref![write, 1, 4] = value.to_be_bytes();

        self.spi.transfer(&write, &mut []).await;
    }

    pub async fn read_register(&mut self, addr: u8) -> u32 {
        let mut write = [0u8; 5];
        let mut read = [0u8; 5];

        write[0] = addr;

        self.spi.transfer(&write, &mut []).await;
        self.spi.transfer(&[], &mut read).await;

        u32::from_be_bytes(*array_ref![read, 1, 4])
    }
}
