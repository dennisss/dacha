use core::arch::asm;

use peripherals::raw::{PinDirection, PinLevel};

use crate::gpio::GPIOPin;

/*
74HC595 Operation
- Set the serial data pin
- Wait 125ns
- Set SRCLK high.
- Hold SRCLK high for 100ns
- Set SRCLK low
    - Must now wait another 100ns before changing SRCLK again

- Set RCLK high 100ns after SRCLK

- So 100ns is around 7 clock cycles of the NRF52840
    - Although may need to wait 300ns for a signal to propagate to outputs or to the 9'th bit of the first register.

- If the clocks are tied then the outputs are one clock cycle ahead of the storage register.
*/

pub struct ShiftRegister {
    serial_data: GPIOPin,

    /// Normally held low.
    serial_clk: GPIOPin,

    /// Normally held low.
    register_clk: GPIOPin,
}

impl ShiftRegister {
    pub fn new(
        mut serial_data: GPIOPin,
        mut serial_clk: GPIOPin,
        mut register_clk: GPIOPin,
    ) -> Self {
        serial_data
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);
        serial_clk
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);
        register_clk
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);

        Self {
            serial_data,
            serial_clk,
            register_clk,
        }
    }

    pub fn shift_bit(&mut self, level: PinLevel) {
        self.serial_data.write(level);
        Self::wait_300ns();

        // Bit is sampled on rising edge of clock.
        self.serial_clk.write(PinLevel::High);
        Self::wait_300ns();

        self.serial_clk.write(PinLevel::Low);
        Self::wait_300ns();
    }

    pub fn show_bits(&mut self) {
        self.register_clk.write(PinLevel::High);
        Self::wait_300ns();
        self.register_clk.write(PinLevel::Low);
        Self::wait_300ns();
        // NOTE: We don't wait here as the user should always call shift_bit
        // before the next call to show_bits.
    }

    fn wait_300ns() {
        for i in 0..100 {
            unsafe { asm!("nop") };
        }
    }
}
