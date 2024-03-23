use std::thread::sleep;
use std::time::Duration;

use common::errors::*;
use parsing::binary::{be_i8, le_i16, le_u16, le_u24};
use peripherals::i2c::{I2CHostController, I2CHostDevice};

const DEVICE_ADDRESS: u8 = 0x77;
const CALIBRATION_START_OFFSET: u8 = 0x31;
const CALIBRATION_SIZE: usize = 21;

const OSC: u8 = 0x1c;
const PWR_CTRL: u8 = 0x1b;
const CONFIG: u8 = 0x1f;

/*
Burst read press_xlsb to temp_msb
- Each 24-bit unsigned

Need calibration co-efficients at 0x31 to 0x45 inclusive

OSR defaults to Standard resolution for pressure (4x oversampling)
- Temperature defaults to 1x

*/

// ~3.6 pascal per foot.

/*
Recommended for in-door
- oversampling: Ultra high resolution
- osrs_p: 16
- osrs_t: 2
- iir filter coeff: 4

- odr: 25hz
-

- Expect 5cm RMS noise
*/

pub struct BMP388 {
    device: I2CHostDevice,
    calibration_data: BMP388CalibrationData,
}

impl BMP388 {
    pub async fn open(host: I2CHostController) -> Result<Self> {
        let mut device = host.device(DEVICE_ADDRESS);

        let mut cal = [0u8; CALIBRATION_SIZE];
        device.write(&[CALIBRATION_START_OFFSET]).await?;
        device.read(&mut cal).await?;

        let mut input = &cal[..];
        let nvm_par_t1 = parse_next!(input, le_u16);
        let nvm_par_t2 = parse_next!(input, le_u16);
        let nvm_par_t3 = parse_next!(input, be_i8);
        let nvm_par_p1 = parse_next!(input, le_i16);
        let nvm_par_p2 = parse_next!(input, le_i16);
        let nvm_par_p3 = parse_next!(input, be_i8);
        let nvm_par_p4 = parse_next!(input, be_i8);
        let nvm_par_p5 = parse_next!(input, le_u16);
        let nvm_par_p6 = parse_next!(input, le_u16);
        let nvm_par_p7 = parse_next!(input, be_i8);
        let nvm_par_p8 = parse_next!(input, be_i8);
        let nvm_par_p9 = parse_next!(input, le_i16);
        let nvm_par_p10 = parse_next!(input, be_i8);
        let nvm_par_p11 = parse_next!(input, be_i8);
        assert_eq!(input.len(), 0);

        let calibration_data = BMP388CalibrationData {
            nvm_par_t1,
            nvm_par_t2,
            nvm_par_t3,
            nvm_par_p1,
            nvm_par_p2,
            nvm_par_p3,
            nvm_par_p4,
            nvm_par_p5,
            nvm_par_p6,
            nvm_par_p7,
            nvm_par_p8,
            nvm_par_p9,
            nvm_par_p10,
            nvm_par_p11,
        };

        println!("Calibration: {:?}", calibration_data);

        // Oversampling: 16x pressure, 2x temperature.
        device.write(&[OSC, 0b100 | (0b001 << 3)]).await?;

        // IIR coefficient
        device.write(&[CONFIG, 0b100 << 1]).await?;

        Ok(Self {
            device,
            calibration_data,
        })
    }

    pub async fn measure(&mut self) -> Result<Measurement> {
        const PRES_EN: u8 = 1 << 0;
        const TEMP_EN: u8 = 1 << 1;
        const FORCED_MODE: u8 = 0b01 << 4;
        self.device
            .write(&[PWR_CTRL, PRES_EN | TEMP_EN | FORCED_MODE])
            .await?;

        // 234 + press_en * (392 + 2^osr_p * 2000) + temp_en * (313 + 2^osr_t * 2000)
        // 234 + (392 + 16 * 2000) + (313 + 2 * 2000)

        // Wait for measurement to finish.
        // TODO: Calculate this time period based on the formula in the datasheet.
        sleep(Duration::from_millis(40));

        self.device.write(&[PWR_CTRL]).await?;
        let mut pwr_ctrl = [0u8; 1];
        self.device.read(&mut pwr_ctrl).await?;

        // Verify that the device has returned to sleep mode.
        // TODO: Check the STATUS register instead?
        if pwr_ctrl[0] & (0b11 << 4) != 0 {
            return Err(err_msg("Device still measuring"));
        }

        let mut status = [0u8; 1];
        self.device.write(&[0x03]).await?;
        self.device.read(&mut status).await?;

        if ((status[0] & (1 << 6)) == 0) || ((status[0] & (1 << 5)) == 0) {
            return Err(err_msg("Temperature/pressure not ready"));
        }

        // 24-bit pressure value followed by 24-bit temperature value.
        let mut data = [0u8; 6];
        self.device.write(&[0x04]).await?;
        self.device.read(&mut data).await?;

        let raw_pres = le_u24(&data[0..3]).unwrap().0;
        let raw_temp = le_u24(&data[3..]).unwrap().0;

        let temp = self.calibration_data.compensate_temperature(raw_temp);
        let pres = self.calibration_data.compensate_pressure(raw_pres, temp);

        Ok(Measurement {
            temperature: temp,
            pressure: pres,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Measurement {
    pub temperature: f64,
    pub pressure: f64,
}

#[derive(Debug)]
pub struct BMP388CalibrationData {
    nvm_par_t1: u16,
    nvm_par_t2: u16,
    nvm_par_t3: i8,
    nvm_par_p1: i16,
    nvm_par_p2: i16,
    nvm_par_p3: i8,
    nvm_par_p4: i8,
    nvm_par_p5: u16,
    nvm_par_p6: u16,
    nvm_par_p7: i8,
    nvm_par_p8: i8,
    nvm_par_p9: i16,
    nvm_par_p10: i8,
    nvm_par_p11: i8,
}

macro_rules! pow2 {
    ($exp:expr) => {
        2.0f64.powi($exp)
    };
}

impl BMP388CalibrationData {
    fn par_t1(&self) -> f64 {
        (self.nvm_par_t1 as f64) / pow2!(-8)
    }

    fn par_t2(&self) -> f64 {
        (self.nvm_par_t2 as f64) / pow2!(30)
    }

    fn par_t3(&self) -> f64 {
        (self.nvm_par_t3 as f64) / pow2!(48)
    }

    fn par_p1(&self) -> f64 {
        ((self.nvm_par_p1 as f64) - pow2!(14)) / pow2!(20)
    }

    fn par_p2(&self) -> f64 {
        ((self.nvm_par_p2 as f64) - pow2!(14)) / pow2!(29)
    }

    fn par_p3(&self) -> f64 {
        (self.nvm_par_p3 as f64) / pow2!(32)
    }

    fn par_p4(&self) -> f64 {
        (self.nvm_par_p4 as f64) / pow2!(37)
    }

    fn par_p5(&self) -> f64 {
        (self.nvm_par_p5 as f64) / pow2!(-3)
    }

    fn par_p6(&self) -> f64 {
        (self.nvm_par_p6 as f64) / pow2!(6)
    }

    fn par_p7(&self) -> f64 {
        (self.nvm_par_p7 as f64) / pow2!(8)
    }

    fn par_p8(&self) -> f64 {
        (self.nvm_par_p8 as f64) / pow2!(15)
    }

    fn par_p9(&self) -> f64 {
        (self.nvm_par_p9 as f64) / pow2!(48)
    }

    fn par_p10(&self) -> f64 {
        (self.nvm_par_p10 as f64) / pow2!(48)
    }

    fn par_p11(&self) -> f64 {
        (self.nvm_par_p11 as f64) / pow2!(65)
    }

    pub fn compensate_temperature(&self, raw_temp: u32) -> f64 {
        let partial_data1 = (raw_temp as f64) - self.par_t1();
        let partial_data2 = partial_data1 * self.par_t2();
        partial_data2 + (partial_data1 * partial_data1) * self.par_t3()
    }

    pub fn compensate_pressure(&self, raw_pres: u32, temp: f64) -> f64 {
        let partial_out1 = {
            let partial_data1 = self.par_p6() * temp;
            let partial_data2 = self.par_p7() * (temp * temp);
            let partial_data3 = self.par_p8() * (temp * temp * temp);
            self.par_p5() + partial_data1 + partial_data2 + partial_data3
        };

        let partial_out2 = {
            let partial_data1 = self.par_p2() * temp;
            let partial_data2 = self.par_p3() * (temp * temp);
            let partial_data3 = self.par_p4() * (temp * temp * temp);
            (raw_pres as f64) * (self.par_p1() + partial_data1 + partial_data2 + partial_data3)
        };

        let partial_data1 = (raw_pres as f64) * (raw_pres as f64);
        let partial_data2 = self.par_p9() * self.par_p10() * temp;
        let partial_data3 = partial_data1 * partial_data2;
        let partial_data4 = partial_data3
            + ((raw_pres as f64) * (raw_pres as f64) * (raw_pres as f64)) * self.par_p11();

        partial_out1 + partial_out2 + partial_data4
    }
}
