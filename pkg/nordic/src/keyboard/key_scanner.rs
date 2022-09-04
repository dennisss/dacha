use usb::hid::StandardKeyboardInputReport;

use peripherals::raw::PinLevel;

use crate::gpio::GPIOPin;
use crate::keyboard::mappings::*;
use crate::keyboard::shift_register::ShiftRegister;

pub struct KeyScanner {
    pub(super) x_register: ShiftRegister,

    pub(super) y_inputs: [GPIOPin; KEY_ROWS],
}

impl KeyScanner {
    pub async fn scan(&mut self) -> StandardKeyboardInputReport {
        let mut report = StandardKeyboardInputReport::default();

        // Clear the shift register with all zeros.
        for _ in 0..KEY_COLS {
            self.x_register.shift_bit(PinLevel::Low);
        }

        // Shift a '1' bit.
        self.x_register.shift_bit(PinLevel::High);

        for x in 0..KEY_COLS {
            self.x_register.show_bits();

            for y in 0..KEY_ROWS {
                if self.y_inputs[y].read() == PinLevel::High {
                    let id = y * KEY_COLS + KEY_COLUMN_ORDER[x] + 1;
                    log!("id: ", id as u32);
                    if let Some(usage) = key_id_to_usage(id) {
                        report.add_pressed_key(usage);
                    }
                }
            }

            // Shift a '0' so that the single '1' in the register moves down by 1.
            self.x_register.shift_bit(PinLevel::Low);
        }

        report
    }
}
