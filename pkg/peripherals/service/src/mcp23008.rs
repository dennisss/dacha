

/*
https://ww1.microchip.com/downloads/aemDocuments/documents/APID/ProductDocuments/DataSheets/MCP23008-and-MCP23S08-Data-Sheet-DS20001919.pdf

8-bit register address.
*/

use std::sync::Arc;
use common::errors::*;

use crate::device::*;

// A0,A1,A2 are grounded.
const I2C_ADDRESS: u8 = 0x20;

const IODIR: u8 = 0x00;
const IPOL: u8 = 0x01;
const GPPU: u8 = 0x06;
const GPIO: u8 = 0x09;
const OLAT: u8 = 0x0A;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PinDirection {
    Input,
    Output 
}

#[derive(Debug, Clone, Copy)]
pub struct PinLevels {
    reg: u8,
}

impl PinLevels {
    pub fn get(&self, pin: usize) -> bool {
        base_util::bit_field::get_bit_field(self.reg as u32, pin as u32, 1) != 0
    }
}


pub struct MCP23008 {
    device: Arc<PeripheralsDevice>,
    i2c_periph_name: String, 
}

impl MCP23008 {
    pub fn create(device: Arc<PeripheralsDevice>, i2c_periph_name: &str) -> Self {
        Self {
            device,
            i2c_periph_name: i2c_periph_name.to_string()
        }
    }

    pub async fn set_direction(&self, pin: usize, is_input: bool) -> Result<()> {
        self.write_register_bit(IODIR, pin, is_input).await
    }

    pub async fn set_directions(&self, dirs: &[(usize, PinDirection)]) -> Result<()> {
        let bits = dirs.iter()
            .map(|(pin, dir)| (*pin, *dir == PinDirection::Input))
            .collect::<Vec<_>>();
        self.write_register_bits(IODIR, &bits).await
    }

    pub async fn set_pull_up(&self, pin: usize, enabled: bool) -> Result<()> {
        self.write_register_bit(GPPU, pin, enabled).await
    }

    pub async fn set_pull_ups(&self, up: &[(usize, bool)]) -> Result<()> {
        self.write_register_bits(GPPU, up).await
    }

    pub async fn set_level(&self, pin: usize, high: bool) -> Result<()> {
        self.write_register_bit(OLAT, pin, high).await
    }

    pub async fn set_levels(&self, levels: &[(usize, bool)]) -> Result<()> {
        self.write_register_bits(OLAT, levels).await
    }

    pub async fn read(&self) -> Result<PinLevels> {
        let reg = self.read_register(GPIO).await?;
        Ok(PinLevels { reg })
    }

    async fn write_register(&self, register_addr: u8, register_value: u8) -> Result<()> {
        let write_data = [register_addr, register_value];
        self.device.i2c_transfer(
            &self.i2c_periph_name,
            I2C_ADDRESS,
            &write_data,
            &mut []
        ).await?;

        Ok(())
    }

    async fn write_register_bit(&self, register_addr: u8, offset: usize, value: bool) -> Result<()> {
        self.write_register_bits(register_addr, &[(offset, value)]).await
    }

    async fn write_register_bits(&self, register_addr: u8, bits: &[(usize, bool)]) -> Result<()> {
        let mut reg = self.read_register(register_addr).await?;

        for (offset, value) in bits {
            reg = base_util::bit_field::set_bit_field(
                reg as u32, *offset as u32, 1, if *value { 1 } else { 0 }) as u8;
        }

        self.write_register(register_addr, reg).await
    }

    async fn read_register(&self, register_addr: u8) -> Result<u8> {
        let write_data = [register_addr];
        let mut read_data = [0];

        self.device.i2c_transfer(
            &self.i2c_periph_name,
            I2C_ADDRESS,
            &write_data,
            &mut read_data
        ).await?;
        
        Ok(read_data[0])
    }

    
}
