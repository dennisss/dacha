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
use executor::interrupts::wait_for_irq;
use peripherals::raw::gpiote::GPIOTE;
use peripherals::raw::p0::dirclr::DIRCLR_WRITE_VALUE;
use peripherals::raw::p0::dirset::DIRSET_WRITE_VALUE;
use peripherals::raw::p0::outclr::OUTCLR_WRITE_VALUE;
use peripherals::raw::p0::outset::OUTSET_WRITE_VALUE;
use peripherals::raw::p0::pin_cnf::{DIR_FIELD, INPUT_FIELD, PULL_FIELD};
use peripherals::raw::p0::{P0, P0_REGISTERS};
use peripherals::raw::p1::P1;
use peripherals::raw::Interrupt;

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
            port_index: match p.port() {
                Port::P0 => 0,
                Port::P1 => 1,
            },
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

    pub fn read(&mut self) -> PinLevel {
        let v = self.port.r#in.read().to_raw() & self.pin_mask;
        if v != 0 {
            PinLevel::High
        } else {
            PinLevel::Low
        }
    }
}

pub struct GPIOInterrupts {
    periph: GPIOTE,
    num_used_channels: usize,
}

// impl Drop for GPIOInterrupts {
//     fn drop(&mut self) {}
// }

impl GPIOInterrupts {
    pub fn new(periph: GPIOTE) -> Self {
        // TODO: Do initial cleanup of everything in the peripheral (so that it can be
        // re-used)

        Self {
            periph,
            num_used_channels: 0,
        }
    }

    // TODO: Move somewhere like the drop.
    pub fn reset(&mut self) {
        // Disable all pins
        for i in 0..self.periph.config.len() {
            self.periph.config[i].write_with(|v| v.set_mode_with(|v| v.set_disabled()));
        }

        // Disable all interrupts.
        self.periph.intenclr.write_with(|v| {
            v.set_in0()
                .set_in1()
                .set_in2()
                .set_in3()
                .set_in4()
                .set_in5()
                .set_in6()
                .set_in7()
        });

        // Clear all events.
        self.pending_events();

        // TODO: Clear the NVIC interrupt.
    }

    pub fn into_inner(self) -> GPIOTE {
        self.periph
    }

    pub fn setup_interrupt(
        &mut self,
        pin: GPIOPin,
        polarity: GPIOInterruptPolarity,
    ) -> GPIOInterrupt {
        let channel = self.num_used_channels;
        self.num_used_channels += 1;

        self.periph.intenset.write_with(|v| match channel {
            0 => v.set_in0(),
            1 => v.set_in1(),
            2 => v.set_in2(),
            3 => v.set_in3(),
            4 => v.set_in4(),
            5 => v.set_in5(),
            6 => v.set_in6(),
            7 => v.set_in7(),
            _ => panic!(),
        });

        self.periph.config[channel].write_with(|v| {
            v.set_port(pin.port_index as u32)
                .set_psel(pin.pin_index as u32)
                .set_polarity_with(|v| match polarity {
                    GPIOInterruptPolarity::RisingEdge => v.set_lotohi(),
                    GPIOInterruptPolarity::FallingEdge => v.set_hitolo(),
                    GPIOInterruptPolarity::Toggle => v.set_toggle(),
                })
                .set_mode_with(|v| v.set_event())
        });

        GPIOInterrupt {
            pin,
            // inst: self,
            mask: GPIOInterruptMask {
                value: 1 << channel,
            },
        }
    }

    pub fn pending_events(&mut self) -> GPIOInterruptMask {
        let mut value = 0;
        for i in 0..self.num_used_channels {
            if self.periph.events_in[i].read().is_generated() {
                self.periph.events_in[i].write_notgenerated();
                value |= 1 << i;
            }
        }

        crate::events::flush_events_clear();

        GPIOInterruptMask { value }
    }

    pub async fn wait_for_interrupts(&mut self) -> GPIOInterruptMask {
        wait_for_irq(Interrupt::GPIOTE).await;
        self.pending_events()
    }
}

// TODO: Disable the interrupt on drop.
pub struct GPIOInterrupt {
    // inst: &'a GPIOInterrupts,
    mask: GPIOInterruptMask,
    pin: GPIOPin,
}

impl GPIOInterrupt {
    pub fn take_pin(self) -> GPIOPin {
        self.pin
    }

    pub fn mask(&self) -> GPIOInterruptMask {
        self.mask
    }
}

#[derive(Clone, Copy)]
pub enum GPIOInterruptPolarity {
    RisingEdge,
    FallingEdge,
    Toggle,
}

#[derive(Clone, Copy)]
pub struct GPIOInterruptMask {
    value: u8,
}

impl GPIOInterruptMask {
    pub fn contains(&self, other: GPIOInterruptMask) -> bool {
        self.value & other.value == other.value
    }
}
