use std::thread::sleep;
use std::time::Duration;

use common::errors::*;
use peripherals::i2c::I2CDevice;

const DEVICE_ADDRESS: u8 = 0x58;

const WORD_LEN: usize = 2;
const PACKED_WORD_LEN: usize = 3;

/*
Up to 400kHz I2C

16-bit address space.

MSB first

Communication with the device is not allowed while it is measuring something.


For call Init_air_quality
Then call Measure_air_quality in regular intervals of 1 second.
    - first CO2 first and TVOC
    - During first 15 seconds after Init_air_quality, it will still be in an init phase and return (400, 0) values.

*/

#[derive(Debug)]
pub struct AirQuality {
    pub co2_ppm: u16,
    pub tvoc_ppb: u16,
}

pub struct SGP30 {
    device: I2CDevice,
}

impl SGP30 {
    pub fn open(device: I2CDevice) -> Self {
        Self { device }
    }

    pub fn init_air_quality(&mut self) -> Result<()> {
        self.device.write(DEVICE_ADDRESS, &[0x20, 0x03])?;

        sleep(Duration::from_millis(10));

        Ok(())
    }

    pub fn measure_air_quality(&mut self) -> Result<AirQuality> {
        self.device.write(DEVICE_ADDRESS, &[0x20, 0x08])?;

        sleep(Duration::from_millis(12));

        let mut data = [0u8; 2 * PACKED_WORD_LEN];
        self.device.read(DEVICE_ADDRESS, &mut data)?;

        let mut out = [0u8; 2 * WORD_LEN];
        Self::unpack_response(&data, &mut out)?;

        Ok(AirQuality {
            co2_ppm: u16::from_be_bytes(*array_ref![out, 0, 2]),
            tvoc_ppb: u16::from_be_bytes(*array_ref![out, 2, 2]),
        })
    }

    pub fn get_baseline(&mut self) -> Result<AirQuality> {
        self.device.write(DEVICE_ADDRESS, &[0x20, 0x15])?;

        sleep(Duration::from_millis(12));

        let mut data = [0u8; 2 * PACKED_WORD_LEN];
        self.device.read(DEVICE_ADDRESS, &mut data)?;

        let mut out = [0u8; 2 * WORD_LEN];
        Self::unpack_response(&data, &mut out)?;

        Ok(AirQuality {
            co2_ppm: u16::from_be_bytes(*array_ref![out, 0, 2]),
            tvoc_ppb: u16::from_be_bytes(*array_ref![out, 2, 2]),
        })
    }

    pub fn set_baseline(&mut self, baseline: &AirQuality) -> Result<()> {
        let mut data = [0u8; 8];
        data[0] = 0x20;
        data[1] = 0x1e;

        let word = array_mut_ref![data, 2, 2];
        *word = baseline.co2_ppm.to_be_bytes();
        data[4] = crc8(word);

        let word = array_mut_ref![data, 5, 2];
        *word = baseline.tvoc_ppb.to_be_bytes();
        data[7] = crc8(word);

        self.device.write(DEVICE_ADDRESS, &data)?;

        sleep(Duration::from_millis(10));
        Ok(())
    }

    pub fn get_feature_set_version(&mut self) -> Result<[u8; 2]> {
        self.device.write(DEVICE_ADDRESS, &[0x20, 0x2f])?;

        let mut data = [0u8; 1 * PACKED_WORD_LEN];
        self.device.read(DEVICE_ADDRESS, &mut data)?;

        let mut out = [0u8; 1 * WORD_LEN];
        Self::unpack_response(&data, &mut out)?;

        Ok(out)
    }

    pub fn get_serial(&mut self) -> Result<[u8; 6]> {
        self.device.write(DEVICE_ADDRESS, &[0x36, 0x82])?;
        sleep(Duration::from_micros(500)); // t_idle = 0.5ms

        let mut data = [0u8; 3 * PACKED_WORD_LEN];
        self.device.read(DEVICE_ADDRESS, &mut data)?;

        let mut out = [0u8; 3 * WORD_LEN];
        Self::unpack_response(&data, &mut out)?;

        Ok(out)
    }

    // pub fn get_feature_set_version

    fn unpack_response(input: &[u8], output: &mut [u8]) -> Result<()> {
        assert_eq!(input.len() % PACKED_WORD_LEN, 0);
        assert_eq!(output.len() % WORD_LEN, 0);
        assert_eq!(input.len() / PACKED_WORD_LEN, output.len() / WORD_LEN);

        let mut input_i = 0;
        let mut output_i = 0;
        while input_i < input.len() {
            let input_word = &input[input_i..(input_i + WORD_LEN)];
            let input_sum = input[input_i + WORD_LEN];
            if crc8(input_word) != input_sum {
                return Err(err_msg("Bad checksum"));
            }

            output[output_i..(output_i + WORD_LEN)].copy_from_slice(input_word);

            input_i += PACKED_WORD_LEN;
            output_i += WORD_LEN;
        }

        Ok(())
    }
}

fn crc8(data: &[u8]) -> u8 {
    const INIT_REMAINDER: u8 = 0xff;
    const FINAL_XOR: u8 = 0x00;
    const POLYNOMIAL: u8 = 0x31; // x^8 + x^5 + x^4 + 1

    let mut state: u8 = INIT_REMAINDER;
    for byte in data {
        state ^= *byte;
        for _ in 0..8 {
            let overflow = state & (1 << 7) != 0;
            state <<= 1;
            if overflow {
                state ^= POLYNOMIAL;
            }
        }
    }

    state ^ FINAL_XOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_test() {
        assert_eq!(crc8(&[0xBE, 0xEF]), 0x92);
    }
}
