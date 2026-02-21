use common::fixed::vec::FixedVec;
use executor::critical_mutex::CriticalMutex;
use executor::interrupts::wait_for_irq;
use executor::CriticalSection;
use peripherals::raw::Interrupt;
use peripherals_proto::peripherals::PeripheralResponse;

use crate::controller::stepper::StepperMotorController;
use crate::controller::spi_timer_controller::*;
use crate::controller::peripherals_controller::{PeripheralsController, PeripheralEntry};


/// Note that this is effectively limited by how many channels we have on the timers.
pub const NUM_ENTRIES: usize = 4;

/// Split out state for peripherals which mostly 
pub struct TimerController {
    pub state: CriticalMutex<State>,
}

pub struct State {
    pub entries: FixedVec<TimerControllerEntry, NUM_ENTRIES>,
}

pub enum TimerControllerEntry {
    Stepper(StepperMotorController),
    SPITimer(SPITimerController),
}

impl TimerController {
    pub fn new() -> Self {
        Self {
            state: CriticalMutex::new(State {
                entries: FixedVec::new()
            })
        }
    }
}

pub fn timer_controller_interrupt(controller: *const ()) {
    let controller: &'static TimerController = unsafe { core::mem::transmute(controller) };
    
    // TODO: Make this lock free given we already have a critical section.
    lock!(state <= controller.state.lock(), {
        for entry in &mut state.entries[..] {
            match entry {
                TimerControllerEntry::Stepper(controller) => controller.tick(),
                TimerControllerEntry::SPITimer(controller) => controller.tick(),
            }
        }
    });
}

define_thread!(
    TimerControllerResponseThread,
    timer_controller_response_thread,
    controller: &'static PeripheralsController
);

async fn timer_controller_response_thread(
    periph_controller: &'static PeripheralsController
) {

    loop {

        let mut completed_request = None;

        lock!(timer_state <= periph_controller.timer_controller.state.lock(), {
            for entry in &mut timer_state.entries[..] {
                match entry {
                    TimerControllerEntry::Stepper(_) => {},
                    TimerControllerEntry::SPITimer(spi) => {
                        completed_request = spi.read_completed_request();
                        if completed_request.is_some() {
                            break;
                        }
                    },
                }
            }
        });

        if let Some((response_code, request_sequence, buffer, buffer_idx)) = completed_request {

            let mut res = PeripheralResponse::default();
            res.set_request_sequence(request_sequence as u32);
            res.set_error_code(response_code);

            lock!(state <= periph_controller.state.lock(), {
                state.entries[buffer_idx] = PeripheralEntry::Buffer(buffer);
                periph_controller.write_response(&mut state, &res);
            });

            // Check if there are more requests pending.
            continue;
        }

        // NOTE: The assumption is that we are the exclusive users of this interrupt.
        wait_for_irq(Interrupt::EGU0_SWI0).await;
    }
}

