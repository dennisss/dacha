
use std::sync::Arc;

use common::errors::*;
use peripherals_service::device::PeripheralsDevice;

pub struct MA732 {
    device: Arc<PeripheralsDevice>,
}

impl MA732 {
    pub fn new(device: Arc<PeripheralsDevice>) -> Self {
        Self {
            device
        }
    }

    /*
    async fn write_register(&self, addr: u8, value: u8) -> Result<()> {
        let mut buf = [0u8; 2];
        let n = self.device.spi_transfer("encoder_spi", &[
            

        ], &mut buf[..]).await?;
    }
    */

    /// Gets the measured angle of the magnet in degrees in [0, 360)
    pub async fn get_angle(&self) -> Result<f32> {
        let mut buf = [0u8; 2];
        let n = self.device.spi_transfer("encoder_spi", &[0, 0], &mut buf[..]).await?;
        assert_eq!(n, 2);

        let angle = (u16::from_be_bytes(buf) as f32) / ((u16::max_value() as f32) + 1.0);

        Ok(angle)
    }

    /*
    read_register:
    010 address (5 bits) 0x00

    then read read 16 bits 


    write register:
    100 address <value 8b-bit>
    - then repeated

    */

}
