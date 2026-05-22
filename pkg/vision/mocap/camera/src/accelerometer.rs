use common::errors::*;
use peripherals::i2c::*;

#[derive(Debug, Clone, Copy)]
pub struct Acceleration {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub struct LIS2DW12 {
    i2c: I2CHostDevice,
}

impl LIS2DW12 {
    const REG_WHO_AM_I: u8 = 0x0F;
    const REG_CTRL1: u8 = 0x20;
    const REG_CTRL2: u8 = 0x21;
    const REG_CTRL6: u8 = 0x25;
    const REG_OUT_X_L: u8 = 0x28;

    const EXPECTED_WHO_AM_I: u8 = 0x44;

    // Sensitivity factor for +/-2g in High-Performance mode (0.244 mg/digit)
    const SENSITIVITY_G: f32 = 0.000244;

    /// Initializes the LIS2DW12 device, verifies the chip ID, and configures it 
    /// for high-quality static measurement (High-Performance, Low-Noise, +/-2g).
    pub async fn create(mut i2c: I2CHostDevice) -> Result<Self> {
        // 1. Verify Chip ID
        i2c.write(&[Self::REG_WHO_AM_I]).await?;
        let mut who_am_i = [0u8; 1];
        i2c.read(&mut who_am_i).await?;

        if who_am_i[0] != Self::EXPECTED_WHO_AM_I {
            return Err(format_err!(
                "Invalid WHO_AM_I register value. Expected 0x{:02X}, found 0x{:02X}",
                Self::EXPECTED_WHO_AM_I,
                who_am_i[0]
            ));
        }

        // 2. Configure CTRL1 (0x20): ODR = 12.5 Hz (0010), High-Performance mode (01), LP_MODE = 00
        // Resulting byte: 0x24
        i2c.write(&[Self::REG_CTRL1, 0x24]).await?;

        // 3. Configure CTRL2 (0x21): Block Data Update (BDU) = 1, Auto-increment (IF_ADD_INC) = 1
        // Resulting byte: 0x0C
        i2c.write(&[Self::REG_CTRL2, 0x0C]).await?;

        // 4. Configure CTRL6 (0x25): BW_FILT = 00 (ODR/2), FS = 00 (±2g), FDS = 0 (Low-pass), LOW_NOISE = 1
        // Resulting byte: 0x04
        i2c.write(&[Self::REG_CTRL6, 0x04]).await?;

        Ok(Self { i2c })
    }

    /// Reads the current X, Y, Z acceleration data from the device.
    /// Returns the acceleration vector with units in standard gravity (g).
    pub async fn read_acceleration(&mut self) -> Result<Acceleration> {
        // Auto-increment is enabled via IF_ADD_INC in CTRL2, so writing the starting 
        // register address (OUT_X_L) allows bursting all 6 bytes.
        self.i2c.write(&[Self::REG_OUT_X_L]).await?;

        let mut buf = [0u8; 6];
        self.i2c.read(&mut buf).await?;

        // In 14-bit resolution modes (like High-Performance), data is stored as a 
        // two's complement 16-bit word, left-justified (the 2 least significant bits are 0).
        // A right arithmetic shift (>>) by 2 correctly adjusts the value while preserving the sign.
        let raw_x = i16::from_le_bytes([buf[0], buf[1]]) >> 2;
        let raw_y = i16::from_le_bytes([buf[2], buf[3]]) >> 2;
        let raw_z = i16::from_le_bytes([buf[4], buf[5]]) >> 2;

        Ok(Acceleration {
            x: (raw_x as f32) * Self::SENSITIVITY_G,
            y: (raw_y as f32) * Self::SENSITIVITY_G,
            z: (raw_z as f32) * Self::SENSITIVITY_G,
        })
    }
}