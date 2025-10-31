// Driver for a single stepper motor using DIR/STEP pins.
//
// The input is a sequence of times at which to flip the STEP pin and in which
// direction to move the motor for each step.
//
// The outputs are timed flips on the STEP pin (assumes DEDGE=1 on TMC2209). Shortly
// before each flip, the DIR pin will also be set to some user defined per-step value. 
//
// - Times are relative to the current value of one of the TIMERx peripherals.
// - A single TIMERx CC[i] register is setup to compare to the next step time.
// - A single GPIOTE channel is allocated for flipping the STEP pin.
// - A single PPI channel is used to wire up EVENTS_COMPARE[i] on the TIMER to the GPIOTE channel
//   so that the STEP pin is flipped at precisely the right time.
//
// Assuming the TIMERx is synced to the 16MHz clock, then if on clock sync 'i', the CC register
// is compared, then on clock sync 'i+1' the PPI will notice and trigger the GPIOTE event.
//
// Other important details:
// - Up to 16 stepper motions can be enqueued at a given time
// - This requires a background thread to be running to continously react to EVENTS_COMPARE, motion plan changes and enqueue new steps.

use common::fixed::queue::FixedQueue;
use common::fixed::vec::FixedVec;
use executor::cond_value::*;
use executor::interrupts::wait_for_irq;
use common::register::RegisterRead;
use common::register::RegisterWrite;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;
use peripherals::raw::TaskRegister;
use peripherals_proto::peripherals::StepperMotorStatus;

use crate::gpio::GPIOPin;
use crate::gpio::{PinDirection, PinLevel, GPIO};
use crate::pins::{PeripheralPin, PeripheralPinHandle};
use crate::timer::*;
use crate::gpiote::*;
use crate::ppi::*;
use crate::controller::{PeripheralsController, PeripheralEntry};

const MAX_ENQUEUED_MOTIONS: usize = 16;

/// Minimum time from now to the next step pulse (in 16MHz clock cycles).
const MIN_STEP_TIME: u32 = 10;

/// Maximum time from now to the next step pulse (in 16MHz clock cycles).
///
/// Note that this is also used to guard against steps that we missed since these will
/// appear as times that overflowed and are 'before' the current time (have very large duration)
const MAX_STEP_TIME: u32 = 16_000_000;  // 1 second


define_thread!(
    StepperPeripheralThread,
    stepper_worker_thread,
    controller: &'static PeripheralsController
);

async fn stepper_worker_thread(
    controller: &'static PeripheralsController,
) {
    loop {
        let mut pending = false;

        lock!(state <= controller.state.lock().await.unwrap(), {
            for entry in &mut state.entries {
                let stepper = match entry {
                    PeripheralEntry::Stepper { controller } => controller,
                    _ => continue
                };

                pending |= stepper.tick(); 
            }
        });

        if !pending {
            return;
        }

        wait_for_irq(Interrupt::TIMER0).await;
    }
}


pub struct StepperMotion {
    pub direction: bool,
    pub next_time: u32,
    pub next_velocity: u32,
    pub acceleration: u32,
    pub num_steps: usize,
}

pub struct StepperMotorController {
    step_timer_channel: TimerChannel,

    step_ppi_channel: PPIChannel,

    step_gpiote_channel: GPIOTaskChannel,

    dir_pin: GPIOPin,

    motion_queue: FixedQueue<StepperMotion, MAX_ENQUEUED_MOTIONS>,

    /// If true, then the step timer channel has a 'live' valid step time enqueued.
    ///
    /// The liveness of the CC register is controlled by whether or not the PPI channel
    /// reading from it is enabled.
    have_enqueued_step: bool,

    stats: Stats
}

#[derive(Default)]
struct Stats {
    /// Total number of steps we have stepped through completely.
    total_steps: u32,

    /// Number of failures encountered. A failure is one where we weren't able to
    /// trigger a step fast enough.
    faults: u32,
}

impl StepperMotorController {
    pub fn new(
        mut step_pin: GPIOPin,
        mut dir_pin: GPIOPin,
        ppi: &mut PPIChannels,
        gpiote: &mut GPIOTEChannels,
        timer: &mut Timer,
    ) -> Option<Self> {
        // Initialize GPIO pins as outputs with arbitrary initial values.
        step_pin
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);
        dir_pin
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);

        // Wire next GPIOTE channel to the STEP pin.
        // Triggering TASKS_OUT flips the output level of the pin.
        let mut step_gpiote_channel = match gpiote.new_task_channel(step_pin) {
            Some(v) => v,
            None => return None
        };

        let step_timer_channel = match timer.new_channel() {
            Some(v) => v,
            None => return None
        };

        // Trigger a STEP GPIO toggle on the CC register's COMPARE event.
        let step_ppi_channel = match ppi.new_channel(
            step_timer_channel.compare_event(),
            step_gpiote_channel.out_task()
        ) {
            Some(v) => v,
            None => return None
        };

        Some(Self {
            step_timer_channel,
            step_ppi_channel,
            step_gpiote_channel,
            dir_pin,
            motion_queue: FixedQueue::new(),
            have_enqueued_step: false,
            stats: Stats::default(),
        })
    }

    /// Appends a new future motion to perform.
    /// Note that internally we assume that all motions are ordered and don't overlap.
    ///
    /// Returns whether or not there was space to fit this motion.
    pub fn enqueue_motion(&mut self, motion: StepperMotion) -> bool {
        if self.motion_queue.is_full() {
            return false;
        }

        if motion.num_steps == 0 {
            return true;
        }

        self.motion_queue.push_back(motion);
        true
    }

    /// Clear the entire queue of motions. If a step is currently in progress,
    /// the next tick will attempt to clear it.
    pub fn clear_motions(&mut self) {
        self.motion_queue.clear();
        // TODO:
    }

    pub fn status(&self) -> StepperMotorStatus {
        let mut proto = StepperMotorStatus::default();
        proto.set_total_steps(self.stats.total_steps);
        proto.set_faults(self.stats.faults);
        proto.set_empty_queue_slots((self.motion_queue.capacity() - self.motion_queue.len()) as u32);
        proto.set_active(!self.motion_queue.is_empty() || self.have_enqueued_step);
        proto
    }

    /// This needs to be called when:
    ///
    /// - There is a motion plan change.
    /// - There is a TIMER0 interrupt.
    ///
    /// Returns true is a interrupt is currently configured to fire for the timer peripheral so the
    /// caller needs to monitor it (not monitoring it or not calling tick later may keep the interrupt
    /// permanently firing and may mess up other users of the timer).
    pub fn tick(&mut self) -> bool {
        /*
        TMC2209 timing requirements:
        - DIR is set at least 20ns before STEP is triggered
        - DIR is held for at least 20ns after STEP is tirggered
        - STEP should be held for at least 100ns

        This means we need at least ~1 full 16MHz timer cycle after we set up stuff in this function like DIR to when the timer fires. Then at least amount ~1 full 16Mhz timer cycle before we do things again 

        But note that PPI propagation takes another 1 cycle so we should ensure at least 2 cycles pass since the previous event (this is like 8 CPU clock cycles so this we probably don't need any explicit delay anywhere).
        */

        // TIMING: In the case that we are done the step, this should be take at least a
        // few timer cycles so we shouldn't need extra hold time for the previous step. 
        if self.have_enqueued_step {
            // TODO: After the step has fired, we need a minimum hold time.

            if self.step_timer_channel.pending_event() {
                self.have_enqueued_step = false;
                self.step_ppi_channel.disable();
                self.step_timer_channel.disable_interrupt();
                self.stats.total_steps += 1;
            } else {
                // Still waiting for the step.
                return true;
            }
        }

        // Attempt to setup the next step.

        let current_time = self.step_timer_channel.capture();

        let mut motion = match self.motion_queue.first_mut() {
            Some(v) => v,
            None => return false
        };

        let next_time = motion.next_time;

        // Amount of time remaining between now and the next step.
        let delta_time = {
            let mut t = next_time.wrapping_sub(current_time);
            if next_time < current_time {
                t = t.wrapping_add(u32::max_value());
            }

            t
        };

        // Must have a delay between CC setup and triggering of at least ~2 so that 
        // DIR rises/falls early enough before STEP. Also need extra time for CPU processing delay.
        if delta_time < MIN_STEP_TIME || delta_time >= MAX_STEP_TIME {
            self.motion_queue.clear();
            self.stats.faults += 1;
            return false;
        }

        // TIMING: It is undefined whether or not the current_time capture above will
        // trigger the compare event on the next cycle so just in case, we the above logic should
        // take at least a cycle and then we clear it again here.
        let _ = self.step_timer_channel.pending_event();

        // TODO:
        self.dir_pin.write(if motion.direction {
            PinLevel::High
        } else {
            PinLevel::Low
        });

        self.step_timer_channel.set_compare_value(motion.next_time);

        motion.next_time = motion.next_time.wrapping_add(motion.next_velocity);
        motion.next_velocity = motion.next_velocity.wrapping_add(motion.acceleration);
        motion.num_steps -= 1;

        if motion.num_steps == 0 {
            self.motion_queue.pop_front();
        }

        self.have_enqueued_step = true;
        self.step_ppi_channel.enable();
        self.step_timer_channel.enable_interrupt();

        true
    }

}
