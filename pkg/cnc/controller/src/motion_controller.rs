use std::time::Instant;
use std::time::Duration;
use std::sync::Arc;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::lock;
use executor::sync::AsyncMutex;
use executor::channel;
use cnc::linear_motion_planner::*;
use math::matrix::VectorXd;
use math::vecxd;
use peripherals_proto::peripherals::StepperMotorMotion_Direction;
use peripherals_proto::peripherals::StepperMotorStatus_StoppedReason;
use peripherals_proto::peripherals::StepperMotorMotion;
use peripherals_service::device::PeripheralsDevice;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use cnc_controller_proto::cnc::*;
use executor::child_task::ChildTask;
use executor::sync::AsyncVariable;
use cnc::constrained_vector::constrained_vector;
use cnc::quadratic_stepper_motion::QuadraticStepperMotion;

use crate::motion_utils::{from_motor_space, from_motor_space_f64};
use crate::devices::DevicesController;
use crate::tmc2209::TMC2209Device;
use crate::stepper_motion_generator::*;
use crate::time::DeviceTime;
use crate::time::DevicesTimeVector;
use crate::proto_utils::{VectorProtoExt, LinearMotionProtoExt};
use crate::logging::*;

/*

- Need some minimum time between a motion being added and use being allowed to start it (since we need time to enqueue it)

TODO: Should also have a concept of max time to the next step.

TODO: Will need to do some coordination with other components like the fan controller.
- Once a motion is fully constrained, this needs to phone back the timing 

Doing Homing:
- We will check all diag pins each time we check the motor statuses
- If we see one, we will stop all motors (clear_motion and disable and re-enable the motors)
    - Then report to the 


TODO: Need TMC2209 retries
*/

/// Basically the maximum number of times MotionController::move_to() can
/// be called in quick succession before we block additional requests.
const MAX_PLANNER_QUEUE_LENGTH: usize = 128;

/// When we are currently idle, if this amount of time passes since receiving
/// the first motion request, we transition to the 'Active' state (start enqueuing
/// motions on the MCU).
///
/// TODO: Also timeout if the planner queue is full or MIN_MOTION_START_DELAY amount of fully constrained moves are defined.
const IDLE_START_TIMEOUT: Duration = Duration::from_millis(200);

/// Relative to the current point in time, the earliest time in the future at
/// which we are allowed to enqueue a new motion to start executing on the
/// motor controller.
///
/// This must be larger than the worst case clock drift and request RTT with
/// the motor MCUs to avoid scheduling a motion in the past.
const MIN_MOTION_START_DELAY: Duration = Duration::from_millis(400);

/// Relative to the current point in time, how many seconds of motions in the
/// LinearMotionPlanner we will consume when in the 'Active' state.
///
/// MUST BE >> MIN_MOTION_START_DELAY
const PLANNER_LOOK_AHEAD_WINDOW: Duration = Duration::from_millis(4000);

pub(super) const PLANNER_STEP_SIZE: f64 = 1.0;

/*
/// Minimum amount of time we will grab from the planner.
///
/// MUST BE < PLANNER_LOOK_AHEAD_WINDOW
///
/// Basically if we initially consume 'PLANNER_LOOK_AHEAD_WINDOW' we won't
/// consume more motions until 'PLANNER_MIN_TIME_STEP' more time has elapsed.
/// This sets a limit on the minimum chunk duration used for splitting up
/// very long motions. 
///
/// TODO: Actually if it ends in a small bit left, we may still stop early.
const PLANNER_MIN_TIME_STEP: Duration = Duration::from_millis(200);
*/

/// Relative to the current point in time, how many seconds of motions
/// will be converted to steps and enqueud to run on the motors.
///
/// MUST BE < PLANNER_LOOK_AHEAD_WINDOW since the step generator is fed
/// motions from the planner. Otherwise it will plan for empty time. 
///
/// MUST BE >> MIN_MOTION_START_DELAY
const STEP_GENERATION_WINDOW: Duration = Duration::from_millis(800);

// TODO: Maybe switch back to using a Duration for this and only switch to a f64 at the end.
pub(super) const STEP_GENERATION_STEP: f64 = 0.1;

const POLL_INTERVAL: Duration = Duration::from_millis(50);


// pub struct MotionControllerOptions {
//     pub config: MotionControllerConfig,
// }

pub struct MotionController {
    task: TaskResource,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(MotionController, task);

struct Shared {
    config: Arc<MotionControllerConfig>,

    devices: Arc<DevicesController>,

    motors: Vec<Arc<TMC2209Device>>,

    state: AsyncVariable<State>,

    logging_channel: Arc<LoggingChannel>
}

struct State {
    mode: MotionControllerMode,

    /// When non-None, the backthread is currently attempting to perform a state transition away from
    /// 'mode' to 'next_mode'.
    next_mode: Option<MotionControllerMode>,

    /// User desired mode. 'mode' will eventually become this value.
    /// This will become None once the backthread notices the value and starts
    /// the transition by putting it into next_mode.
    desired_mode: Option<MotionControllerMode>,

    /// 
    position_offset: VectorXd,

    /// If planner is non-empty and we are idle, the time at which
    /// the first motion in planner was added. 
    first_motion_time: Instant,

    planner: MotionControllerLinearPlanner,

    active: bool,

    /// Position of each motor in step units.
    ///
    /// When all motors are at position 0, we are at (0,0,0) in XYZ,etc. space.
    ///
    /// If active_state != None, then this is the position we were at before entering the
    /// active state.
    ///
    /// TODO: Instead of storing this, just always store a StepperMotionGenerator instance
    /// and rely on that for this data.
    motor_positions: Vec<i32>,

    /// When motor_positions[i] is 0, the position recorded
    /// on the MCU is motor_position_offsets[i]
    motor_position_offsets: Vec<i32>,

    /// If the motors were externally stopped, this is the time at which the 'endstop' was hit.
    ///
    /// TODO: THis needs to be frequently reset.
    ///
    /// TODO: Instead give the caller an API for quering the step history?
    hit_time: Option<DevicesTimeVector>,

    hit_position: Option<VectorXd>,
}

enum Action {
    Motions(MotionControllerLinearPlanner),
    SetPosition(VectorXd),
    
}


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MotionControllerMode {
    /// All motors are off.
    /// We aren't doing anything.
    Disabled,
    
    /// All motors are on.
    /// In this mode, we will accept motion commands and monitor for
    /// external exits via stop_motors().
    Enabled,
    
    /// An unexpected event has occured.
    /// Like Disabled but requires explicit exit from Alarm mode.
    Alarm,
}

struct ActiveState {
    start_time: Instant,

    start_primary_time: DeviceTime,

    planner_consumed_time: f64,

    /// Up to what point in time in the queue we have consumed commands.
    queue_consumed_time: f64,

    queue: StepperMotionGenerator,
}

/// NOTE: This is exclusively owned by the main background thread so that most operations can be
/// done without locking the shared state. 
struct EnabledState {
    /// Previously sent motions.
    past_step_motions: Vec<VecDeque<(DeviceTime, QuadraticStepperMotion)>>,

    /// Steps we have prepared but we can't yet enqueue since the motor has no space.
    pending_step_motions: Vec<VecDeque<(DeviceTime, QuadraticStepperMotion)>>,

    /// Whether or not pending_step_motions is empty.
    have_pending_motions: bool,

    /// Last direction sent to each motor.
    /// (this is mainly used for compressed unchanged direction commands)
    last_directions: Vec<StepperMotorMotion_Direction>,

    /// When this is present, 'active_state.queue.motor_positions()' contains the
    /// motor position after all 'pending_step_motions' have been executed.
    active_state: Option<ActiveState>,
}

impl EnabledState {
    fn reset(&mut self) {
        for q in &mut self.past_step_motions {
            q.clear();
        }

        for q in &mut self.pending_step_motions {
            q.clear();
        }
        self.have_pending_motions = false;

        // TODO: Also reset last_directions?
    }

    /// Tries to determine where all the motors where located at the given point in time.
    ///
    /// The returned motor positions are sub-step interpolated based on how far into the step
    /// we have gone.
    ///
    /// Internally this basically works by rewinding the step motion history until we find the
    /// steps that were sent closest to the requested time.
    ///
    /// - final_inactive_motor_positions: 
    fn position_at_time(&self, final_inactive_motor_positions: &[i32], target_motor_times: &[DeviceTime]) -> Vec<f64> {
        // The current motor positions we are looking at. This is initialized
        // to point to the final 
        let mut cur_motor_positions = {
            let mut pos = final_inactive_motor_positions;
            if let Some(active_state) = &self.active_state {
                pos = active_state.queue.motor_positions();
            }
            
            pos
            .iter()
            .map(|v| *v as f64)
            .collect::<Vec<f64>>()
        };

        // TODO: Handle this. this is tecnhically possible if we see multiple hits.
        // if hit_time > now {
        //     eprintln!("HIT TIME: {:?}, NOW: {:?}", hit_time, now);
        // }

        // let hit_delta = now - hit_time;
        // println!("HIT DELTA: {:?}", hit_delta);

        // TODO: This code would probably be simpler if we stored the original DeviceTimes used to create the motions.

        for motor_i in 0..cur_motor_positions.len() {
            
            let motor_target_time = target_motor_times[motor_i];

            // Find the first motion that starts before the motor_target_time.

            let past_motions = self.pending_step_motions[motor_i].iter().rev()
                .chain(self.past_step_motions[motor_i].iter().rev());

            let mut corrected = false;

            for (motion_start_time, motion) in past_motions {
                let sign: f64 = if motion.num_steps.direction() { 1.0 } else { -1.0 };

                // Undo the movement of this motion.
                cur_motor_positions[motor_i] -= sign * (motion.num_steps.count() as f64);

                // Go back further if the hit was before this motion
                if motor_target_time < *motion_start_time {
                    continue;
                }

                // let start_delta = cnc::time_difference_u32(motor_target_time, motion.next_step_time);
                // if start_delta < 0 {
                //     continue;
                // }

                let mut motion = motion.clone();

                let mut found = false;

                let mut step_start_time = motion_start_time.clone();

                while motion.num_steps.count() > 1 {
                    // let step_start_time = motion.next_step_time;
                    let step_dur = motion.next_step_duration;
                    let step_end_time = step_start_time.add_ticks(step_dur);
                    motion.next();
                    
                    // TODO: this is only valid for motions with at least 2 steps.
                    // let step_end_time = motion.next_step_time;

                    // step_end_time - motor_target_time


                    if step_end_time > motor_target_time {
                        let delta = ((motor_target_time.sub(step_start_time)).lower() as f64) / (step_dur as f64);

                        println!("[][][] Delta: {}", delta);
                        cur_motor_positions[motor_i] += delta * sign;
                        
                        // Partial completion of the step.

                        found = true;
                        corrected = true;
                        break;
                    }

                    step_start_time = step_end_time;
                    cur_motor_positions[motor_i] += sign;
                }

                if !found {
                    // TODO: This seems to happen a lot. (i'm guessing for X and Y which is a false alarm).
                    eprintln!("STOPPED ON A MOTION BOUNDARY!!");
                    cur_motor_positions[motor_i] += sign;
                }

                break;
            }

            if motor_i == 2 && !corrected {
                println!("MOTOR 2 NOT CORRECTED");
            }
        }

        cur_motor_positions
    }

    /// Garbage collects any past motion records that are very old.
    ///
    /// now_remote_times: Should be the list of the current remote time for each motor.
    fn cleanup_history(&mut self, now_remote_times: &[DeviceTime]) {
        for (motor_i, queue) in self.past_step_motions.iter_mut().enumerate() {
            while !queue.is_empty() {
                let (motion_start_time, motion) = &queue[0];

                // TODO: Check based on the last step time.
                if motion_start_time.add_duration(Duration::from_secs(2)) < now_remote_times[motor_i] {
                    queue.pop_front();
                } else {
                    break;
                }
            }
        }

    }
}


impl MotionController {

    /// Performs basic validation of the config and sets up final values.
    pub fn adjust_config(config: &mut MotionControllerConfig) -> Result<()> {
        let mut motor_indexes = HashMap::new();

        for (i, proto) in config.motors().iter().enumerate() {
            motor_indexes.insert(proto.device_name().to_string(), i);
        }

        let get_motor_index = |name: &str| -> Result<u32> {
            motor_indexes.get(name).map(|v| *v as u32)
                .ok_or_else(|| format_err!("Unknown motor named: {}", name))
        };

        let mut used_axes = HashSet::new();
        let mut max_axis = 0;

        let mut mark_used_axis = |index: u32| -> Result<()> {
            if !used_axes.insert(index) {
                return Err(err_msg("Multiple mapping for axis"));
            }

            max_axis = max_axis.max(index);
            Ok(())
        };


        // Setting up all motor indexes in the geometry so we don't need to
        // do name to index lookups later.
        for geometry in config.geometry_mut() {

            match geometry.geometry_case_mut() {
                AxisGeometryGeometryCase::Direct(v) => {
                    let i = get_motor_index(v.motor_name())?;
                    v.set_motor_index(i);

                    mark_used_axis(v.axis_index())?;
                }
                AxisGeometryGeometryCase::CoreXy(v) => {
                    let a = get_motor_index(v.a_motor_name())?;
                    v.set_a_motor_index(a);
                    let b = get_motor_index(v.b_motor_name())?;
                    v.set_b_motor_index(b);

                    mark_used_axis(v.x_axis_index())?;
                    mark_used_axis(v.y_axis_index())?;
                }
                AxisGeometryGeometryCase::NOT_SET => {
                    return Err(err_msg("Undefined geometry"));
                }
            }
        }

        if used_axes.is_empty() {
            return Err(err_msg("No axes defined"));
        }
        if (max_axis + 1) as usize != used_axes.len() {
            return Err(err_msg("Some axis indexes skipped"));
        }

        if (max_axis + 1) as usize != config.axes().len() {
            return Err(err_msg("Bad axes list in config"));
        }

        Ok(())
    }

    pub async fn create(
        mut config: MotionControllerConfig,
        devices: Arc<DevicesController>,
        logging_channel: Arc<LoggingChannel>,
    ) -> Result<Self> {
        Self::adjust_config(&mut config)?;

        let config = Arc::new(config);

        let mut motors = vec![];

        for (i, proto) in config.motors().iter().enumerate() {
            let dev = devices.get_motor(proto.device_name()).await?;
            motors.push(dev);
        }

        let shared = Arc::new(Shared {
            config: config.clone(),
            devices,
            motors,
            state: AsyncVariable::new(State {
                mode: MotionControllerMode::Disabled,
                next_mode: None,
                desired_mode: None,
                position_offset: VectorXd::zero_with_shape(config.motors().len(), 1),
                first_motion_time: Instant::now(), // doesn't matter
                planner: MotionControllerLinearPlanner::new(config.clone()),
                active: false,
                motor_positions: vec![0; config.motors().len()],
                motor_position_offsets: vec![0; config.motors().len()],
                hit_time: None,
                hit_position: None,
            }),
            logging_channel
        });

        let task = TaskResource::spawn_interruptable(
            "MotionController",
            Self::background_thread(shared.clone())
        );

        Ok(Self {
            task,
            shared
        })
    }

    /*
    General Idea:

    - I can append to the planner a list of motions with all the motion end points aligned to step endpoints.
    - There is still a risk that if I take only part of a motion, then it will be put back into the pool with a very small segment remaining that may be zero or one steps long.

    - Either way, issue is that we can't gracefully split up segments.
        - But we do need to split up segments

    - Getting step times:


    */

    /*

    Handling state transitions:
    - Some endstops will only be polled during homing mode:
    - 

    TODO: Need to verify initial state of all endstops is ok.


    Automating set_position:
    - In homing mode,
        - Have rules of the form 'when endstop N is hit, we are at position (X, Y, Z)'
            - Need a back in time queue to see what motor position was reached at each time.

        - Stepper motion generator isn't good for this since it measures positions prior to execution

    TODO: NEed to wait for endstops to become 'ready' before we do anything with them (minimally the request to poll them has landed on the MCU)
    */

    // TODO: It is very bad if we e-stop and the planenr and queue positions get out of sync since then the queue won't be able to give out sane step times (likely to be overlapping)


    async fn background_thread(shared: Arc<Shared>) -> Result<()> {

        let mut enabled_state = None;

        loop {
            let cycle_start = Instant::now();

            let (mode, next_mode) = lock!(state <= shared.state.lock().await?, {

                if let Some(mode) = state.desired_mode.take() {
                    state.next_mode = Some(mode);
                }

                if let Some(mode) = state.next_mode.clone() {
                    if mode == state.mode {
                        state.next_mode = None;
                    }
                }

                (state.mode.clone(), state.next_mode.clone())
            });

            // State transitions.
            if let Some(next_mode) = next_mode {

                println!("Transition to {:?}", next_mode);

                match next_mode {
                    MotionControllerMode::Disabled | MotionControllerMode::Alarm => {
                        // Disable motors.
                        for motor in shared.motors.iter().rev() {
                            motor.disable().await?;
                        }
                    }

                    MotionControllerMode::Enabled => {

                        for motor in shared.motors.iter() {
                            // TODO: If we stopped without syncing motor positions with the MCU, we may need to do that now (may also need to wait for any active moves to stop).

                            // This will set the STOPPED reason to HOST_CLEAR. The first
                            // status checks in the 'Enabled' state will notice this and resync
                            // the motor position and clear all pending motions.
                            motor.clear_stepper_queue().await?;
                        }

                        let mut past_step_motions = vec![];
                        for _ in shared.config.motors() {
                            past_step_motions.push(VecDeque::new());
                        }

                        let mut pending_step_motions = vec![];
                        for _ in shared.config.motors() {
                            pending_step_motions.push(VecDeque::new());
                        }
                        let mut have_pending_motions = false;

                        let mut last_directions = vec![StepperMotorMotion_Direction::UNCHANGED; shared.config.motors().len()];

                        enabled_state = Some(EnabledState {
                            past_step_motions,
                            pending_step_motions,
                            have_pending_motions,
                            last_directions,
                            active_state: None,
                        });
                    }
                }

                // TODO: Actually change the mode in the state.

                // TODO: Can't change to enabled until we are done getting the 
                lock!(state <= shared.state.lock().await?, {
                    // TODO: Don't configure a next mode of Enabled until the cycles finish successfully (since we need to resync motor positions).
                    state.mode = next_mode;
                    state.next_mode = None;
                });
            }

            match mode {
                MotionControllerMode::Disabled | MotionControllerMode::Alarm => {
                    // Nothing to do.
                }

                MotionControllerMode::Enabled => {
                    Self::cycle_enabled(&shared, enabled_state.as_mut().unwrap(), cycle_start).await?;

                    // TODO: It is an error if not all moving motors are stopped, but it's hard to determine that due to other issues.
                }

            }

            let cycle_end = Instant::now();

            let cycle_dur = cycle_end - cycle_start;

            // TODO: Bring back.
            // if sent_something > 0 {
                // println!("Cycle: {:.2?} ; {:?}", cycle_dur, mode);
            // }



            // Wait for time period or state change.
            // TODO
            let cycle_time = cycle_end - cycle_start;
            if cycle_time < POLL_INTERVAL {

                let max_sleep = POLL_INTERVAL - cycle_time;

                {
                    let state = shared.state.lock().await?.read_exclusive();

                    if state.desired_mode.is_some() {
                        continue;
                    }

                    let _ = executor::timeout(max_sleep, state.wait()).await;
                }
            }
        }
    }

    /// Returns true if the state has stabilized and the 
    async fn cycle_enabled(
        shared: &Shared, enabled_state: &mut EnabledState, cycle_start: Instant
    ) -> Result<bool> {
        
        // TODO: All requests in here need a timeout.

        let state_responses = {
            let mut batch = shared.devices.new_batch();

            for (motor_i, motor) in shared.config.motors().iter().enumerate() {
                let dev = &shared.motors[motor_i];
                batch.add(dev.device_name(), dev.get_stepper_motor_status_request()?);
            }

            executor::timeout(Duration::from_millis(2000), batch.send())
            .await
            .map_err(|_| err_msg("state responses timed out"))??
        };
        let mut state_responses_i = 0;

        let mut statuses = vec![];
        let mut some_motors_active = false;
        let mut need_reset = false;

        for motor_i in 0..shared.config.motors().len() {
            let dev = &shared.motors[motor_i];
            let s = state_responses[state_responses_i].stepper_status();
            state_responses_i += 1;

            match s.stopped() {
                // TODO: Handle unknown reasons?

                StepperMotorStatus_StoppedReason::NONE => {},
                StepperMotorStatus_StoppedReason::HOST_CLEAR => {
                    need_reset = true;
                }
                StepperMotorStatus_StoppedReason::TIMING_FAULT => {
                    // TODO: Go into alarm mode.
                    // (clear everything + )

                    println!("[ALARM] TIMING_FAULT for motor: {}", shared.config.motors()[motor_i].device_name());
                    println!("Status: {:?}", s);

                    lock!(state <= shared.state.lock().await?, {
                        state.desired_mode = Some(MotionControllerMode::Alarm);
                    });

                    return Ok(false);
                }

            }

            some_motors_active |= s.active();

            statuses.push(s.clone());
        }

        if need_reset {


            // TODO: It is possible that this was triggered by an external alarm (e.g. from endstop so need to avoid starting again).

            // Wait for all the steppers to come to rest.
            if some_motors_active {
                return Ok(false);
            }

            // Ensure some time has elapsed since the motors were stopped (e.g. so that the drivers
            // notice any disabled signals).
            executor::sleep(Duration::from_millis(5)).await?;

            // NOTE: There is a risk that stop_motors() is called again in parallel to this which
            // may cause us miss some stops. If we need to go into the alarm state, we should end
            // up entering it shortly  

            for motor_i in 0..shared.config.motors().len() {
                if statuses[motor_i].stopped() != StepperMotorStatus_StoppedReason::NONE {
                    shared.motors[motor_i].reset_stepper_motor_queue().await?;
                    shared.motors[motor_i].enable().await?;
                }
            }

            // NOTE: Below in the lock we will also reset all planned motions so we
            // will ground completely to a halt 
        }


        // If we make it this far, then we are clear to worry about doing actual motion planning.

        let now = Instant::now();

        // TODO: Over the coarse of a print, we need to periodically perform time corrections
        // and not just at the beginning (when entering the active state).
        let now_primary_time;
        let now_remote_times = {
            // TODO: Still no guarantee that this will give a monotonic time.
            // TODO: Only pull times for devices used by motors.
            let device_times = shared.devices.time().to_all_device_times(now).await?;

            let mut out = vec![];
            for motor in &shared.motors {
                out.push(device_times
                    .get(motor.device_name())
                    .ok_or_else(|| err_msg("Missing motor device time"))?
                    .clone()
                );
            }

            now_primary_time = device_times.get(
                shared.devices.time().primary_device_name()).unwrap().clone();

            out
        };


        lock!(state <= shared.state.lock().await?, {
            let locked = Instant::now();

            if locked - now > Duration::from_millis(2) {
                println!("Lock Delay: {:?}", locked - now);
            }
            
            let state: &mut State = &mut state;

            if need_reset {
                // TODO: Must verify that all motors involved in a move have been stopped.
                // (if not, then we should enter a fault state).

                if let Some(hit_time) = state.hit_time.take() {
                    if enabled_state.active_state.is_none() {
                        eprintln!("NOT MOVING BUT HIT")
                        // TODO: Return an error since we should always be moving when doing a move.
                    }

                    let mut hit_motor_times = vec![];
                    for motor_i in 0..shared.config.motors().len() {
                        hit_motor_times.push(
                            hit_time.get(shared.motors[motor_i].device_name()).unwrap().clone()
                        );
                    }

                    let hit_motor_positions = enabled_state.position_at_time(
                        &state.motor_positions, &hit_motor_times
                    );

                    state.hit_position = Some(from_motor_space_f64(&hit_motor_positions, &shared.config));
                }



                enabled_state.reset();
                state.planner.clear();

                // Note that this is only valid if we assume that all motors that were moving have been stopped
                // simultaneously.
                if let Some(active_state) = enabled_state.active_state.take() {
                    state.motor_positions.copy_from_slice(active_state.queue.motor_positions());
                }
                state.active = false;


                // TODO: Whenever we do this, pull out the current motor positions before ending?
                // state.active_state = None;

                // Retrieve the current position of each motor from the MCU since we don't know
                // how many of the planned steps were exactly completed.
                for motor_i in 0..shared.config.motors().len() {
                    if statuses[motor_i].stopped() != StepperMotorStatus_StoppedReason::NONE {
                        let mut pos = statuses[motor_i].position();
                        if shared.config.motors()[motor_i].inverted() {
                            pos = -pos;
                        }

                        state.motor_positions[motor_i] = pos -
                            state.motor_position_offsets[motor_i];
                    } else {
                        // TODO: Check position unchanged.
                    }
                }

                state.planner.set_start_position(from_motor_space(&state.motor_positions, &shared.config));
            }

            // tODO: Occasionally verify the motor position on the MCU is synced with our local estimate (for all non-active motors)


            // TODO: Currently there is not much value to having the 'active_state'

            // TODO: Need checks to ensure that the queue is sufficiently full.
            // If we are active and the queue becomes empty, we need to delay future steps?

            if enabled_state.active_state.is_none() {
                if !state.planner.is_empty() && state.first_motion_time + IDLE_START_TIMEOUT >= now {

                    let start_time = now + MIN_MOTION_START_DELAY;

                    println!("Now time: {:?}", now_remote_times);

                    let mut remote_start_time = now_remote_times.clone();
                    for t in &mut remote_start_time {
                        *t = t.add_duration(MIN_MOTION_START_DELAY)
                    }
                    let start_primary_time = now_primary_time.add_duration(MIN_MOTION_START_DELAY);

                    println!("-> Active : {:?}", remote_start_time);

                    state.planner.set_start_time(0.0);

                    if shared.logging_channel.active() {
                        let mut entry = LogEntry::default();
                        let e = entry.motion_start_mut();
                        
                        let position = state.planner.start_position() + &state.position_offset; 
                        e.set_time(start_primary_time.raw());
                        e.set_position(position.to_proto());
                        // TODO: Include the motor_position_offsets?
                        e.motor_position_mut().extend_from_slice(&state.motor_positions[..]);

                        shared.logging_channel.send(entry);
                    }

                    // TODO: Need to preserve motor positions across queue initializations.
                    state.active = true;
                    enabled_state.active_state = Some(ActiveState {
                        start_time,
                        start_primary_time,
                        planner_consumed_time: 0.0,
                        queue_consumed_time: 0.0,
                        queue: StepperMotionGenerator::new(
                            shared.config.clone(),
                            &state.motor_positions,
                            &remote_start_time
                        )
                    });
                }
            }

            if let Some(active_state) = &mut enabled_state.active_state {
                // TODO: Also check that the queue of commands to send is empty?
                if state.planner.is_empty() && active_state.queue.is_empty() {
                    if !some_motors_active {
                        println!("=> Idle");

                        state.motor_positions.copy_from_slice(active_state.queue.motor_positions());
                        state.active = false;
                        enabled_state.active_state = None;

                        if shared.logging_channel.active() {
                            let mut entry = LogEntry::default();
                            let e = entry.motion_end_mut();
                            
                            let position = state.planner.start_position() + &state.position_offset; 
                            // TODO: Make this more tight to the time of the final motion?
                            e.set_time(now_primary_time.raw());
                            e.set_position(position.to_proto());
                            // TODO: Include the motor_position_offsets?
                            e.motor_position_mut().extend_from_slice(&state.motor_positions[..]);

                            shared.logging_channel.send(entry);
                        }

                        // TODO: Have a 'continue' here?
                    }
                    
                } else if !enabled_state.have_pending_motions {
                    
                    // TODO: Bring this back in some form.
                    /*
                    if active_state.next_motion_time < now + MIN_MOTION_START_DELAY {
                        // TODO: This is only an error if we get the motions within the time window
                        // and still failed to schedule them.
                        eprintln!("Motions queueing too slow!");
                        // TODO: Need to fix this to handle the queue.
                        // active_state.next_motion_time = now + MIN_MOTION_START_DELAY;
                        // active_state.next_motion_remote_time = now_mcu_time_full.add_duration(MIN_MOTION_START_DELAY);

                    }
                    */

                    // Take from 'planner' and push into the 'queue'
                    {
                        let max_planner_time = ((now + PLANNER_LOOK_AHEAD_WINDOW) - active_state.start_time).as_secs_f64();
                        // TODO: I don't need planner_consumed_time. I can instead just look at the start_time in the planner.
                        let max_planner_time_step = max_planner_time - active_state.planner_consumed_time;

                        if max_planner_time_step >= PLANNER_STEP_SIZE {
                            active_state.planner_consumed_time += PLANNER_STEP_SIZE;
                            
                            let mut base_time = state.planner.start_time();

                            // TODO: Check these comments.
                            // NOTE: We only use the end_position and time in each of these objects.
                            let mut motions = vec![];
                            // TODO: Need to use the duration returned by this (yes since we don't know if we fully saturated the time span)
                            state.planner.next(active_state.planner_consumed_time, &mut motions);

                            if motions.len() > 0 && shared.logging_channel.active() {
                                let mut entry = LogEntry::default();

                                for motion in &motions {
                                    let m = entry.linear_motions_mut().new_motions();
                                    m.set_time(
                                        active_state.start_primary_time
                                        .add_secs(base_time)
                                        .raw()
                                    );

                                    m.set_motion(motion.to_proto());

                                    base_time += motion.duration;
                                }

                                shared.logging_channel.send(entry);
                            }

                            for motion in motions {
                                // TODO: Should warn if the motion was delayed relative to the previous one.
                                active_state.queue.enqueue(motion);
                            }
                        }
                    }

                    // TODO: Maybe propagate back the latest machine position to the planner as the new planner
                    // start_position to account for drift due to step quantization.

                }
                
            }
        });

        // Take from the 'queue' and prepare to send to the machine.
        if let Some(active_state) = &mut enabled_state.active_state {
            // TODO: Limit the minimum time step for this?

            let max_queue_time = ((now + STEP_GENERATION_WINDOW) - active_state.start_time).as_secs_f64();
            let max_queue_time_step = max_queue_time - active_state.queue_consumed_time;

            // TODO: Entire alarm mode if this fails.
            if max_queue_time_step >= STEP_GENERATION_STEP {
                active_state.queue_consumed_time += STEP_GENERATION_STEP;
                let step_motions = active_state.queue.to_commands(active_state.queue_consumed_time).unwrap();

                for (i, step_motion) in step_motions.into_iter().enumerate() {
                    for step_motion in step_motion {
                        // println!("Enqueue: {:?}", step_motion);

                        enabled_state.pending_step_motions[i].push_back(step_motion);
                        enabled_state.have_pending_motions = true;
                    }
                }
            }
        }

        // Do enqueues.

        // TODO: Ideally prioritize enqueues based on start time (k-way merge).
        let s = Instant::now();

        // TODO: Complain if we are ever not able to maintain at least Xms of motions across all motors enqueued
        // on the MCU.

        enabled_state.have_pending_motions = false;
        let mut sent_something = 0;
        let mut batch = shared.devices.new_batch();

        let mut log_entry = LogEntry::default();
        let should_log = shared.logging_channel.active();

        for (motor_i, queue) in enabled_state.pending_step_motions.iter_mut().enumerate() {
            let mut empty_slots = statuses[motor_i].empty_queue_slots() as usize;

            loop {
                if empty_slots == 0 {
                    break;
                }

                let (motion_time, motion) = match queue.pop_front() {
                    Some(v) => v,
                    None => break
                };

                let mut proto = StepperMotorMotion::default();


                // TODO: I don't understand why this hpapnes.
                // if motion_time < now_remote_times[motor_i].add_duration(Duration::from_millis(100)) {
                //     eprintln!("BAD MOTION TIME: {:?} vs {:?}", motion_time, now_remote_times[motor_i]);
                // }

                // TODO: Compress this to be a delta relative to the last time.
                proto.set_next_step_time(motion.next_step_time);
                proto.set_next_step_duration(
                    if motion.num_steps.count() == 1 { 0 } else { motion.next_step_duration });
                proto.set_step_duration_increment(motion.step_duration_increment);
                proto.set_num_steps_minus_one(motion.num_steps.count() - 1);

                let mut dir = motion.num_steps.direction();
                if shared.config.motors()[motor_i].inverted() {
                    dir = !dir;
                }

                let dir_proto = match dir {
                    true => StepperMotorMotion_Direction::FORWARD,
                    false => StepperMotorMotion_Direction::BACKWARD
                };
                if dir_proto == enabled_state.last_directions[motor_i] {
                    proto.clear_direction();
                } else {
                    proto.set_direction(dir_proto);
                    enabled_state.last_directions[motor_i] = proto.direction();
                }

                if should_log {
                    let e = log_entry.stepper_motions_mut().new_motions();

                    let start_time = shared.devices.time().to_primary_clock(motion_time).await?;

                    e.set_motor_index(motor_i as u32);
                    e.set_start_time(start_time.raw());
                    e.set_motion(proto.clone());

                    // Never compressed in the log.
                    e.motion_mut().set_direction(dir_proto);

                    e.motion_mut().clear_next_step_time();
                }

                // TODO: These should be easy to sequence compress and acks should also be compressable.
                batch.add(
                    shared.motors[motor_i].device_name(),
                    shared.motors[motor_i].make_enqueue_stepper_motion(proto)?
                );

                enabled_state.past_step_motions[motor_i].push_back((motion_time, motion));

                empty_slots -= 1;
            }


            enabled_state.have_pending_motions |= !queue.is_empty();
        }

        if should_log && batch.len() > 0 {
            shared.logging_channel.send(log_entry);
        }

        // Assumption here is that the stepper queue length is smaller than the max in flight requests we allow.
        // TODO: Need to cap this since we are dealing with big queues now (and prioritize by start time).
        if batch.len() > 0 {
            sent_something += batch.len();

            // TODO: This can be optimized a bit since we are discarding the responses.
            batch.send().await
            .map_err(|e| {
                println!("NOW: {:?}", now);

                println!("CURRENT DEV TIME: {:?}", now_remote_times);

                let now2 = Instant::now();
                println!("DELTA: {:?}", (now2 - now).as_secs_f64());

                e                    
            })?;
        }

        if enabled_state.have_pending_motions {
            println!("Remote queue full!");
        }

        // Clean up all completed motion from the history.
        enabled_state.cleanup_history(&now_remote_times);

        let cycle_end = Instant::now();

        let cycle_dur = cycle_end - cycle_start;

        if sent_something > 0 {
            println!("Cycle: {:.2?} ; Enqueue {}", cycle_dur, sent_something);
        }

        Ok(true)
    }

    /*
        // TODO: Most important thing is to make sure we don't lose track of steps between the floating
        // point and step dimensions.

        // TODO: Need guarantees that we are only giving out monotonic timestamps to each motor.

        // TODO: Whenever in homing mode, it would be good to explicitly decrease all the motor currents a lot.
    }
    */

    pub async fn enable(&self, enabled: bool) -> Result<()> {

        let target_mode = if enabled { MotionControllerMode::Enabled } else { MotionControllerMode::Disabled };

        let alarm = lock!(state <= self.shared.state.lock().await?, {
            if self.will_reach_alarm_mode(&state) {
                return true;
            }

            state.desired_mode = Some(target_mode);
            state.notify_all();
            false
        });

        if alarm {
            return Err(err_msg("In alarm mode"));
        }

        self.wait_for_mode(|m| m == target_mode).await?;

        Ok(())

        // todo!()
    }

    fn will_reach_alarm_mode(&self, state: &State) -> bool {
        state.mode == MotionControllerMode::Alarm ||
        state.next_mode == Some(MotionControllerMode::Alarm) ||
        state.desired_mode == Some(MotionControllerMode::Alarm)
    }

    // TODO: This needs to block for us to exit alarm mode.
    pub async fn reset_alarm(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if self.will_reach_alarm_mode(&state) {
                state.desired_mode = Some(MotionControllerMode::Disabled);
                state.notify_all();
            }
        });

        self.wait_for_mode(|m| m != MotionControllerMode::Alarm).await?;

        Ok(())
    }

    async fn wait_for_mode<F: Fn(MotionControllerMode) -> bool>(&self, f: F) -> Result<()> {
        loop {
            let done = lock!(state <= self.shared.state.lock().await?, {

                let reached = f(state.mode) && state.next_mode.is_none() && state.desired_mode.is_none();
                if reached {
                    return Ok(true);
                }

                let mut might_reach = false;
                if let Some(m) = state.desired_mode {
                    might_reach = f(m);
                } else if let Some(m) = state.next_mode {
                    might_reach = f(m);
                }

                if !might_reach {
                    return Err(format_err!("Will not reach desired state : {:?}, {:?}, {:?}", state.mode, state.next_mode, state.desired_mode));
                }

                Result::<_, Error>::Ok(false)

            })?;

            if done {
                return Ok(());
            }

            // TODO: Speed me up
            executor::sleep(Duration::from_millis(100)).await?;
        }
    }


    /// Requests that the MotionController enter into alarm mode.
    ///
    /// Note that this returns quickly and DOES NOT block for the transition to complete.
    pub async fn trigger_alarm(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            state.desired_mode = Some(MotionControllerMode::Alarm);
            state.notify_all();
        });

        Ok(())
    }

    pub async fn stop_motors(
        &self,
        names: &[String],
        disable: bool,
        alarm: bool,
        hit_time: Option<DeviceTime>
    ) -> Result<()> {

        // TODO: Make sure this never fails.
        let hit_time = match hit_time {
            Some(v) => {
                Some(self.shared.devices.time().all_times_at(v).await?)
            },
            None => None
        };

        lock!(state <= self.shared.state.lock().await?, {
            state.hit_time = hit_time;
            state.hit_position = None;
        });

        let mut batch = self.shared.devices.new_batch();

        for motor_name in names {
            let motor_i = self.shared.config.motors().iter().position(|m| m.device_name() == motor_name)
                .ok_or_else(|| format_err!("No motor named: {}", motor_name))?;
            
            let motor = &self.shared.motors[motor_i];
            
            // If the controller is currently in the enabled state, this will
            // have the effect of causing the background thread to notice that the MCU stepper queues
            // have been stopped on the next cycle and resync with that last moved to position.
            batch.add(motor.device_name(), motor.clear_stepper_queue_request()?);

            if disable {
                batch.add(motor.device_name(), motor.disable_request()?);
            }
        }

        batch.send().await?;

        if alarm {
            self.trigger_alarm().await?;
        }

        Ok(())
    }

    /// NOTE: The assumption is that the motion controller is idle and all queues are empty.
    pub async fn set_position(&self, position: VectorXd) -> Result<()> {
        let num_axes = self.shared.config.axes().len();

        if position.len() != num_axes {
            return Err(err_msg("Position has wrong number of dimensions."));
        }

        lock!(state <= self.shared.state.lock().await?, {
            self.check_accepting_movements(&state)?;

            state.position_offset = position;
            state.planner.set_start_position(VectorXd::zero_with_shape(num_axes, 1));

            for i in 0..state.motor_positions.len() {
                state.motor_position_offsets[i] += state.motor_positions[i];
                state.motor_positions[i] = 0;
            }

            Ok(())
        })
    }

    /// Returns the last position to which the motion controller will move to.
    pub async fn last_position(&self) -> Result<VectorXd> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.planner.last_position().clone() + &state.position_offset)
        })
    }

    pub async fn hit_position(&self) -> Result<Option<VectorXd>> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.hit_position.clone().map(|p| p + &state.position_offset))
        })
    }

    /// Blocks until all pending and in-flight motions have completed.
    /// (meaning that the hardware has sent all ticks)
    ///
    /// TODO: Also have this try to wait until the final step is physically done moving.
    ///
    /// NOTE: The idle state is also reached if the motors had to stop due to an
    /// endstop being triggered.
    pub async fn wait_until_idle(&self) -> Result<()> {
        loop {
            // TODO: The issue is that resyncing endstop positions is a race.
            let done = lock!(state <= self.shared.state.lock().await?, {
                self.check_accepting_movements(&state)?;

                Result::<_, Error>::Ok(state.planner.is_empty() && !state.active)
            })?;

            if done {
                break;
            }

            // TODO: Speed me up
            executor::sleep(Duration::from_millis(100)).await?;
        }

        Ok(())
    }

    pub fn num_axes(&self) -> usize {
        self.shared.config.axes().len()
    }
    
    fn check_accepting_movements(&self, state: &State) -> Result<()> {
        if self.will_reach_alarm_mode(state) {
            return Err(err_msg("In Alarm state"));
        }
    
        if state.mode != MotionControllerMode::Enabled {
            return Err(format_err!("Not accepting movements. Current state: {:?}", state.mode));
        }

        Ok(())
    }

    pub async fn move_to(&self, pos: VectorXd, feed_rate: f64) -> Result<()> {
        self.move_to_with_options(pos, &MoveOptions::default_for_feed_rate(feed_rate)).await
    }

    /// Schedules a movement to be performed in the future.
    ///
    /// Note that this blocks until the movement is schedules but the actual motion
    /// will happen later.
    pub async fn move_to_with_options(&self, pos: VectorXd, options: &MoveOptions) -> Result<()> {
        // TODO: Basically only allow this in the Enabled mode.

        // TODO: Quantize to step unit boundaries.

        // TODO: MAX_PLANNER_QUEUE_LENGTH

        lock!(state <= self.shared.state.lock().await?, {

            self.check_accepting_movements(&state)?;

            if state.planner.is_empty() {
                state.first_motion_time = Instant::now();
            }

            let next_pos = pos - &state.position_offset;

            state.planner.move_to_with_options(next_pos, options)?;

            Ok(())
        })
    }

    pub async fn set_max_junction_deviation(&self, value: f64) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            state.planner.set_max_junction_deviation(value);
        });
        Ok(())
    }
}


use cnc::linear_motion::LinearMotion;

#[derive(Clone)]
pub struct MoveOptions {
    pub feed_rate: f64,
    pub acceleration: Option<f64>,
    pub force: bool,
}

impl MoveOptions {
    pub fn default_for_feed_rate(feed_rate: f64) -> Self {
        Self {
            feed_rate,
            acceleration: None,
            force: false
        }
    }

    pub fn from_proto(proto: &MoveOptionsProto) -> Self {
        Self {
            feed_rate: proto.feed_rate(),
            acceleration: {
                if proto.has_acceleration() {
                    Some(proto.acceleration())
                } else {
                    None
                }
            },
            force: proto.force()
        }
    }

}


pub struct MotionControllerLinearPlanner {
    inner: LinearMotionPlanner,
    config: Arc<MotionControllerConfig>,
}

impl MotionControllerLinearPlanner {
    pub fn new(config: Arc<MotionControllerConfig>) -> Self {
        Self {
            inner: LinearMotionPlanner::new(
                VectorXd::zero_with_shape(config.axes().len(), 1),
                config.planner().clone()
            ),
            config
        }
    }

    pub fn start_time(&self) -> f64 {
        self.inner.start_time()
    }

    pub fn set_start_time(&mut self, v: f64) {
        self.inner.set_start_time(v);
    }

    pub fn next(
        &mut self,
        max_time: f64,
        out: &mut Vec<LinearMotion>
    ) {
        self.inner.next(max_time, out);
    }

    pub fn set_max_junction_deviation(&mut self, value: f64) {
        self.inner.set_max_junction_deviation(value);
    }

    pub fn last_position(&self) -> &VectorXd {
        self.inner.last_position()
    }

    pub fn start_position(&self) -> &VectorXd {
        self.inner.start_position()
    }

    pub fn set_start_position(&mut self, start_position: VectorXd) {
        self.inner.set_start_position(start_position);
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn move_to(&mut self, pos: VectorXd, feed_rate: f64) -> Result<()> {
        self.move_to_with_options(pos, &MoveOptions::default_for_feed_rate(feed_rate))
    }

    pub fn move_to_with_options(
        &mut self,
        pos: VectorXd,
        options: &MoveOptions
    ) -> Result<()> {
        if pos.len() != (self.config.axes().len() as usize) {
            return Err(err_msg("Wrong size tensor"));
        }

        if options.feed_rate < 1.0 {
            return Err(err_msg("Input feed rate too small"));
        }

        // TODO: Go into an error/alarm state if we have any failures like this.
        let last_pos = self.inner.last_position();

        let dir = &pos - last_pos;

        if dir.norm() < 0.001 {
            return Ok(());
        }


        // TODO: May want to base this on the orientation in X-Y
        let acceleration = {
            let mut accel_limits = vec![];
            for axis in self.config.axes() {
                if options.force && options.acceleration.is_some() {
                    accel_limits.push(100_000.0);
                } else {
                    accel_limits.push(axis.max_acceleration());
                }
            }

            let accel = options.acceleration.unwrap_or(100000.0);

            self.expand_coordinated_rate(accel, &dir, accel_limits)
        };

        let speed = {
            let mut speed_limits = vec![];
            for axis in self.config.axes() {
                if options.force {
                    speed_limits.push(100_000.0);
                } else {
                    speed_limits.push(axis.max_speed());
                }
            }

            self.expand_coordinated_rate(options.feed_rate, &dir, speed_limits)
        };

        // Currently lot's of the internal code doesn't work with zero speeds.
        if speed < 0.1 {
            return Err(format_err!("Resolved feedrate is very slow: {} mm/s", speed))
        }

        if acceleration < 0.1 {
            return Err(format_err!("Resolved acceleration is very small: {} mm/s^2", acceleration));
        }

        // TODO: Set a limit on the max feed rate based on configured machine limits.

        // TODO: Warn if we are adding to an empty one while active 

        self.inner.move_to(pos, speed, acceleration);

        Ok(())
    }

    fn expand_coordinated_rate(&self, rate: f64, dir: &VectorXd, mut speed_limits: Vec<f64>) -> f64 {

        // https://linuxcnc.org/docs/html/gcode/machining-center.html#sub:feed-rate

        // TODO: Unit test this.
        let mut coordination_priority = 10000;
        for i in 0..self.config.axes().len() {
            let axis = &self.config.axes()[i];
            let moving = dir[i].abs() > 0.0001;
            if moving && axis.coordination_priority() < coordination_priority {
                coordination_priority = axis.coordination_priority();
            }
        }

        let mut rate_masked_dir = dir.clone();
        for i in 0..self.config.axes().len() {
            let axis = &self.config.axes()[i];
            if axis.coordination_priority() != coordination_priority {
                rate_masked_dir[i] = 0.0;
            }
        }

        let rate_speed = (rate_masked_dir.normalized() * rate).abs();
        for i in 0..self.config.axes().len() {
            let axis = &self.config.axes()[i];

            if axis.coordination_priority() == coordination_priority {
                speed_limits[i] = speed_limits[i].min(rate_speed[i]);
            }
        }

        let v = constrained_vector(&dir, &speed_limits).norm();

        // assert!(v < 100.0, "{:?} : {:?} : {:?}", v, speed_limits, dir);

        v
    }
}



