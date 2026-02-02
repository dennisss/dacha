use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use common::InRange;
use peripherals_service::device::PeripheralsDevice;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnclosureSide {
    Left,
    Right
}

impl EnclosureSide {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right"
        }
    }
}

pub struct PSUVoltages {
    pub v12: f32,
    pub v5: f32,
    pub ps_on: f32,
}

impl PSUVoltages {
    pub fn waiting_for_power_on(&self) -> bool {
        self.ps_on.in_range(4.0, 6.0)
    }

    pub fn output_stable(&self) -> bool {
        self.v12.in_range(11.0, 13.0) && self.v5.in_range(4.0, 6.0)
    }
}


pub struct ManagementDevice {
    device: Arc<PeripheralsDevice>,
}

impl ManagementDevice {
    pub async fn create() -> Result<Self> {
        let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

        let config = configs.remove(&"jbod_management")
            .ok_or_else(|| err_msg("No config with the given name"))?;

        let (mut device, _) = PeripheralsDevice::create(&config).await?;

        Ok(Self {
            device: Arc::new(device)
        })
    }

    // Helper for scripts to turn on the PSUs with appropriate checks.
    pub async fn power_on(&mut self) -> Result<()> {
        for side in [EnclosureSide::Left, EnclosureSide::Right] {
            let v = self.read_psu_voltages(side).await?;
            if !v.waiting_for_power_on() {
                return Err(format_err!("PSU {:?} not attached", side));
            }

            self.toggle_psu_power(side, true).await?;

            executor::sleep(Duration::from_secs(1)).await?;

            let v = self.read_psu_voltages(side).await?;
            if !v.output_stable() {
                return Err(format_err!("PSU {:?} outputs not stable", side));
            }
        }

        Ok(())
    }

    // Helper for scripts to turn off the PSUs.
    pub async fn power_off(&mut self) -> Result<()> {
        for side in [EnclosureSide::Left, EnclosureSide::Right] {
            self.toggle_psu_power(side, false).await?;
        }

        Ok(())
    }


    pub async fn read_psu_voltages(&self, side: EnclosureSide) -> Result<PSUVoltages> {
        let v5 = self.device.analog_read(match side {
            EnclosureSide::Left => "5v_l_sense",
            EnclosureSide::Right => "5v_r_sense",
        }).await?;

        let v12 = self.device.analog_read(match side {
            EnclosureSide::Left => "12v_l_sense",
            EnclosureSide::Right => "12v_r_sense",
        }).await?;

        let ps_on = self.device.analog_read(match side {
            EnclosureSide::Left => "ps_on_l_sense",
            EnclosureSide::Right => "ps_on_r_sense",
        }).await?;

        Ok(PSUVoltages {
            v5,
            v12,
            ps_on
        })
    }

    pub async fn toggle_sas_power(&self, side: EnclosureSide, on: bool) -> Result<()> {
        self.device.gpio_write(match side {
            EnclosureSide::Left => "sas_on_l",
            EnclosureSide::Right => "sas_on_r",
        }, on).await?;
        Ok(())
    }

    pub async fn toggle_psu_power(&self, side: EnclosureSide, on: bool) -> Result<()> {
        self.device.gpio_write(match side {
            EnclosureSide::Left => "ps_on_l",
            EnclosureSide::Right => "ps_on_r",
        }, !on).await?;
        Ok(())
    }

    pub fn num_leds(&self) -> usize {
        15 * 4
    }

    pub async fn set_led_data(&self, data: &[u8]) -> Result<()> {
        self.device.neopixel_transfer("led", 0, &data).await?;
        self.device.neopixel_show("led").await
    }

    pub async fn set_fan_speed(&self, speed: f32) -> Result<()> {
        self.device.pwm_write("fan_pwm", speed).await
    }

    pub async fn get_fan_speeds(&self) -> Result<Vec<f32>> {

        let mut periph_names = vec![];
        for i in 0..6 {
            periph_names.push(format!("fan{}_tach", i + 1))
        }

        self.device.read_tachometers(&periph_names).await
    }

}
