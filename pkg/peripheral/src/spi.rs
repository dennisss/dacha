use std::os::unix::prelude::AsRawFd;

use common::errors::*;

/*

Linux Definitions:
https://github.com/torvalds/linux/blob/5bfc75d92efd494db37f5c4c173d3639d4772966/include/uapi/linux/spi/spidev.h

Raspberry Pi Example:
https://raw.githubusercontent.com/raspberrypi/linux/rpi-3.10.y/Documentation/spi/spidev_test.c

Mode flags are defined here:
- https://github.com/torvalds/linux/blob/5bfc75d92efd494db37f5c4c173d3639d4772966/include/uapi/linux/spi/spi.h

*/

#[cfg(target_pointer_width = "32")]
type MemoryPointer = u32;

#[cfg(target_pointer_width = "64")]
type MemoryPointer = u64;

mod linux {
    const SPI_IOC_MAGIC: u8 = b'k';

    const SPI_IOC_TYPE_MESSAGE: u8 = 0; // &[spi_ioc_transfer]
    const SPI_IOC_TYPE_MODE: u8 = 1; // u8
    const SPI_IOC_TYPE_LSB_FIRST: u8 = 2; // TODO
    const SPI_IOC_TYPE_BITS_PER_WORD: u8 = 3; // u8
    const SPI_IOC_TYPE_MAX_SPEED_HZ: u8 = 4; // u32
    const SPI_IOC_TYPE_MODE32: u8 = 5; // TODO

    #[derive(Default)]
    #[repr(C)]
    pub struct spi_ioc_transfer {
        pub tx_buf: u64,
        pub rx_buf: u64,

        pub len: u32,
        pub speed_hz: u32,

        delay_usecs: u16,
        pub bits_per_word: u8,
        cs_change: u8,
        tx_nbits: u8,
        rx_nbits: u8,
        word_delay_usecs: u8,
        pad: u8,
    }

    ioctl_write_buf!(
        spi_transfer,
        SPI_IOC_MAGIC,
        SPI_IOC_TYPE_MESSAGE,
        spi_ioc_transfer
    );

    ioctl_read!(spi_read_mode, SPI_IOC_MAGIC, SPI_IOC_TYPE_MODE, u8);
    ioctl_write_ptr!(spi_write_mode, SPI_IOC_MAGIC, SPI_IOC_TYPE_MODE, u8);

    ioctl_read!(
        spi_read_bits_per_word,
        SPI_IOC_MAGIC,
        SPI_IOC_TYPE_BITS_PER_WORD,
        u8
    );
    ioctl_write_ptr!(
        spi_write_bits_per_word,
        SPI_IOC_MAGIC,
        SPI_IOC_TYPE_BITS_PER_WORD,
        u8
    );

    ioctl_read!(
        spi_read_max_speed_hz,
        SPI_IOC_MAGIC,
        SPI_IOC_TYPE_MAX_SPEED_HZ,
        u32
    );
    ioctl_write_ptr!(
        spi_write_max_speed_hz,
        SPI_IOC_MAGIC,
        SPI_IOC_TYPE_MAX_SPEED_HZ,
        u32
    );
}

pub trait SPI {
    /// Half duplex transfer which first sends out all bytes in 'send' and
    /// immediately afterwards starts receiving bytes into the 'receive' buffer.
    fn transfer(&mut self, send: &[u8], receive: &mut [u8]) -> Result<()>;
}

pub struct SPIDevice {
    file: std::fs::File,
    bits_per_word: u8,
    speed_hz: u32,
}

impl SPIDevice {
    pub fn open(path: &str) -> Result<Self> {
        // TODO: Set as a non-blocking open
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        let mode = 0;
        let bits_per_word = 8;
        let speed = 1000000; // 1MHz (default)
        let fd = file.as_raw_fd();
        unsafe { linux::spi_write_mode(fd, &mode) }?;
        unsafe { linux::spi_write_max_speed_hz(fd, &speed) }?;
        unsafe { linux::spi_write_bits_per_word(fd, &bits_per_word) }?;

        Ok(Self {
            file,
            bits_per_word,
            speed_hz: speed,
        })
    }

    pub fn set_speed_hz(&mut self, speed_hz: u32) -> Result<()> {
        let fd = self.file.as_raw_fd();
        unsafe { linux::spi_write_max_speed_hz(fd, &speed_hz) }?;
        self.speed_hz = speed_hz;
        Ok(())
    }

    pub fn set_mode(&mut self, mode: u8) -> Result<()> {
        let fd = self.file.as_raw_fd();
        unsafe { linux::spi_write_mode(fd, &mode) }?;
        Ok(())
    }
}

impl SPI for SPIDevice {
    fn transfer(&mut self, send: &[u8], receive: &mut [u8]) -> Result<()> {
        let mut transfers = [
            linux::spi_ioc_transfer::default(),
            linux::spi_ioc_transfer::default(),
        ];

        // First transfer sends, but ignores any received bytes.
        transfers[0].tx_buf =
            unsafe { std::mem::transmute::<_, MemoryPointer>(send.as_ptr()) } as u64;
        transfers[0].len = send.len() as u32;
        transfers[0].bits_per_word = self.bits_per_word;
        transfers[0].speed_hz = self.speed_hz;

        // Second transfer sends zeros and receives data from the device.
        transfers[1].rx_buf =
            unsafe { std::mem::transmute::<_, MemoryPointer>(receive.as_mut_ptr()) } as u64;
        transfers[1].len = receive.len() as u32;
        transfers[1].bits_per_word = self.bits_per_word;
        transfers[1].speed_hz = self.speed_hz;

        let transfers = if receive.len() == 0 {
            &transfers[0..1]
        } else {
            &transfers
        };

        unsafe { linux::spi_transfer(self.file.as_raw_fd(), transfers) }?;

        Ok(())
    }
}
