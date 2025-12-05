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
// - Up to MAX_ENQUEUED_MOTIONS stepper motions can be enqueued at a given time
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
use peripherals_proto::peripherals::{StepperMotorStatus, StepperMotorMotion, StepperMotorMotion_Direction, StepperMotorStatus_StoppedReason};
use cnc::quadratic_stepper_motion::{QuadraticStepperMotion, StepCount};
use cnc::time_remaining_u32;

use crate::controller::allocator::Box;
use crate::gpio::GPIOPin;
use crate::gpio::{PinDirection, PinLevel, GPIO};
use crate::pins::{PeripheralPin, PeripheralPinHandle};
use crate::timer::*;
use crate::gpiote::*;
use crate::ppi::*;
use crate::controller::{PeripheralsController, PeripheralEntry};

const MAX_ENQUEUED_MOTIONS: usize = 1024;

/// Minimum time from now to the next step pulse (in 16MHz clock cycles).
/// Theoretical minimum is around 2, but its good to have a buffer if I estimated wrong.
const MIN_STEP_TIME: u32 = 20;

/// Maximum time from now to the next step pulse (in 16MHz clock cycles).
///
/// Note that this is also used to guard against steps that we missed since these will
/// appear as times that overflowed and are 'before' the current time (have very large duration)
const MAX_STEP_TIME: u32 = 2 * 16_000_000;  // 2 seconds


// This thread is started when the first stepper is configured is stopped
// when all the peripherals are unconfigured.
define_thread!(
    StepperPeripheralThread,
    stepper_worker_thread,
    controller: &'static PeripheralsController
);

async fn stepper_worker_thread(
    controller: &'static PeripheralsController,
) {
    loop {
        lock!(state <= controller.state.lock().await.unwrap(), {
            for entry in &mut state.entries {
                let stepper = match entry {
                    PeripheralEntry::Stepper { controller } => controller,
                    _ => continue
                };

                stepper.tick(); 
            }
        });

        wait_for_irq(Interrupt::TIMER4).await;
    }
}



pub struct StepperMotorController {
    step_timer_channel: TimerChannel,

    step_ppi_channel: PPIChannel,

    step_gpiote_channel: GPIOTaskChannel,

    dir_pin: GPIOPin,

    motion_queue: Box<FixedQueue<QuadraticStepperMotion, MAX_ENQUEUED_MOTIONS>>,

    /// If true, then the step timer channel has a 'live' valid step time enqueued.
    ///
    /// The liveness of the CC register is controlled by whether or not the PPI channel
    /// reading from it is enabled.
    have_enqueued_step: bool,

    enqueued_step_dir: i32,

    /// Direction of the last enqueued motion.
    last_direction: bool,

    stats: Stats
}

#[derive(Default)]
struct Stats {
    position: i32,

    /// Number of failures encountered. A failure is one where we weren't able to
    /// trigger a step fast enough.
    ///
    /// TODO: If one motor is ever stopped, allow other dependent motors (A/B)
    /// to be simultaneously stopped)
    stopped: StepperMotorStatus_StoppedReason,
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
            motion_queue: Box::default(),
            have_enqueued_step: false,
            enqueued_step_dir: 1,
            last_direction: false,
            stats: Stats::default(),
        })
    }

    /// Appends a new future motion to perform.
    /// Note that internally we assume that all motions are ordered and don't overlap.
    ///
    /// Returns whether or not there was space to fit this motion.
    pub fn enqueue_motion(&mut self, req: &StepperMotorMotion) -> bool {
        if self.motion_queue.is_full() {
            return false;
        }

        let direction = match req.direction() {
            StepperMotorMotion_Direction::UNCHANGED => self.last_direction,
            StepperMotorMotion_Direction::FORWARD => true,
            StepperMotorMotion_Direction::BACKWARD => false
        };
        self.last_direction = direction;

        // Reject motions until the controller explicitly clears the error condition.
        if self.stats.stopped != StepperMotorStatus_StoppedReason::NONE {
            return true;
        }

        let motion = QuadraticStepperMotion {
            next_step_time: req.next_step_time(),
            next_step_duration: req.next_step_duration(),
            step_duration_increment: req.step_duration_increment(),
            num_steps: StepCount::new(req.num_steps_minus_one() + 1, direction),
        };

        self.motion_queue.push_back(motion);
        true
    }

    /// Clear the entire queue of motions. If a step is currently in progress,
    /// the next tick will attempt to clear it.
    pub fn clear_motions(&mut self) {
        self.motion_queue.clear();
        self.stats.stopped = StepperMotorStatus_StoppedReason::HOST_CLEAR;
    }

    pub fn reset(&mut self) {
        self.stats.stopped = StepperMotorStatus_StoppedReason::NONE;
    }

    pub fn status(&self) -> StepperMotorStatus {
        let mut proto = StepperMotorStatus::default();
        proto.set_position(self.stats.position);
        proto.set_stopped(self.stats.stopped);
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
            if self.step_timer_channel.pending_event() || self.motion_queue.is_empty() {
                self.have_enqueued_step = false;
                self.step_ppi_channel.disable();
                self.step_timer_channel.disable_interrupt();
                self.stats.position += self.enqueued_step_dir;
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

        let mut next_time = motion.next_step_time;

        // Amount of time remaining between now and the next step.
        let delta_time = time_remaining_u32(next_time, current_time);

        // Must have a delay between CC setup and triggering of at least ~2 so that 
        // DIR rises/falls early enough before STEP. Also need extra time for CPU processing delay.
        if delta_time < MIN_STEP_TIME || delta_time >= MAX_STEP_TIME {
            self.motion_queue.clear();
            self.stats.stopped = StepperMotorStatus_StoppedReason::TIMING_FAULT;
            return false;
        }

        // TIMING: It is undefined whether or not the current_time capture above will
        // trigger the compare event on the next cycle so just in case, we the above logic should
        // take at least a cycle and then we clear it again here.
        let _ = self.step_timer_channel.pending_event();

        self.dir_pin.write_bool(motion.num_steps.direction());
        // TODO: Optimize this.
        self.enqueued_step_dir = if motion.num_steps.direction() { 1 } else { -1 };

        self.step_timer_channel.set_compare_value(next_time);

        motion.next();

        if motion.num_steps.count() == 0 {
            self.motion_queue.pop_front();
        }

        self.have_enqueued_step = true;
        self.step_ppi_channel.enable();
        self.step_timer_channel.enable_interrupt();

        true
    }

}
