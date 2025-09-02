use std::thread::sleep;
use std::time::Duration;

use common::errors::*;
use peripherals::i2c::{I2CHostController, I2CHostDevice};

const DEVICE_ADDRESS: u8 = 0x36;

/*
Up to 1MHz I2C clock
*/

pub struct AS5601 {
    device: I2CHostDevice,
}

define_bit_flags!(AS5601Status u8 {
    MAGNET_TOO_STRONG = 1 << 3,
    MAGNET_TOO_WEAK = 1 << 4,
    MAGNET_DETECTED = 1 << 5
});

impl AS5601 {
    pub async fn open(controller: &I2CHostController) -> Result<Self> {
        let mut device = controller.device(DEVICE_ADDRESS);
        
        /*
        let WD = 0;
        let FTH = 0;
        let SF = 0;
        let HYST = 0;
        let PM = 0b00; // Normal power mode
        let ABN = 0b1000; // 2048 positions (15.6kHz output rate)

        device.write(&[
            0x07, // Set address to start of CONF register.
            (WD << 5) | (FTH << 2) | SF << 0, // CONF (first byte)
            HYST << 2 | PM << 0, // CONF (second byte)
            ABN << 0, // ABN register
        ])
        */

        Ok(Self {
            device,
        })
    }

    pub async fn read_status(&mut self) -> Result<AS5601Status> {
        self.device.write(&[0x0B]).await?;
        let mut out = [0];
        self.device.read(&mut out).await?;

        Ok(AS5601Status::from_raw(out[0]))
    }

    // Ideally this value ends up in the middle of the range (128 / 2) for 3.3V operation.
    pub async fn read_agc(&mut self) -> Result<u8> {
        self.device.write(&[0x1A]).await?;
        let mut out = [0];
        self.device.read(&mut out).await?;
        Ok(out[0])
    }

    // This is a 12-bit value
    pub async fn read_raw_angle(&mut self) -> Result<u16> {
        self.device.write(&[0x0C]).await?;
        let mut out = [0, 0];
        self.device.read(&mut out).await?;

        Ok(u16::from_be_bytes(out))
    }
}


