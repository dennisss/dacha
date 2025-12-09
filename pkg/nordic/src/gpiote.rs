

use peripherals::raw::gpiote::GPIOTE;
use common::register::RegisterRead;
use common::register::RegisterWrite;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;
use peripherals::raw::TaskRegister;
use executor::interrupts::wait_for_irq;

use crate::gpio::GPIOPin;
use crate::gpio::{PinDirection, PinLevel, GPIO};
use crate::pins::{PeripheralPin, PeripheralPinHandle};

/// Manages shared access to the GPIOTE peripheral.
///
/// Different users may use the methods on this struct to allocate
/// independent channels for isolated usecases.
///
/// Note that while all the state is in the native GPIOTE peripheral
/// memory, we require this to consume the whole peripheral to ensure
/// no other code is using the peripheral in an incompatible way.
///
/// Internally this keeps track of which channels are in use by immediately changing the 'mode' of any created channel to non-'disabled'. Any channels that are still disabled in periphereal memory haven't been handled out to any users yet. 
pub struct GPIOTEChannels {
    periph: GPIOTE
}

impl GPIOTEChannels {
    pub fn new(periph: GPIOTE) -> Self {
        Self { periph }
    }

    /// Finds the first channel in the GPIOTE peripheral that is still disabled.
    /// Note that the caller needs to immediately configure it for it to not longer be returned by future calls of this.
    fn next_free_channel(&mut self) -> Option<GPIOTEChannel> {
        for i in 0..self.periph.config.len() {
            if self.periph.config[i].read().mode().is_disabled() {
                return Some(GPIOTEChannel { index: i, periph: unsafe { GPIOTE::new() } });
            }
        }

        None
    }

    /// NOTE: Currently this is fixed to be a task channel that toggles the output pin on the TASKS_OUT task.
    pub fn new_task_channel<Pin: PeripheralPin>(
        &mut self, pin: Pin
    ) -> Option<GPIOTaskChannel> {
        let channel = match self.next_free_channel() {
            Some(v) => v,
            None => return None
        };

        let pin_port = pin.port() as u32;
        let pin_num = pin.pin();

        self.periph.config[channel.index].write_with(move |v| {
            v.set_port(pin_port)
                .set_psel(pin_num as u32)
                .set_polarity_with(|v| v.set_toggle())
                .set_mode_with(|v| v.set_task())
        });

        Some(GPIOTaskChannel {
            channel
        })
    }

    // TODO: Think more abotu whether or not I want this to take ownership of the pin.
    pub fn new_interrupt_channel<Pin: PeripheralPin>(
        &mut self,
        pin: &Pin,
        polarity: GPIOInterruptPolarity
    ) -> Option<GPIOInterruptChannel> {
        let channel = match self.next_free_channel() {
            Some(v) => v,
            None => return None
        };

        // TODO: Ideally set this up closer to the waiter since we can't guarantee it will be consistently polled.
        self.periph.intenset.write_with(|v| match channel.index {
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

        self.periph.config[channel.index].write_with(|v| {
            v.set_port(pin.port() as u32)
                .set_psel(pin.pin() as u32)
                .set_polarity_with(|v| match polarity {
                    GPIOInterruptPolarity::RisingEdge => v.set_lotohi(),
                    GPIOInterruptPolarity::FallingEdge => v.set_hitolo(),
                    GPIOInterruptPolarity::Toggle => v.set_toggle(),
                })
                .set_mode_with(|v| v.set_event())
        });

        Some(GPIOInterruptChannel {
            channel
        })
    }

}


pub struct GPIOTEChannel {
    periph: GPIOTE,
    index: usize,
}

// On drop, the channel will get disabled.
impl Drop for GPIOTEChannel {
    fn drop(&mut self) {
        self.periph.config[self.index].write_with(move |v| {
            v.set_mode_with(|v| v.set_disabled())
        });

        // TODO: Clear interrupts, events, etc.

        /*

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
        */
    }
}

pub struct GPIOTaskChannel {    
    channel: GPIOTEChannel
}

impl GPIOTaskChannel {


    // TODO: Make this more unsafe since we can't guarantee the channel reference outlives the PPI channel.
    pub fn out_task(&mut self) -> &mut TaskRegister {
        &mut self.channel.periph.tasks_out[self.channel.index]
    }

}

#[derive(Clone, Copy)]
pub enum GPIOInterruptPolarity {
    RisingEdge,
    FallingEdge,
    Toggle,
}


/// 
///
/// Corresponds to a single GPIOTE channel on which 
pub struct GPIOInterruptChannel {
    // pin: Pin,
    channel: GPIOTEChannel,
}

impl GPIOInterruptChannel {

    // pub fn take_pin(self) -> Pin {
    //     self.pin
    // }

    /// Checks if the interrupt event is currently pending and clears it.
    /// Returns the initial value when this was called.
    pub fn pending_events(&mut self) -> bool {
        let mut pending = false;
        
        let i = self.channel.index;

        if self.channel.periph.events_in[i].read().is_generated() {
            self.channel.periph.events_in[i].write_notgenerated();
            crate::events::flush_events_clear();
            pending = true;
        }

        pending
    }

    pub async fn wait_for_interrupts(&mut self) -> bool {
        // TODO: Enable the interrupt when entering this function and disable it when out of here.

        wait_for_irq(Interrupt::GPIOTE).await;
        self.pending_events()
    }

}


/// TODO: The correctness of this assumes there is only one consumer of the port event and interrupts.
pub struct GPIOPortWaiter {
    periph: GPIOTE
}

impl Drop for GPIOPortWaiter {
    fn drop(&mut self) {
        self.periph.intenclr.write_with(|v| v.set_port());
    }
}

impl GPIOPortWaiter {
    pub unsafe fn new() -> Self {
        let mut periph = GPIOTE::new();

        periph.intenset.write_with(|v| v.set_port());

        Self {
            periph
        }
    }

    pub fn pending_event(&mut self) -> bool {
        if self.periph.events_port.read().is_generated() {
            self.periph.events_port.write_notgenerated();
            crate::events::flush_events_clear();
            return true;
        }

        false
    }

    pub async fn wait(&mut self) {
        while !self.pending_event() {
            wait_for_irq(Interrupt::GPIOTE).await;
        }
    }

}


