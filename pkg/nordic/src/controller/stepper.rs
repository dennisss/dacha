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
/// This must be safely larger than the amount of time it will take all the code in tick() after ".capture()" to run.
const MIN_STEP_TIME: u32 = 16;

/// Maximum time from now to the next step pulse (in 16MHz clock cycles).
///
/// Note that this is also used to guard against steps that we missed since these will
/// appear as times that overflowed and are 'before' the current time (have very large duration)
const MAX_STEP_TIME: u32 = 4 * 16_000_000;  // 4 seconds


pub struct StepperMotorController {
    /// If true, then the step timer channel has a 'live' valid step time enqueued.
    ///
    /// The liveness of the CC register is controlled by whether or not the PPI channel
    /// reading from it is enabled.
    have_enqueued_step: bool,

    step_timer_channel: TimerChannel<'static>,

    step_ppi_channel: PPIChannel,

    step_gpiote_channel: GPIOTaskChannel,

    dir_pin: GPIOPin,

    motion_queue: Box<FixedQueue<QuadraticStepperMotion, MAX_ENQUEUED_MOTIONS>>,

    enqueued_step_dir: i32,

    /// Direction of the last enqueued motion.
    last_direction: bool,

    ///
    pulse_width: u32,

    pulse_end_time: u32,

    stats: Stats,
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
        pulse_width: u32,
        ppi: &mut PPIChannels,
        gpiote: &mut GPIOTEChannels,
        timer: &'static Timer,
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
            pulse_width,
            pulse_end_time: 0,
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
    pub fn clear_motions(&mut self, current_time: u32) {
        // Attempt to abort the enqueued step if it is far enough away in time
        // (and hasn't occured yet) that we think we can safely stop it.
        if self.have_enqueued_step {
            let next_time = self.step_timer_channel.compare_value();
            let delta_time = time_remaining_u32(next_time, current_time);
            if delta_time > MIN_STEP_TIME && delta_time < MAX_STEP_TIME {
                self.step_ppi_channel.disable();
                self.step_timer_channel.disable_interrupt();
                self.have_enqueued_step = false;
                self.pulse_end_time = 0;
            }
        }

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

    // TODO: Verify that the PPI always gets triggered without race conditions (disabling it before it triggers) by forking to a counter mode timer and verifying the count is correct).

    /// This needs to be called when:
    ///
    /// - There is a motion plan change.
    /// - There is a TIMER0 interrupt.
    pub fn tick(&mut self) {
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
            if self.step_timer_channel.pending_event_no_wait() {
                self.stats.position += self.enqueued_step_dir;
                self.have_enqueued_step = false;
                self.step_timer_channel.disable_interrupt();
                // This line is last to ensure that the PPI actually fires before we disable it.
                self.step_ppi_channel.disable();
            } else {
                // Still waiting for the step.
                return;
            }
        }

        // Attempt to setup the next step.

        let (next_time, next_direction) = {
            if self.pulse_end_time != 0 {
                self.pulse_end_time = 0;
                // TODO: Return a proper direction.
                (self.pulse_end_time, false)
            } else {
                let mut motion = match self.motion_queue.first_mut() {
                    Some(v) => v,
                    None => return
                };

                let next_time = motion.next_step_time;
                let next_direction = motion.num_steps.direction();

                motion.next();

                if motion.num_steps.count() == 0 {
                    self.motion_queue.pop_front();
                }

                if self.pulse_width != 0 {
                    self.pulse_end_time = next_time.wrapping_add(self.pulse_width).max(1);
                }

                (next_time, next_direction)
            }
        };

        // TODO: Optimize this.
        self.enqueued_step_dir = if next_direction { 1 } else { -1 };

        self.dir_pin.write_bool(next_direction);

        // TIMING: The code is structured so that this line is low as possible in this function
        // so that we can bound the amount of time it takes after this to fully setup the step.
        let current_time = self.step_timer_channel.capture();

        // Amount of time remaining between now and the next step.
        let delta_time = time_remaining_u32(next_time, current_time);

        // Must have a delay between CC setup and triggering of at least ~2 so that 
        // DIR rises/falls early enough before STEP. Also need extra time for CPU processing delay.
        let too_slow = delta_time < MIN_STEP_TIME || delta_time >= MAX_STEP_TIME;
        if unsafe { core::intrinsics::unlikely(too_slow) } {
            self.motion_queue.clear();
            self.stats.stopped = StepperMotorStatus_StoppedReason::TIMING_FAULT;
            return;
        }

        self.step_timer_channel.set_compare_value(next_time);

        // If the timer channel hasn't been used in a while, it may have a stale event pending so we
        // need to clear that.
        //
        // It's also undefined if the above current_time capture will trigger a new event
        // immediately.
        //
        // TIMING: This must run at least one clock cycle after the capture() to ensure we clear any
        // event caused by that.
        let _ = self.step_timer_channel.clear_pending_no_wait();

        self.have_enqueued_step = true;
        self.step_ppi_channel.enable();
        self.step_timer_channel.enable_interrupt();
    }

}
