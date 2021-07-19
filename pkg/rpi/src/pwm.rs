

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
*/

use common::errors::*;

use crate::gpio::*;
use crate::memory::MemoryBlock;

struct PWMPinSpec {
    number: usize,
    mode: Mode,
    controller: usize,
    channel: usize,
}

const PIN_SPECS: &[PWMPinSpec] = &[
    PWMPinSpec {
        number: 12,
        mode: Mode::AltFn0,
        controller: 0,
        channel: 0
    }
];

pub struct PWM {
    mem: MemoryBlock,
    gpio: GPIO
}

impl PWM {

    pub fn open() -> Result<Self> {

        // TODO: Automatically apply the FExxxxxx part
        let mem = MemoryBlock::open(0xFE20c000, 40)?;

        let gpio = GPIO::open()?;

        let gpio_pin = gpio.pin(12)
            .set_mode(Mode::AltFn0)
            .set_resistor(Resistor::PullUp);

        // CTL = PWEN1 | MSEN1
        mem.write_register(0x00, 1 | (1 << 7));

        // RNG1
        mem.write_register(0x10, 4000);

        // DAT1
        mem.write_register(0x14, 200);
        

        Ok(Self {
            mem,
            gpio
        })
    }

}