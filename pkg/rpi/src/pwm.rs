/*

Want to use PWM mode (not serialiser mode)
- Submode: MSEN=1 M/S

- Need to set M = DAT1
- Need to set S = RNG1

- 100MHz clock

- Therfore 4000 should be the range?

We care about PWM0_0

PWM0 is at 0x7e20c000
- 40 byte blcok

PWM1 is at 0x7e20c800

Can check clocks setup state with `sudo cat /sys/kernel/debug/clk/clk_summary`

In order for the PWM clock to be configured, we must add "dtoverlay=pwm" to the end of the "/boot/config.txt" file

Overlays are documented here:
- https://github.com/raspberrypi/linux/blob/18141488040d6dea071911110275260e387ea4bd/arch/arm/boot/dts/overlays/README#L2381

Info on making custom overlays:
- https://jumpnowtek.com/rpi/Using-the-Raspberry-Pi-Hardware-PWM-timers.html

alternative solution is to /sys/class/pwm/pwmchip0
- https://jumpnowtek.com/rpi/Using-the-Raspberry-Pi-Hardware-PWM-timers.html
- For this, the user needs to be in the 'gpio' group.

*/

use common::async_std::fs;
use common::async_std::path::{Path, PathBuf};
use common::errors::*;

use crate::gpio::*;
use crate::memory::{MemoryBlock, PWM0_PERIPHERAL_OFFSET};

// Sysfs directory for the driver for PWM channel 0 on the Raspberry Pi.
const SYS_PWM_DIR: &str = "/sys/class/pwm/pwmchip0";

const NANOS_PER_SECOND: usize = 1000000000;

struct PWMPinSpec {
    number: usize,
    mode: Mode,
    controller: usize,
    channel: usize,
}

// Valid PWM pin combinations pulled from section 8.5 of the BCM2711 (RPI 4B)
// peripherals doc.
const PIN_SPECS: &[PWMPinSpec] = &[
    PWMPinSpec {
        number: 12,
        mode: Mode::AltFn0,
        controller: 0,
        channel: 0,
    },
    PWMPinSpec {
        number: 13,
        mode: Mode::AltFn0,
        controller: 0,
        channel: 1,
    },
    PWMPinSpec {
        number: 18,
        mode: Mode::AltFn5,
        controller: 0,
        channel: 0,
    },
    PWMPinSpec {
        number: 19,
        mode: Mode::AltFn5,
        controller: 0,
        channel: 1,
    },
    PWMPinSpec {
        number: 40,
        mode: Mode::AltFn0,
        controller: 1,
        channel: 0,
    },
    PWMPinSpec {
        number: 41,
        mode: Mode::AltFn0,
        controller: 1,
        channel: 1,
    },
    PWMPinSpec {
        number: 45,
        mode: Mode::AltFn0,
        controller: 0,
        channel: 1,
    },
];

pub struct DirectPWM {
    mem: MemoryBlock,
    gpio: GPIO,
}

impl DirectPWM {
    pub fn open() -> Result<Self> {
        // TODO: Automatically apply the FExxxxxx part
        let mut mem = MemoryBlock::open_peripheral(PWM0_PERIPHERAL_OFFSET, 40)?;

        let gpio = GPIO::open()?;

        let gpio_pin = gpio
            .pin(12)
            .set_mode(Mode::AltFn0)
            .set_resistor(Resistor::PullUp);

        // CTL = PWEN1 | MSEN1
        mem.write_register(0x00, 1 | (1 << 7));

        // RNG1
        mem.write_register(0x10, 4000);

        // DAT1
        mem.write_register(0x14, 200);

        Ok(Self { mem, gpio })
    }
}

pub struct SysPWM {
    channel_dir: PathBuf,
    pin: GPIOPin,
}

impl SysPWM {
    pub async fn open(mut pin: GPIOPin) -> Result<Self> {
        let pin_spec = PIN_SPECS
            .iter()
            .find(|s| s.number == pin.number())
            .ok_or_else(|| format_err!("PWM not supported by pin: {}", pin.number()))?;

        if pin_spec.controller != 0 {
            return Err(err_msg(
                "Only PWM controller 0 supported with sys fs driver",
            ));
        }

        if !Path::new(SYS_PWM_DIR).exists().await {
            return Err(err_msg("PWM sys fs driver not detected."));
        }

        // Export channel 0.
        let export_path = Path::new(SYS_PWM_DIR).join("export");
        fs::write(export_path, format!("{}\n", pin_spec.channel)).await?;

        let channel_dir = Path::new(SYS_PWM_DIR).join(format!("pwm{}", pin_spec.channel));
        if !channel_dir.exists().await {
            return Err(format_err!(
                "Failed to export PWM channel {}",
                pin_spec.channel
            ));
        }

        pin.set_mode(pin_spec.mode);

        Ok(Self { channel_dir, pin })
    }

    /// Configures the current PWM value
    ///
    /// frequency: Frequency of the square wave in Hz
    /// duty_cycle: Percentage of the time the square wave should be up (from
    /// 0.0 to 1.0).
    pub async fn write(&mut self, frequency: f32, duty_cycle: f32) -> Result<()> {
        // Convert to nanoseconds.
        let period = ((NANOS_PER_SECOND as f32) / frequency) as usize;
        let duty_cycle = ((period as f32) * duty_cycle) as usize;

        fs::write(self.channel_dir.join("period"), format!("{}\n", period)).await?;
        fs::write(
            self.channel_dir.join("duty_cycle"),
            format!("{}\n", duty_cycle),
        )
        .await?;
        fs::write(self.channel_dir.join("enable"), "1\n").await?;
        Ok(())
    }
}
