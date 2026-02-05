use std::os::unix::fs::FileExt;
use std::sync::Arc;
use std::{
    io::{Read, Write},
    os::unix::prelude::AsRawFd,
};

use common::errors::*;
use executor::sync::AsyncMutex;
use sys::bindings::{I2C_RETRIES, I2C_SLAVE, I2C_SLAVE_FORCE};
use sys::ioctl;

/*
TODO: Use async files, but we should ensure that they aren't buffering the reads/writes.

https://www.kernel.org/doc/html/v5.5/i2c/dev-interface.html

All ioctl commands return -1 on error. 0 on success. Read values read the read value.
*/

pub struct I2CHostController {
    file: Arc<AsyncMutex<std::fs::File>>,
}

#[derive(Clone, Copy)]
pub enum I2CTestResponse {
    Available,
    KernelClaimed,
}

impl I2CHostController {
    pub fn open(path: &str) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        // TODO: Should be more specific about the frequency.

        if let Err(e) = unsafe { ioctl(file.as_raw_fd(), I2C_RETRIES, 0) } {
            return Err(err_msg("Failed to set retries"));
        }

        Ok(Self {
            file: Arc::new(AsyncMutex::new(file)),
        })
    }

    pub async fn test(&mut self, addr: u8) -> Result<I2CTestResponse> {
        let file = self.file.lock().await?.read_exclusive();

        match unsafe { ioctl(file.as_raw_fd(), sys::bindings::I2C_SLAVE, addr as u64) } {
            Ok(_) => {}
            Err(sys::Errno::EBUSY) => {
                return Ok(I2CTestResponse::KernelClaimed);
            }
            Err(_) => {
                return Err(err_msg("Failed to set addr"));
            }
        }

        let mut data = [0u8; 1];
        let n = file.read_at(&mut data, 0)?;

        Ok(I2CTestResponse::Available)
    }

    pub async fn write(&mut self, addr: u8, data: &[u8]) -> Result<()> {
        self.device(addr).write(data).await
    }

    pub async fn read(&mut self, addr: u8, output: &mut [u8]) -> Result<()> {
        self.device(addr).read(output).await
    }

    pub fn device(&self, addr: u8) -> I2CHostDevice {
        I2CHostDevice {
            file: self.file.clone(),
            addr,
            force: false,
        }
    }
}

pub struct I2CHostDevice {
    file: Arc<AsyncMutex<std::fs::File>>,
    addr: u8,
    force: bool,
}

impl I2CHostDevice {

    /// Configures if operations should be forced.
    /// When 'forced', operations will not fail by the kernel has locked the address.
    pub fn set_force(&mut self, force: bool) {
        self.force = force;
    }

    fn configure_addr(&self, file: &std::fs::File) -> Result<()> {
        let cmd = if self.force { I2C_SLAVE_FORCE } else { I2C_SLAVE };
        if let Err(e) = unsafe { ioctl(file.as_raw_fd(), sys::bindings::I2C_SLAVE, self.addr as u64) } {
            return Err(format_err!("Failed to set i2c addr: {}", e));
        }
        
        Ok(())
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();

        self.configure_addr(&file)?;
        file.write_all_at(data, 0)?;

        Ok(())
    }

    pub async fn read(&mut self, output: &mut [u8]) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();

        self.configure_addr(&file)?;
        file.read_exact_at(output, 0)?;

        Ok(())
    }
}
