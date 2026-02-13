use core::arch::asm;
use core::ops::{Deref, DerefMut, Drop};
use core::pin::Pin;

use common::register::{RegisterRead, RegisterWrite};
use executor::critical_mutex::CriticalMutex;
use executor::lock;
use peripherals::raw::timer0::{TIMER0, TIMER0_REGISTERS};
use peripherals::raw::timer0::intenset::INTENSET_WRITE_VALUE;
use peripherals::raw::timer0::intenclr::INTENCLR_WRITE_VALUE;
use peripherals::raw::timer1::TIMER1;
use peripherals::raw::timer2::TIMER2;
use peripherals::raw::timer3::TIMER3;
use peripherals::raw::timer4::TIMER4;
use peripherals::raw::{Interrupt, InterruptState, PinDirection, EventRegister};
use peripherals::raw::TaskRegister;

use crate::pins::{connect_pin, disconnect_pin, is_pin_connected, PeripheralPin};


// TODO: Codegen this.
pub struct TIMERx {
    base_address: u32,
    interrupt: Interrupt,
    total_channels: usize,
}

impl TIMERx {
    unsafe fn clone(&self) -> Self {
        Self {
            base_address: self.base_address,
            interrupt: self.interrupt,
            total_channels: self.total_channels
        }
    }
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
timerx_from!(TIMER1, TIMER1, 4);
timerx_from!(TIMER2, TIMER2, 4);
timerx_from!(TIMER3, TIMER3, 6);
timerx_from!(TIMER4, TIMER4, 6);


pub struct Timer {
    periph: TIMERx,
    available_channels: CriticalMutex<u8>
}

impl Timer {
    pub fn new(mut periph: TIMERx) -> Self {
        periph.mode.write_timer();
        periph.prescaler.write(0); // 16 MHz
        periph.bitmode.write_32bit();
        periph.tasks_start.write_trigger();

        let mut available_channels = 0;
        for i in 0..periph.total_channels {
            available_channels |= 1 << i;
        }

        Self { periph, available_channels: CriticalMutex::new(available_channels) }
    }

    pub fn new_channel<'a>(&'a self) -> Option<TimerChannel<'a>> {

        let mut available_channels = self.available_channels.lock();

        let mut index = None;
        for i in 0..8 {
            let mask: u8 = 1 << i;
            if *available_channels & mask != 0 {
                index = Some(i);
                *available_channels &= !mask;
                break;
            }
        }

        let index = match index {
            Some(v) => v,
            None => return None
        };

        Some(TimerChannel {
            timer: self,
            periph: unsafe { self.periph.clone() },
            index,
            interrupt_mask: 1 << (16 + index)
        })
    }

    pub fn capture(&self) -> Option<u32> {
        let mut channel = match self.new_channel() {
            Some(v) => v,
            None => return None
        };
        
        Some(channel.capture())
    }
}

pub struct TimerChannel<'a> {
    // TODO: Eventually find a nice way to dedup these.
    timer: &'a Timer,
    periph: TIMERx,

    index: usize,
    interrupt_mask: u32,
}

impl<'a> Drop for TimerChannel<'a> {
    fn drop(&mut self) {
        let mut available_channels = self.timer.available_channels.lock();
        *available_channels |= 1 << self.index;
    }
}

impl<'a> TimerChannel<'a> {

    pub fn compare_value(&self) -> u32 {
        self.periph.cc[self.index].read()
    }

    pub fn set_compare_value(&mut self, value: u32) {
        unsafe {
            self.periph.cc.get_unchecked_mut(self.index).write(value);
        }
    }

    pub fn capture(&mut self) -> u32 {
        unsafe {
            self.periph.tasks_capture.get_unchecked_mut(self.index).write_trigger();
            self.periph.cc.get_unchecked_mut(self.index).read()
        }
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

    pub fn pending_event_no_wait(&mut self) -> bool {
        let mut pending = false;
        
        let event = unsafe { self.periph.events_compare.get_unchecked_mut(self.index) };
 
        if event.read().is_generated() {
            event.write_notgenerated();
            pending = true;
        }

        pending
    }

    /// Clears any pending event on this channel.
    /// If the caller doesn't run for at least 4 more cycles, it may re-trigger an interrupt.
    #[inline(always)]
    pub fn clear_pending_no_wait(&mut self) {
        unsafe {
            self.periph.events_compare.get_unchecked_mut(self.index).write_notgenerated();
        }
    }

    pub fn compare_event(&self) -> &EventRegister {
        &self.periph.events_compare[self.index]
    }

    pub fn capture_task(&mut self) -> &mut TaskRegister {
        &mut self.periph.tasks_capture[self.index]
    }

    pub fn enable_interrupt(&mut self) {
        self.periph.intenset.write(INTENSET_WRITE_VALUE::from_raw(self.interrupt_mask));

        /*
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
        */
    }

    pub fn disable_interrupt(&mut self) {
        self.periph.intenclr.write(INTENCLR_WRITE_VALUE::from_raw(self.interrupt_mask));

        /*
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
        */

        // crate::events::flush_events_clear();
    }
}
