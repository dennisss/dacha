/*
Raspberry Pi 4:
- BCM2711

Code examples:
- https://elinux.org/RPi_GPIO_Code_Samples#Direct_register_access

Accessing peripheral address using bcm_host_get_peripheral_address():
https://www.raspberrypi.org/documentation/hardware/raspberrypi/peripheral_addresses.md

-
*/

use std::ffi::CStr;

use common::errors::*;
use libc::c_void;

use crate::memory::MemoryBlock;

// NOTE: 0x7eXXXXXX virtual addresses map to 0x20XXXXXX

// TODO: Can we use /dev/gpiomem for this?

// Highest 32-bit GPIO register is at offset 0xF0
// TODO: Eventually change this at runtime depending on the CPU used.
const GPIO_REGISTER_BASE: u32 = 0xFE200000;
const GPIO_REGISTER_BLOCK_SIZE: usize = 244;

const NUM_GPIO_PINS: usize = 58;

const REGISTER_SIZE: usize = std::mem::size_of::<u32>();

// lazy_static! {
//     static ref GPIO_SINGLETON: Result<GPIO> = {
//         GPIO::open()
//     };
// }

pub struct GPIO {
    mem: MemoryBlock,
}

impl GPIO {
    pub fn open() -> Result<Self> {
        let mem = MemoryBlock::open(GPIO_REGISTER_BASE, GPIO_REGISTER_BLOCK_SIZE)?;
        Ok(Self { mem })
    }

    pub fn pin(&self, number: usize) -> GPIOPin {
        assert!(number < NUM_GPIO_PINS);
        GPIOPin {
            peripheral: self,
            number,
        }
    }
}

pub struct GPIOPin<'a> {
    peripheral: &'a GPIO,
    number: usize,
}

impl<'a> GPIOPin<'a> {
    pub fn set_mode(&self, mode: Mode) -> &Self {
        // Byte offset of the GPFSELn register.
        // GPFSEL0 is at offset 0 and there are 10 pins per register.
        let offset = (self.number / 10) * REGISTER_SIZE;

        // 3 bits per pin in the register.
        let bit_offset = (self.number % 10) * 3;

        let mask = !(0b111 << bit_offset);
        let bits = mode.to_value() << bit_offset;

        self.peripheral
            .mem
            .modify_register(offset, |v| (v & mask) | bits);

        self
    }

    pub fn get_mode(&self) -> Mode {
        let offset = (self.number / 10) * REGISTER_SIZE;
        let bit_offset = (self.number % 10) * 3;

        let r = self.peripheral.mem.read_register(offset);
        let bits = (r >> bit_offset) & 0b111;
        Mode::from_value(bits).unwrap()
    }

    pub fn write(&self, high: bool) -> &Self {
        // Offset to GPSET0|1 or GPCLR0|1
        let mut offset = if high { 0x1c } else { 0x28 };
        offset += (self.number / 32) * REGISTER_SIZE;

        let bit_offset = self.number % 32;

        // NOTE: Writes of 0-bits don't do anything to the other pins.
        self.peripheral.mem.write_register(offset, 1 << bit_offset);

        self
    }

    pub fn read(&self) -> bool {
        // Offset to GPLEV0
        let mut offset = 0x34;
        offset += (self.number / 32) * REGISTER_SIZE;

        let bit_offset = self.number % 32;

        let reg = self.peripheral.mem.read_register(offset);

        ((reg >> bit_offset) & 1) != 0
    }

    pub fn set_resistor(&self, resistor: Resistor) -> &Self {
        // GPIO_PUP_PDN_CNTRL_REG0....
        // 2-bits per pin.
        let mut offset = 0xe4;
        offset += (self.number / 16) * REGISTER_SIZE;

        let bit_offset = (self.number % 16) * 2;

        let mask = !(0b11 << bit_offset);
        let bits = resistor.to_value() << bit_offset;

        self.peripheral
            .mem
            .modify_register(offset, |v| (v & mask) | bits);

        self
    }

    pub fn get_resistor(&self) -> Resistor {
        let mut offset = 0xe4;
        offset += (self.number / 16) * REGISTER_SIZE;

        let bit_offset = (self.number % 16) * 2;

        let reg = self.peripheral.mem.read_register(offset);

        let bits = (reg >> bit_offset) & 0b11;

        Resistor::from_value(bits).unwrap()
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
    AltFn6 = 0b010
);

enum_def!(Resistor u32 =>
    None = 0b00,
    PullUp = 0b01,
    PullDown = 0b10,
    Reserved = 0b11
);
