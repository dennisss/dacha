/*
Raspberry Pi 4:
- BCM2711

Code examples:
- https://elinux.org/RPi_GPIO_Code_Samples#Direct_register_access
-

TODO: Verify that mutations to shared registers (with modify_register) don't happen concurrently
*/

use std::sync::Arc;

use common::errors::*;

use crate::memory::{MemoryBlock, GPIO_PERIPHERAL_OFFSET, GPIO_PERIPHERAL_SIZE};

const NUM_GPIO_PINS: usize = 58;

const REGISTER_SIZE: usize = std::mem::size_of::<u32>();
const REGISTER_BITS: usize = 32;

// lazy_static! {
//     static ref GPIO_SINGLETON: Result<GPIO> = {
//         GPIO::open()
//     };
// }

pub struct GPIO {
    mem: Arc<MemoryBlock>,
}

impl GPIO {
    pub fn open() -> Result<Self> {
        let mem = MemoryBlock::open_peripheral(GPIO_PERIPHERAL_OFFSET, GPIO_PERIPHERAL_SIZE)?;
        Ok(Self { mem: Arc::new(mem) })
    }

    pub fn pin(&self, number: usize) -> GPIOPin {
        assert!(number < NUM_GPIO_PINS);
        GPIOPin {
            mem: self.mem.clone(),
            number,
        }
    }
}

#[derive(Clone)]
pub struct GPIOPin {
    mem: Arc<MemoryBlock>,
    number: usize,
}

impl GPIOPin {
    pub fn number(&self) -> usize {
        self.number
    }

    pub fn set_mode(&mut self, mode: Mode) -> &mut Self {
        // Byte offset of the GPFSELn register.
        // GPFSEL0 is at offset 0 and there are 10 pins per register.
        let offset = (self.number / 10) * REGISTER_SIZE;

        // 3 bits per pin in the register.
        let bit_offset = (self.number % 10) * 3;

        let mask = !(0b111 << bit_offset);
        let bits = mode.to_value() << bit_offset;

        self.mem.modify_register(offset, |v| (v & mask) | bits);

        self
    }

    pub fn get_mode(&self) -> Mode {
        let offset = (self.number / 10) * REGISTER_SIZE;
        let bit_offset = (self.number % 10) * 3;

        let r = self.mem.read_register(offset);
        let bits = (r >> bit_offset) & 0b111;
        Mode::from_value(bits).unwrap()
    }

    pub fn write(&mut self, high: bool) -> &mut Self {
        // Offset to GPSET0|1 or GPCLR0|1
        let mut offset = if high { 0x1c } else { 0x28 };
        offset += (self.number / 32) * REGISTER_SIZE;

        let bit_offset = self.number % 32;

        // NOTE: Writes of 0-bits don't do anything to the other pins.
        self.mem.write_register(offset, 1 << bit_offset);

        self
    }

    pub fn read(&self) -> bool {
        // Offset to GPLEV0
        self.read_pin_bits(0x34, 1) != 0
    }

    pub fn set_resistor(&mut self, resistor: Resistor) -> &mut Self {
        // GPIO_PUP_PDN_CNTRL_REG0....
        // 2-bits per pin.
        let mut offset = 0xe4;
        offset += (self.number / 16) * REGISTER_SIZE;

        let bit_offset = (self.number % 16) * 2;

        let mask = !(0b11 << bit_offset);
        let bits = resistor.to_value() << bit_offset;

        self.mem.modify_register(offset, |v| (v & mask) | bits);

        self
    }

    pub fn get_resistor(&self) -> Resistor {
        let bits = self.read_pin_bits(0xe4, 2);
        Resistor::from_value(bits).unwrap()
    }

    // NOTE: Attempting to to monitor gpio events directly will crash raspbian.
    /*
    pub fn event_detected(&self) -> bool {
        // Offset to GPEDS0. GPEDS1 is at 0x44
        // 1-bit per pin
        self.read_pin_bits(0x40, 1) != 0
    }

    pub fn clear_event_detected(&mut self) {
        let mut offset = 0x40;
        offset += (self.number / 32) * REGISTER_SIZE;

        let bit_offset = self.number % 32;

        // Writing a 1 clears the bit.
        self.mem.write_register(offset, 1 << bit_offset);
    }

    /// Configures the complete set of events that should be monitored for this
    /// pin.
    pub fn watch_events(&mut self, events: EventType) {
        let has_val = |e: EventType| {
            if events.contains(e) {
                1
            } else {
                0
            }
        };

        // GPREN0 / GPREN1
        self.modify_pin_bits(0x4c, 1, has_val(EventType::RISING_EDGE));

        // GPFEN0 / GPFEN1
        self.modify_pin_bits(0x58, 1, has_val(EventType::FALLING_EDGE));

        // GPHEN0 / GPHEN1
        self.modify_pin_bits(0x64, 1, has_val(EventType::HIGH));

        // GPLEN0 / GPLEN1
        self.modify_pin_bits(0x70, 1, has_val(EventType::HIGH));
    }
    */

    fn read_pin_bits(&self, mut offset: usize, bits_per_pin: usize) -> u32 {
        offset += ((self.number * bits_per_pin) / REGISTER_BITS) * REGISTER_SIZE;

        let bit_offset = (self.number * bits_per_pin) % REGISTER_BITS;
        let mask = (1 << bits_per_pin) - 1;

        let reg = self.mem.read_register(offset);

        (reg >> bit_offset) & mask
    }

    fn modify_pin_bits(&mut self, mut offset: usize, bits_per_pin: usize, value: u32) {
        offset += ((self.number * bits_per_pin) / REGISTER_BITS) * REGISTER_SIZE;

        let bit_offset = (self.number * bits_per_pin) % REGISTER_BITS;
        let mask = (1 << bits_per_pin) - 1;
        let shifted_mask = mask << bit_offset;

        self.mem.modify_register(offset, |v| {
            (v & !shifted_mask) | ((value << bit_offset) & shifted_mask)
        });
    }
}

enum_def!(Mode u32 =>
    Input = 0b000,
    Output = 0b001,
    AltFn0 = 0b100,
    AltFn1 = 0b101,
    AltFn2 = 0b110,
    AltFn3 = 0b111,
    AltFn4 = 0b011,
    AltFn5 = 0b010
);

enum_def!(Resistor u32 =>
    None = 0b00,
    PullUp = 0b01,
    PullDown = 0b10,
    Reserved = 0b11
);

define_bit_flags!(EventType u8 {
    RISING_EDGE = 1 << 0,
    FALLING_EDGE = 1 << 1,
    HIGH = 1 << 2,
    LOW = 1 << 3
});
