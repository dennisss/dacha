use std::{io::{Read, Write}, os::unix::prelude::AsRawFd};

use common::errors::*;

/*

https://www.kernel.org/doc/html/v5.5/i2c/dev-interface.html

All ioctl commands return -1 on error. 0 on success. Read values read the read value.
*/


mod linux {
    const I2C_IOC_MAGIC: u8 = 0x07;

    const I2C_IOC_TYPE_SLAVE: u8 = 0x03;

    ioctl_write_int!(i2c_set_peripheral_addr, I2C_IOC_MAGIC, I2C_IOC_TYPE_SLAVE);

}

pub struct I2CDevice {
    file: std::fs::File,
}

impl I2CDevice {

    pub fn open(path: &str) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0701 as libc::c_ulong, 0) };
        if r != 0 {
            return Err(err_msg("Failed to set retries"));
        }

        Ok(Self { file })
    }

    pub fn test(&mut self, addr: u8) -> Result<bool> {
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), 0x0703 as libc::c_ulong, addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        // let r = unsafe { linux::i2c_set_peripheral_addr(self.file.as_raw_fd(), addr as u64) }?;
        // println!("SET ADDR RESULT: {}", r);

        // println!("READ");

        let mut data = [0u8; 1];
        let n = self.file.read(&mut data)?;
        // println!("N: {}", n);

        Ok(true)
    }

    pub fn write(&mut self, addr: u8, data: &[u8]) -> Result<()> {
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), 0x0703 as libc::c_ulong, addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        self.file.write_all(data)?;

        Ok(())
    }

    pub fn read(&mut self, addr: u8, output: &mut [u8]) -> Result<()> {
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), 0x0703 as libc::c_ulong, addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        self.file.read_exact(output)?;

        Ok(())
    }

}