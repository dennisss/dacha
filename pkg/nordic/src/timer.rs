use core::arch::asm;
use core::ops::{Deref, DerefMut, Drop};
use core::pin::Pin;

use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::timer0::{TIMER0, TIMER0_REGISTERS};
use peripherals::raw::timer4::TIMER4;
use peripherals::raw::{Interrupt, InterruptState, PinDirection, EventRegister};

use crate::pins::{connect_pin, disconnect_pin, is_pin_connected, PeripheralPin};

pub struct Timer {
    periph: TIMER0,
    interrupt: Interrupt,
    // TODO: Instead use a bit mask
    used_channels: usize,
    total_channels: usize,
}

impl Timer {
    pub fn new(mut periph: TIMER0) -> Self {
        periph.mode.write_timer();
        periph.prescaler.write(0); // 16 MHz
        periph.bitmode.write_32bit();
        periph.tasks_start.write_trigger();

        Self { periph, interrupt: Interrupt::TIMER0, used_channels: 0, total_channels: 4 }
    }

    pub fn reset(&mut self) {
        self.used_channels = 0;
    }

    pub fn new_channel(&mut self) -> Option<TimerChannel> {
        if self.used_channels + 1 > self.total_channels {
            return None;
        }

        // TODO: Auto de-allocate channels once the channels are dropped.
        let index = self.used_channels;
        self.used_channels += 1;

        Some(TimerChannel { periph: unsafe { TIMER0::new() }, index })
    }

    pub fn capture(&mut self) -> Option<u32> {
        if self.used_channels + 1 > self.total_channels {
            return None;
        }

        let i = self.used_channels;
        self.periph.tasks_capture[i].write_trigger();
        Some(self.periph.cc[i].read())
    }
}

pub struct TimerChannel {
    periph: TIMER0,
    index: usize,
}

impl TimerChannel {

    pub fn set_compare_value(&mut self, value: u32) {
        self.periph.cc[self.index].write(value);
    }

    pub fn capture(&mut self) -> u32 {
        self.periph.tasks_capture[self.index].write_trigger();
        self.periph.cc[self.index].read()
    }

    pub fn pending_event(&mut self) -> bool {
        let mut pending = false;
        
        if self.periph.events_compare[self.index].read().is_generated() {
            self.periph.events_compare[self.index].write_notgenerated();
            crate::events::flush_events_clear();
            pending = true;
        }


        pending
    }

    pub fn compare_event(&self) -> &EventRegister {
        &self.periph.events_compare[self.index]
    }

    pub fn enable_interrupt(&mut self) {
        let i = self.index;
        self.periph.intenset.write_with(|v| {
            match i {
                0 => v.set_compare0(),
                1 => v.set_compare1(),
                2 => v.set_compare2(),
                3 => v.set_compare3(),
                4 => v.set_compare4(),
                5 => v.set_compare5(),
                _ => panic!()
            }
        });
    }

    pub fn disable_interrupt(&mut self) {
        let i = self.index;
        self.periph.intenclr.write_with(|v| {
            match i {
                0 => v.set_compare0(),
                1 => v.set_compare1(),
                2 => v.set_compare2(),
                3 => v.set_compare3(),
                4 => v.set_compare4(),
                5 => v.set_compare5(),
                _ => panic!()
            }
        });

        crate::events::flush_events_clear();
    }
}
