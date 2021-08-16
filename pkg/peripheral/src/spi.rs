use std::os::unix::prelude::AsRawFd;

/*

Linux Definitions:
https://github.com/torvalds/linux/blob/5bfc75d92efd494db37f5c4c173d3639d4772966/include/uapi/linux/spi/spidev.h

Raspberry Pi Example:
https://raw.githubusercontent.com/raspberrypi/linux/rpi-3.10.y/Documentation/spi/spidev_test.c

*/

mod linux {
    const SPI_IOC_MAGIC: u8 = b'k';

    const SPI_IOC_TYPE_MESSAGE: u8 = 0; // &[spi_ioc_transfer]
    const SPI_IOC_TYPE_MODE: u8 = 1; // u8
    const SPI_IOC_TYPE_LSB_FIRST: u8 = 2; //
    const SPI_IOC_TYPE_BITS_PER_WORD: u8 = 3; // u8
    const SPI_IOC_TYPE_MAX_SPEED_HZ: u8 = 4; // u32
    const SPI_IOC_TYPE_MODE32: u8 = 5;

    #[derive(Default)]
    #[repr(C)]
    pub struct spi_ioc_transfer {
        pub tx_buf: u64,
        pub rx_buf: u64,

        pub len: u32,
        speed_hz: u32,

        delay_usecs: u16,
        bits_per_word: u8,
        cs_change: u8,
        tx_nbits: u8,
        rx_nbits: u8,
        word_delay_usecs: u8,
        pad: u8,
    }

    ioctl_read!(spi_read_mode, SPI_IOC_MAGIC, SPI_IOC_TYPE_MODE, u8);

    // ioctl_read!(spi_read_mode, SPI_IOC_MAGIC, SPI_IOC_TYPE_MODE, u8);

    ioctl_write_buf!(
        spi_transfer,
        SPI_IOC_MAGIC,
        SPI_IOC_TYPE_MESSAGE,
        spi_ioc_transfer
    );
}

trait SPI {
    fn transfer(&mut self, send: &[u8], receive: &mut [u8]) -> std::result::Result<(), ()>;
}

pub struct SPIDevice {
    file: std::fs::File,
}

impl SPIDevice {
    pub fn open() -> std::io::Result<Self> {
        // TODO: Set as a non-blocking open
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/spidev1.1")?;

        Ok(Self { file })
    }
}

/*
impl SPI for SPIDevice {
    fn transfer(&mut self, send: &[u8], receive: &mut [u8]) -> std::result::Result<(), ()> {

        // typical speed: 500000

        let mut transfers = [
            linux::spi_ioc_transfer::default(),
            linux::spi_ioc_transfer::default()
        ];

        transfers[0].tx_buf = unsafe { std::mem::transmute(send.as_ptr()) };
        transfers[0].len = send.len() as u32;

        transfers[1].rx_buf = unsafe { std::mem::transmute(receive.as_mut_ptr()) };
        transfers[1].len = receive.len() as u32;

        unsafe { linux::spi_transfer(self.file.as_raw_fd(), &transfers) }.unwrap();

        Ok(())
    }
}
*/
