use core::arch::asm;
use core::ops::{Deref, DerefMut, Drop};
use core::pin::Pin;

use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::timer0::{TIMER0, TIMER0_REGISTERS};
use peripherals::raw::timer4::TIMER4;
use peripherals::raw::{Interrupt, InterruptState, PinDirection, EventRegister};

use crate::pins::{connect_pin, disconnect_pin, is_pin_connected, PeripheralPin};


// TODO: Codegen this.
pub struct TIMERx {
    base_address: u32,
    interrupt: Interrupt,
    total_channels: usize,
}

impl Deref for TIMERx {
    type Target = TIMER0_REGISTERS;

    fn deref(&self) -> &Self::Target {
        unsafe { ::core::mem::transmute(self.base_address) }
    }
}

impl DerefMut for TIMERx {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { ::core::mem::transmute(self.base_address) }
    }
}

macro_rules! timerx_from {
    ($t:ident, $i:ident, $n:expr) => {
        impl From<$t> for TIMERx {
            fn from(mut value: $t) -> Self {
                TIMERx {
                    base_address: unsafe {
                        core::mem::transmute::<&mut TIMER0_REGISTERS, u32>(value.deref_mut())
                    },
                    interrupt: Interrupt::$i,
                    total_channels: $n,
                }
            }
        }
    };
}

timerx_from!(TIMER0, TIMER0, 4);
timerx_from!(TIMER4, TIMER4, 6);


pub struct Timer {
    periph: TIMERx,
    // TODO: Instead use a bit mask
    used_channels: usize,
}

impl Timer {
    pub fn new(mut periph: TIMERx) -> Self {
        periph.mode.write_timer();
        periph.prescaler.write(0); // 16 MHz
        periph.bitmode.write_32bit();
        periph.tasks_start.write_trigger();

        Self { periph, used_channels: 0 }
    }

    pub fn reset(&mut self) {
        self.used_channels = 0;
    }

    pub fn new_channel(&mut self) -> Option<TimerChannel> {
        if self.used_channels + 1 > self.periph.total_channels {
            return None;
        }

        // TODO: Auto de-allocate channels once the channels are dropped.
        let index = self.used_channels;
        self.used_channels += 1;

        Some(TimerChannel { periph: unsafe { TIMER0::new() }, index })
    }

    pub fn capture(&mut self) -> Option<u32> {
        if self.used_channels + 1 > self.periph.total_channels {
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
