use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};
use peripherals_proto::peripherals::ConfigureGPIO_InterruptPolarity;
use executor::interrupts::wait_for_irq;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;

use crate::gpio::PinLevel;
use crate::gpiote::GPIOPortWaiter;
use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;


/*
General handling:
- Allow having up to one request waiting for interrupt events.
- can return CANCELLED
- Cancellation can re-use the sequence number, peripheral_number, and 

Alternative:
- Subscribe[]


GPIOTE : EVENTS_PORT Event
- Triggered by DETECT signal in the GPIO peripheral


TODO: figure out which settings are required for wake from sleep (check GPIOTE docs)

What poll does:
- Sets up sense register
    - REgisters level and request number in the GPIO peripheral

- Interrupt thread
    - Clear port event
    - For each GPIO pin
        - If triggered, return response
        - Trigger a 
    - Wait for PORT event
    - 


*/

define_thread!(
    InterruptPeripheralThread,
    interrupt_worker_thread,
    controller: &'static PeripheralsController
);

async fn interrupt_worker_thread(
    controller: &'static PeripheralsController
) {
    executor::interrupts::yield_now().await;

    let mut waiter = unsafe { GPIOPortWaiter::new() };

    loop {
        lock!(state <= controller.state.lock(), {
            for i in 0..state.entries.len() {
                let mut entry = match &mut state.entries[i] {
                    PeripheralEntry::GPIO(entry) => entry,
                    _ => continue
                };

                let request_sequence = match entry.pending_interrupt_sequence.clone() {
                    Some(v) => v,
                    None => continue
                };

                let current_level = entry.pin.read();

                let fired = match entry.interrupt_polarity {
                    ConfigureGPIO_InterruptPolarity::DISABLED => false,
                    ConfigureGPIO_InterruptPolarity::HIGH_LEVEL => {
                        current_level == PinLevel::High
                    }
                    ConfigureGPIO_InterruptPolarity::LOW_LEVEL => {
                        current_level == PinLevel::Low
                    }
                    ConfigureGPIO_InterruptPolarity::RISING_EDGE |
                    ConfigureGPIO_InterruptPolarity::FALLING_EDGE => todo!()
                };

                if fired {
                    entry.pin.set_sense(None);
                    entry.pending_interrupt_sequence = None;

                    let mut res = PeripheralResponse::default();
                    res.set_request_sequence(request_sequence);
                    controller.write_response(&mut state, &res);
                }
            }
        });

        waiter.wait().await;
    }
}