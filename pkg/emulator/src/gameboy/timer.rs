use crate::gameboy::clock::{Clock, CYCLES_PER_SECOND};
use crate::gameboy::memory::{InterruptState, InterruptType, MemoryInterface};
use common::errors::*;
use std::cell::RefCell;
use std::rc::Rc;

// TODO: We currently assume that all frequencies divide the system clock
// frequency (this also implies that we should be able to optimize the modulus
// operator below to use bit testing instead of division).

const DIVIDER_TICK: u64 = CYCLES_PER_SECOND / 16384;

const TIMER_TICKS: &'static [u64; 4] = &[
    CYCLES_PER_SECOND / 4096,
    CYCLES_PER_SECOND / 262144,
    CYCLES_PER_SECOND / 65536,
    CYCLES_PER_SECOND / 16384,
];

// TODO: Dedup
fn bitget(v: u8, bit: u8) -> bool {
    if v & (1 << bit) != 0 {
        true
    } else {
        false
    }
}

pub struct Timer {
    clock: Rc<RefCell<Clock>>,
    interrupts: Rc<RefCell<InterruptState>>,

    /// FF04 (R/w)
    divider: u8,
    /// FF05 (R/W)
    counter: u8,
    /// FF06 (R/W)
    modulo: u8,
    /// FF07 (R/W)
    control: u8,
}

impl Timer {
    pub fn new(clock: Rc<RefCell<Clock>>, interrupts: Rc<RefCell<InterruptState>>) -> Self {
        Self {
            clock,
            interrupts,
            divider: 0,
            counter: 0,
            modulo: 0,
            control: 0,
        }
    }

    pub fn step(&mut self) -> Result<()> {
        let now = self.clock.borrow().cycles;
        if now % DIVIDER_TICK == 0 {
            self.divider = self.divider.wrapping_add(1);
        }

        // If timer running
        if bitget(self.control, 2) {
            let interval = TIMER_TICKS[(self.control & 0b11) as usize];
            if now % interval == 0 {
                if self.counter == 0xff {
                    //					println!("Fire timer");
                    self.interrupts.borrow_mut().trigger(InterruptType::Timer);
                    self.counter = self.modulo;
                } else {
                    // TODO: Instead do a safe overflowing add above and check
                    // for carry.
                    self.counter += 1;
                }
            }
        }

        Ok(())
    }
}

impl MemoryInterface for Timer {
    fn store8(&mut self, addr: u16, value: u8) -> Result<()> {
        match addr {
            0xff04 => {
                self.divider = 0;
            }
            // TODO: Does this get reset to zero on write?
            0xff05 => {
                self.counter = value;
            }
            0xff06 => {
                self.modulo = value;
            }
            0xff07 => {
                /* println!("Control timer {:0X}", value); */
                self.control = value;
            }
            _ => {
                return Err(err_msg("Unsupported address"));
            }
        }

        Ok(())
    }

    fn load8(&mut self, addr: u16) -> Result<u8> {
        Ok(match addr {
            0xff04 => self.divider,
            // TODO: Does this get reset to zero on write?
            0xff05 => self.counter,
            0xff06 => self.modulo,
            0xff07 => self.control,
            _ => {
                return Err(err_msg("Unsupported address"));
            }
        })
    }
}
