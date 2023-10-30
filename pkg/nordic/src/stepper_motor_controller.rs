// This file contains an implementation of a multi-stepper motor controller
// optimized with usage of the NRF PPI, GPIOTE, and TIMER peripherals.
//
// TODO: To reduce the number of interrupts, we could consider using multiple CC
// registers per motor to queue up multiple steps in advance.

// TODO: Document the expected driver settings (dedge = 1)

use cnc::linear_motion::LinearMotion;
use common::fixed::queue::FixedQueue;
use common::fixed::vec::FixedVec;
use executor::cond_value::*;
use executor::mutex::*;
use math::matrix::Vector3f;
use peripherals::raw::gpiote::GPIOTE;
use peripherals::raw::ppi::chenset::CHENSET_WRITE_VALUE;
use peripherals::raw::ppi::PPI;
use peripherals::raw::register::RegisterRead;
use peripherals::raw::register::RegisterWrite;
use peripherals::raw::timer0::TIMER0_REGISTERS;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;
use peripherals::raw::TaskRegister;

use crate::gpio::GPIOPin;
use crate::gpio::{PinDirection, PinLevel, GPIO};
use crate::pins::{PeripheralPin, PeripheralPinHandle};
use crate::spi::SPIHost;
use crate::tmc2130::TMC2130;

/// Maximum number of stepper motors we support controlling on one chip.
///
/// This is mainly limited by:
/// - Number of TIMERS : NRF52840 has 5. We currently need one per motor to
///   simplify the implementation.
/// - Number of PPI channels : NRF52840 has 20. Need one per CC register used in
///   all TIMERs.
/// - Number of GPIOTE channels : NRF52840 has 8. Need one per motor.
/// - Static size of the vector used in the LinearMotion (currently Vector3f).
pub const MAX_NUM_MOTORS: usize = 3;

pub const MAX_ENQUEUED_MOTIONS: usize = 16;

/*
Going faster:
- Goal is minimize interrupts as interrupts have an executor overhead
- Use the TIMER
    - Set prescaller to 0 to get a 16MHz wave.
    - BITMODE = 32-bit
    - Timers have at least 4 CC registers so can buffer basically up to 4 steps in advance.
- Channels at 0 velocity and acceleration can save speed
    - Must support Infinite time to reach (or close to infinite)


- Other things:
    - When the CPU finishes 2 steps from the quickest motor,

- So support 5 motors.


Motor control loop:
- Global State:
    - List of linear motions to execute.
        - We assume that the list is continous and filled faster than the commands get executed.
    - Input event to trigger an emergency stop
        - The client is response for clearing the motion queue before doing an estop
- Start up
    - Set all the direction pins

- Clear all timers
-

TODO: We also want to be able to look up what the current position is (for debugging and knowing where the endstop is).

- DIR most not change for at least 20 nanoseconds after the

- So I wake up for the last step.
    - Start
*/

/// Shared channel used to communicate with a StepperMotorController.
/// This is used to communicate operations to execute and receive a progress
/// report on when those operations have completed or where interrupted.
pub struct StepperMotorControllerQueue {
    intent: CondValue<StepperMotorControllerIntent>,
    inputs_queue: Mutex<FixedQueue<LinearMotion, MAX_ENQUEUED_MOTIONS>>,
    state: CondValue<StepperMotorControllerState>,
}

/// Summary of what the StepperMotorController's client wants the controller to
/// do.
pub enum StepperMotorControllerIntent {
    /// Don't do anything. If currently doing something, immediately stop.
    ///
    /// This is used as the initial intent.
    Stop,

    /// Read motions from the start of inputs_queue and execute them.
    /// The expectation is the client will continue enqueuing motions to keep
    /// inputs_queue full while advertising this intent.
    Run,

    /// Similar to Run except no more motions will be enqueued.
    /// Finish executing any enqueued motions and transition to the Finished
    /// state.
    Finish,

    /// If we are currently in the StoppedEarly state,
    Reset,
}

pub enum StepperMotorControllerState {
    Stopped,
    Running,
    Finished,

    /// Currently this means that we received the Run intent but ran out of
    /// motions to execute.
    Error,
}

/*
// spi: SPIHost,

// /// A pin connected to the DIAG1 output of all TMC2130 driver.
// diag1_pin: PeripheralPinHandle,
*/

pub struct StepperMotorControllerConfig<'a> {
    pub gpio: &'a mut GPIO,
    pub gpiote: GPIOTE,
    pub ppi: PPI,

    /// NOTE: This timer must have one CC register for each motor.
    pub timer: &'static mut TIMER0_REGISTERS,

    pub timer_interrupt: Interrupt,

    /// A pin connected to the EN input of all TMC2130 driver.
    /// - High: Motors disabled.
    /// - Low: Motors enabled.
    ///
    /// TODO:
    pub en_pin: PeripheralPinHandle,

    /// NOTE: We assume the motors are configured in the order they will show up
    /// in linear motions.
    pub motors: FixedVec<StepperMotorConfig, MAX_NUM_MOTORS>,
}

pub struct StepperMotorConfig {
    pub step_pin: PeripheralPinHandle,
    pub dir_pin: PeripheralPinHandle,
}

/// Executes a motion plan using one or more connected stepper motors
/// (using TMC2130s drivers).
///
/// NOTE: This only handles motion execution and not endstops.
///
/// This is meant to run as a service on its own thread/task with
/// StepperMotorController::run(). Clients communicate with the controller via
/// the StepperMotorControllerQueue.
///
/// Assumptions made:
/// - Steps take < ~200 seconds as we limited by the timer settings we use.
/// - A single LinearMotion only consists of each motor going in one direction
///   (we don't support flipping directions in the middle of a motion).
/// - A single LinearMotion starts and ends at an exact integer step position
///
/// Old Implementation details:
/// - Per-loop
///   - Initialize timer registers and wire up PPI (don't start timers yet).
/// - Main Loop
///   - Idle State
///     - Wait for data to be present
///     - When it is available,
///       - Generate CC registers starting at second step.
///       - Manually trigger first STEP
///       - Start timers
///       - Transition to Running state.
///   - Running State
///     - State:
///       - Current linear motion
///       - Current step position for each motor
///       - For each motor, CC registers offset and number of steps currently
///         enqueued.
/// - Each motor is driven by a TIMER peripheral @ 16MHz in 32-bit BITMODE.
///   - The CC registers are used as a cyclic queue to store the time at which
///     the next step should start.
///   - As we use dedge = 1, when the CC[i] time is hit, we use PPI to make the
///     STEP pin toggle from high to low and then back on the next event.
///   - We will buffer as many steps further as possible in the CC registers
///     (4-6 depending on the timer) to avoid waking up very frequently.
///   - When a single motor finishes half of the its steps, the thread is woken
///     up by an interrupt and we buffer more steps for all motors.
///   - When we are done all steps, we are also woken up by an interrupt and we
///     stop all timers.
///   - This design is meant to reduce the number of times the thread is woken
///     up and to make this thread more tolerant to waking up late (e.g. if
///     another thread is blocking for a while).
/// - Transitioning between motions
///   - When we have registered the CC register for the final step for each
///     motor, we will wait for an interrupt to occur for the last STEP pulse
///     out of all the motors.
///   - When the interrupt happens, we will prepare the steps for the next
///     motion. At this point, all the CC registers should be un-used.
///   - And we also flip the DIR pin for all motors if needed.
///     - A potential risk is that the STEP pulse from the last motion was only
///       recently triggered and we might flip the DIR too soon.
///     - Worst time we need to wait 1 16MHz PPI clock cycle and 20 ns (the
///       TMC2130 min DIR hold time) before it is safe to change it after the
///       interrupt occurs. THis is ~5-6 CPU clock cycles.
///     - Our code takes much longer than that many CPU clock cycles to setup
///       the next motions so this is not going to be an issue.
pub struct StepperMotorController<'a> {
    queue: &'a StepperMotorControllerQueue,

    timer: &'static mut TIMER0_REGISTERS,

    timer_interrupt: Interrupt,

    motor_states: FixedVec<MotorState, MAX_NUM_MOTORS>,
}

struct MotorState {
    dir_pin: GPIOPin,

    /// Current position in step units of this motor.
    ///
    /// TODO: Have a host side check to ensure that positions fit within 32
    /// signed bits.
    current_position: i32,

    /// Either
    ///  -1 if this motor is going in reverse.
    ///   1 if this motor is going forwards
    ///   0 if this motor is not moving in the current_motion.
    direction_increment: i32,

    prev_direction_increment: i32,

    /// Index of the CC register used to store the time at which the next step
    /// for this motor should start.
    step_cc_register: usize,

    /// Index of the PPI channel which will trigger the motor's STEP pin to
    /// toggle when the timer's counter is equal to the CC register.
    step_ppi_channel: usize,
}

enum LoopState {
    Stopped,
    Running {
        /// The current motion being executed.
        current_motion: LinearMotion,

        /// The start time of the current_motion relative to the start of the
        /// 16MHz timers.
        start_time: u32,

        /// Whether or not we have started the TIMER yet. Will only be false
        /// before the first step is planned.
        timer_running: bool,
    },
}

impl<'a> StepperMotorController<'a> {
    /// Creates a new motor controller while initializing any registers to a
    /// well defined initial state.
    ///
    /// NOTE: The motor drivers EN pin shouldn't be driven low (enabled) until
    /// this function is complete.
    pub fn new(
        queue: &'a StepperMotorControllerQueue,
        mut config: StepperMotorControllerConfig,
    ) -> Self {
        // let mut en_pin = config.gpio.pin(config.en_pin);
        // // Initially powered off.
        // en_pin
        //     .set_direction(PinDirection::Output)
        //     .write(PinLevel::High);

        let mut next_cc_register = 0;

        let mut next_ppi_channel = 0;

        let mut next_gpiote_channel = 0;

        let mut motor_states = FixedVec::new();

        // TODO: Enable interrupts for all CC registers.

        for motor in config.motors.into_iter() {
            config.timer.tasks_stop.write_trigger(); // Make sure initially stopped.
            config.timer.mode.write_timer();
            config.timer.bitmode.write_32bit();
            config.timer.prescaler.write(0); // Full 16MHz

            let step_pin_num = motor.step_pin.pin() as u32;
            let step_pin_port = motor.step_pin.port() as u32;

            let mut step_pin = config.gpio.pin(motor.step_pin);
            step_pin
                .set_direction(PinDirection::Output)
                .write(PinLevel::Low);

            let mut dir_pin = config.gpio.pin(motor.dir_pin);
            dir_pin
                .set_direction(PinDirection::Output)
                .write(PinLevel::Low);

            let step_gpiote_channel = next_gpiote_channel;
            next_gpiote_channel += 1;

            // Wire next GPIOTE channel to the STEP pin.
            // Triggering TASKS_OUT flips the output level of the pin.
            config.gpiote.config[step_gpiote_channel].write_with(move |v| {
                v.set_port(step_pin_port)
                    .set_psel(step_pin_num)
                    .set_polarity_with(|v| v.set_toggle())
                    .set_mode_with(|v| v.set_task())
            });

            let step_ppi_channel = next_ppi_channel;
            next_ppi_channel += 1;

            let step_cc_register = next_cc_register;
            next_cc_register += 1;

            // Trigger a STEP GPIO toggle on the CC register's COMPARE event.
            config.ppi.ch[step_ppi_channel].eep.write(unsafe {
                core::mem::transmute::<&EventRegister, u32>(
                    &config.timer.events_compare[step_cc_register],
                )
            });
            config.ppi.ch[ppi_channel].tep.write(unsafe {
                core::mem::transmute::<&mut TaskRegister, u32>(
                    &mut config.gpiote.tasks_out[step_gpiote_channel],
                )
            });

            // TODO: Move this to the motion setup code.
            config
                .ppi
                .chenset
                .write(CHENSET_WRITE_VALUE::from_raw(1 << ste_ppi_channel));

            // TODO: To enable/disable a motor, we need to enable/disable the intterupt and
            // the PPI channel.

            motor_states.push(MotorState {
                dir_pin,
                current_position: 0,
                prev_direction_increment: 0,
                direction_increment: 0,
                step_cc_register,
                step_ppi_channel,
            });
        }

        Self {
            queue,
            timer: config.timer,
            timer_interrupt: config.timer_interrupt,
            motor_states,
        }
    }

    // TODO: Which piece of code should be responsible for performing a graceful
    // slow down if we run low on

    /*
    So the simple solution is to just have one
    */

    pub async fn run(mut self) {
        // TODO: If we stop on a non-even number of CC steps, we need to apply an offset
        // to the next run.

        // TODO: Verify we don't overflow the max time between steps.

        // Set up all the timers. registers (don't start the timers yet)

        // Wire up the PPI to fire the GPIOs

        // Check if the there data to be processed and we are

        /*
        The big challenge:
        - In between motions, if the direction changes, how do we handle that?
            - Need to stop on the last step from the previous motion and prepare the first step in the timer
                - Challenge is making sure that we change the direction pin only AFTER the last motion's last step is done.

        GPIOTE is also used for pin interrupts

        In terms of conserving PPIs, we could use FORK to make one channel do multiple things

        Simpler solution:
        - Use a single timer for all motors.
        - Use a single CC register per motor.
        - DIR changes performed manually.

        */

        let mut state = LoopState::Stopped;

        loop {
            match &mut state {
                LoopState::Stopped => {
                    let intent = self.queue.intent.lock().await;
                    if intent != StepperMotorControllerIntent::Run {
                        intent.wait().await;
                        continue;
                    }

                    let mut inputs_queue = self.queue.inputs_queue.lock().await;

                    let current_motion = inputs_queue.pop_front().unwrap();
                    drop(inputs_queue);
                    drop(intent);

                    // TODO: Increase the type.
                    let current_time: u32 = 0;

                    self.timer.tasks_clear.write_trigger();

                    // TODO: Set per motor position and alter the directions as needed.
                    for i in 0..self.motor_states.len() {
                        let mut motor_state = &mut self.motor_states[i];
                        motor_state.timer.tasks_clear.write_trigger();
                        motor_state.current_position = current_motion.start_position[i] as i32;
                        motor_state.direction_increment = ((current_motion.end_position[i] as i32)
                            - motor_state.current_position)
                            .signum();

                        if motor_state.direction_increment > 0 {
                            motor_state.dir_pin.write(PinLevel::High);
                        } else {
                            motor_state.dir_pin.write(PinLevel::Low);
                        }
                    }

                    state = LoopState::Running {
                        current_motion,
                        start_time: 0,
                        timer_running: false,
                    };

                    continue;

                    // TODO: We should increase all the time CC values by 1 so
                    // that we can also handle the
                    // initial case.

                    /*
                    let mut current_time = timer.now();

                    let start_position = motion.start_position[0] as i32;

                    assert_no_debug!(start_position == current_position);

                    let end_position = motion.end_position[0] as i32;

                    let step_dir = 1; // Forwards

                    while current_position != end_position {
                        let next_position = current_position + step_dir;

                        let end_time = current_time.add_seconds(cnc::kinematics::time_to_travel(
                            (next_position - start_position) as f32,
                            motion.start_velocity[0],
                            motion.acceleration[0],
                        ));

                        step_pin.write(PinLevel::High);
                        for i in 0..10 {
                            unsafe { asm!("nop") };
                        }
                        step_pin.write(PinLevel::Low);

                        timer.wait_until(end_time).await;

                        current_position = next_position;
                    }

                    */
                }
                LoopState::Running {
                    current_motion,
                    start_time,
                    timer_running,
                } => {
                    // If not started,
                    // - Set direction_step
                    // - Setup INTENSET/CLR (should be ok to always have them)
                    // - Enable/disable PPI
                    // - Schedule the
                    //

                    for i in 0..self.motor_states.len() {
                        let mut motor_state = &mut self.motor_states[i];

                        if self.timer.events_compare[motor_state.step_cc_register]
                            .read()
                            .is_generated()
                        {
                            self.timer.events_compare[motor_state.step_cc_register]
                                .write_notgenerated();
                            motor_state.current_position += motor_state.prev_direction_increment;
                        }

                        motor_state.prev_direction_increment = motor_state.direction_increment;

                        // TODO: Check if we are complete with this motor.
                        // (in particular if we about to complete)

                        // TODO: Only need to do this stuff if the event was triggered or this is
                        // the first one?

                        // TODO: I need a special case for the first step in each motion that waits
                        // for the prior one to finish? (could be done outside this loop).
                        // ^ Must verify that we always clear any prior step events though.

                        let next_position =
                            motor_state.current_position + motor_state.direction_increment;

                        // TODO: Verify on the host that no motion takes too long to run.
                        // TODO: Document why we do the + 1.
                        let relative_end_time = (cnc::kinematics::time_to_travel(
                            (next_position - (current_motion.start_position[i] as i32)) as f32,
                            current_motion.start_velocity[i],
                            current_motion.acceleration[i],
                        ) * 16_000_000.0) as u32
                            + 1;

                        let end_time = (*start_time).wrapping_add(relative_end_time);

                        // TODO: THis will actually be wrong if we accidentally override the first
                        // step.
                        self.timer.cc[motor_state.step_cc_register].write(end_time);
                    }

                    // TODO: If we don't need all of the CC registers, then need to set the unused
                    // ones to a far away value so that they don't flip the pin.

                    // Start the timers for all motors with some motion happening.
                    // NOTE: This is done in its own loop to ensure that all the timers are roughly
                    // synchronized to each other.
                    if !timer_running {
                        // TODO: I might as well clear all the events right here (and wait the no-op
                        // period)

                        self.timer.tasks_start.write_trigger();
                        *timers_running = true;
                    }

                    // Wait for at least one CC register to trigger.
                    executor::interrupts::wait_for_irq(self.timer_interrupt).await;
                }
            }

            // let state =
        }
    }
}
