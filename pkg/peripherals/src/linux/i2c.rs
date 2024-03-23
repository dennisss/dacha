use std::os::unix::fs::FileExt;
use std::sync::Arc;
use std::{
    io::{Read, Write},
    os::unix::prelude::AsRawFd,
};

use common::errors::*;
use executor::sync::AsyncMutex;

/*
TODO: Use async files, but we should ensure that they aren't buffering the reads/writes.

https://www.kernel.org/doc/html/v5.5/i2c/dev-interface.html

All ioctl commands return -1 on error. 0 on success. Read values read the read value.
*/

mod linux {
    const I2C_IOC_MAGIC: u8 = 0x07;

    const I2C_IOC_TYPE_SLAVE: u8 = 0x03;

    ioctl_write_int!(i2c_set_peripheral_addr, I2C_IOC_MAGIC, I2C_IOC_TYPE_SLAVE);
}

pub struct I2CHostController {
    file: Arc<AsyncMutex<std::fs::File>>,
}

impl I2CHostController {
    pub fn open(path: &str) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        // TODO: Should be more specific about the frequency.

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0701 as libc::c_ulong, 0) };
        if r != 0 {
            return Err(err_msg("Failed to set retries"));
        }

        Ok(Self {
            file: Arc::new(AsyncMutex::new(file)),
        })
    }

    pub async fn test(&mut self, addr: u8) -> Result<bool> {
        let file = self.file.lock().await?.read_exclusive();

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0703 as libc::c_ulong, addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        // let r = unsafe { linux::i2c_set_peripheral_addr(self.file.as_raw_fd(), addr
        // as u64) }?; println!("SET ADDR RESULT: {}", r);

        // println!("READ");

        let mut data = [0u8; 1];
        let n = file.read_at(&mut data, 0)?;
        // println!("N: {}", n);

        Ok(true)
    }

    // TODO: Dedup with the Device function.
    pub async fn write(&mut self, addr: u8, data: &[u8]) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0703 as libc::c_ulong, addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        file.write_all_at(data, 0)?;

        Ok(())
    }

    // TODO: Dedup with the Device function.
    pub async fn read(&mut self, addr: u8, output: &mut [u8]) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0703 as libc::c_ulong, addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        file.read_exact_at(output, 0)?;

        Ok(())
    }

    pub fn device(&self, addr: u8) -> I2CHostDevice {
        I2CHostDevice {
            file: self.file.clone(),
            addr,
        }
    }
}

pub struct I2CHostDevice {
    file: Arc<AsyncMutex<std::fs::File>>,
    addr: u8,
}

impl I2CHostDevice {
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0703 as libc::c_ulong, self.addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        file.write_all_at(data, 0)?;

        Ok(())
    }

    pub async fn read(&mut self, output: &mut [u8]) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();

        let r = unsafe { libc::ioctl(file.as_raw_fd(), 0x0703 as libc::c_ulong, self.addr as u64) };
        if r != 0 {
            return Err(err_msg("Failed to set addr"));
        }

        file.read_exact_at(output, 0)?;

        Ok(())
    }
}
