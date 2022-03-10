/*
Two notions:
1. Pins
    P0_00 to P0_31
    P1_00 to P1_15

2. GPIOPins
    => The GPIO port can allow trading a Pin object for a

    A pin is defined by:
        => Bit in OUT|OUTSET|OUTCLR registers
        => Bit in IN register
        => Bit in DIR|DIRSET|DIRCLR registers
        => Bit in LATCH register
        => Register in PIN_CNF[i]
*/

/*
Naming:
- drivers
- peripherals
- registers


utils::ceil_devs
*/

use peripherals::raw::p0::dirclr::DIRCLR_WRITE_VALUE;
use peripherals::raw::p0::dirset::DIRSET_WRITE_VALUE;
use peripherals::raw::p0::outclr::OUTCLR_WRITE_VALUE;
use peripherals::raw::p0::outset::OUTSET_WRITE_VALUE;
use peripherals::raw::p0::{P0, P0_REGISTERS};
use peripherals::raw::p1::P1;
use peripherals::raw::register::RegisterWrite;

pub use peripherals::raw::{PinDirection, PinLevel};

use crate::pins::{PeripheralPin, Port};

pub struct GPIO {
    p0: P0,
    p1: P1,
}

impl GPIO {
    pub fn new(p0: P0, p1: P1) -> Self {
        Self { p0, p1 }
    }

    /// TODO: &mut self should be needed as having the PeripheralPin should be
    /// sufficient to gurantee exclusivity.
    pub fn pin<P: PeripheralPin>(&mut self, p: P) -> GPIOPin {
        let port: &mut P0_REGISTERS = match p.port() {
            Port::P0 => &mut self.p0,
            Port::P1 => &mut self.p1,
        };

        GPIOPin {
            port: unsafe { core::mem::transmute(port) },
            pin_mask: 1u32 << p.pin(),
        }
    }
}

pub struct GPIOPin {
    port: &'static mut P0_REGISTERS,
    pin_mask: u32,
}

impl GPIOPin {
    pub fn set_direction(&mut self, dir: PinDirection) -> &mut Self {
        if dir == PinDirection::Output {
            self.port
                .dirset
                .write(DIRSET_WRITE_VALUE::from_raw(self.pin_mask));
        } else {
            self.port
                .dirclr
                .write(DIRCLR_WRITE_VALUE::from_raw(self.pin_mask));
        }

        self
    }

    pub fn write(&mut self, level: PinLevel) {
        if level == PinLevel::High {
            self.port
                .outset
                .write(OUTSET_WRITE_VALUE::from_raw(self.pin_mask));
        } else {
            self.port
                .outclr
                .write(OUTCLR_WRITE_VALUE::from_raw(self.pin_mask));
        }
    }
}
