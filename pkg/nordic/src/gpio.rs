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

use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::p0::dirclr::DIRCLR_WRITE_VALUE;
use peripherals::raw::p0::dirset::DIRSET_WRITE_VALUE;
use peripherals::raw::p0::outclr::OUTCLR_WRITE_VALUE;
use peripherals::raw::p0::outset::OUTSET_WRITE_VALUE;
use peripherals::raw::p0::pin_cnf::{DIR_FIELD, INPUT_FIELD, PULL_FIELD, DRIVE_FIELD};
use peripherals::raw::p0::{P0, P0_REGISTERS};
use peripherals::raw::p1::P1;

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
            Port::P0 => &mut *self.p0,
            Port::P1 => &mut *self.p1,
        };

        GPIOPin {
            port: unsafe { core::mem::transmute(port) },
            port_index: p.port() as u32,
            pin_index: p.pin() as usize,
            pin_mask: 1u32 << p.pin(),
            // handle: p.into(),
        }
    }
}

pub struct GPIOPin {
    port: &'static mut P0_REGISTERS,
    port_index: u32,
    pin_index: usize,
    pin_mask: u32,
    // /// NOTE: This is only used if we want to get the raw pin reference back.
    // handle: PeripheralPinHandle,
}

// TODO: Think more about whether or not having this is a good idea.
impl PeripheralPin for GPIOPin {
    fn port(&self) -> Port {
        match self.port_index {
            0 => Port::P0,
            1 => Port::P1,
            _ => panic!()
        }
    }
    fn pin(&self) -> u8 {
        self.pin_index as u8
    }
}

#[derive(Clone, Copy)]
pub enum Resistor {
    None,
    PullDown,
    PullUp,
}

impl GPIOPin {
    /// Resets the pin back to an initial state (no pull up/down, no drive, no
    /// input buffer).
    pub fn reset(&mut self) -> &mut Self {
        self.port.pin_cnf[self.pin_index].write_with(|v| v.set_input(INPUT_FIELD::Disconnect));
        self.write(PinLevel::Low);
        self
    }

    pub fn set_direction(&mut self, dir: PinDirection) -> &mut Self {
        let mut pin_cnf = self.port.pin_cnf[self.pin_index].read();

        if dir == PinDirection::Output {
            // self.port
            //     .dirset
            //     .write(DIRSET_WRITE_VALUE::from_raw(self.pin_mask));

            pin_cnf.set_dir(DIR_FIELD::Output);
            pin_cnf.set_input(INPUT_FIELD::Disconnect);
        } else {
            // self.port
            //     .dirclr
            //     .write(DIRCLR_WRITE_VALUE::from_raw(self.pin_mask));

            pin_cnf.set_dir(DIR_FIELD::Input);
            pin_cnf.set_input(INPUT_FIELD::Connect);
        }

        self.port.pin_cnf[self.pin_index].write(pin_cnf);

        self
    }

    pub fn set_resistor(&mut self, value: Resistor) -> &mut Self {
        let mut pin_cnf = self.port.pin_cnf[self.pin_index].read();
        pin_cnf.set_pull(match value {
            Resistor::None => PULL_FIELD::Disabled,
            Resistor::PullDown => PULL_FIELD::Pulldown,
            Resistor::PullUp => PULL_FIELD::Pullup,
        });
        self.port.pin_cnf[self.pin_index].write(pin_cnf);
        self
    }

    pub fn set_high_drive(&mut self, on: bool) -> &mut Self {
        let mut pin_cnf = self.port.pin_cnf[self.pin_index].read();
        pin_cnf.set_drive(if on {
            DRIVE_FIELD::H0H1
        } else {
            DRIVE_FIELD::S0S1
        });
        self.port.pin_cnf[self.pin_index].write(pin_cnf);
        self
    }

    pub fn write(&mut self, level: PinLevel) {
        self.write_bool(level == PinLevel::High);
    }

    pub fn write_bool(&mut self, level: bool) {
        if level {
            self.port
                .outset
                .write(OUTSET_WRITE_VALUE::from_raw(self.pin_mask));
        } else {
            self.port
                .outclr
                .write(OUTCLR_WRITE_VALUE::from_raw(self.pin_mask));
        }
    }

    pub fn read(&mut self) -> PinLevel {
        let v = self.port.r#in.read().to_raw() & self.pin_mask;
        if v != 0 {
            PinLevel::High
        } else {
            PinLevel::Low
        }
    }
}
