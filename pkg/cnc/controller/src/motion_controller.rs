use std::time::Instant;
use std::time::Duration;
use std::sync::Arc;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::lock;
use executor::sync::AsyncMutex;
use cnc::linear_motion_planner::*;
use math::matrix::VectorXf;
use math::vecxf;
use peripherals_proto::peripherals::StepperMotorMotion_Direction;
use peripherals_service::device::PeripheralsDevice;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use cnc_controller_proto::cnc::*;

use crate::devices::DevicesController;
use crate::tmc2209::TMC2209Device;
use crate::stepper_motion_generator::*;
use crate::time::DeviceTime;

/*
Background thread that owns the motors and endstops

Input is a list of move commands.

- Once we accumulate up to X seconds of fully defined motions or we have timed otu
- Mark motion start time as 

- Need some minimum time between a motion being added and use being allowed to start it (since we need time to enqueue it)


TODO: Should also have a concept of max time to the next step.

TODO: Will need to do some coordination with other components like the fan controller.
- Once a motion is fully constrained, this needs to phone back the timing 


Doing Homing:
- We will check all diag pins each time we check the motor statuses
- If we see one, we will stop all motors (clear_motion and disable and re-enable the motors)
    - Then report to the 

- We also have a concept of sometimes on endpoints (for Z)
    - These is a long polling command so run in a separate thread
    - Notify the main thread when it happens.

- 


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
const MIN_MOTION_START_DELAY: Duration = Duration::from_millis(200);

/// Relative to the current point in time, how many seconds of motions in the
/// LinearMotionPlanner we will consume when in the 'Active' state.
///
/// MUST BE >> MIN_MOTION_START_DELAY
const PLANNER_LOOK_AHEAD_WINDOW: Duration = Duration::from_millis(2000);

/// Minimum amount of time we will grab from the planner.
///
/// MUST BE < PLANNER_LOOK_AHEAD_WINDOW
///
/// Basically if we initially consume 'PLANNER_LOOK_AHEAD_WINDOW' we won't
/// consume more motions until 'PLANNER_MIN_TIME_STEP' more time has elapsed.
/// This sets a limit on the minimum chunk duration used for splitting up
/// very long motions. 
const PLANNER_MIN_TIME_STEP: Duration = Duration::from_millis(200);

/// Relative to the current point in time, how many seconds of motions
/// will be converted to steps and enqueud to run on the motors.
///
/// MUST BE < PLANNER_LOOK_AHEAD_WINDOW since the step generator is fed
/// motions from the planner. Otherwise it will plan for empty time. 
///
/// MUST BE >> MIN_MOTION_START_DELAY
const STEP_GENERATION_WINDOW: Duration = Duration::from_millis(400);


const POLL_INTERVAL: Duration = Duration::from_millis(100);


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

    state: AsyncMutex<State>
}

struct State {
    /// If planner is non-empty and we are idle, the time at which
    /// the first motion in planner was added. 
    first_motion_time: Instant,

    planner: MotionControllerLinearPlanner,

    motors_on: bool,

    active_state: Option<ActiveState>,

    /// TODO: Instead of storing this, just always store a StepperMotionGenerator instance
    /// and rely on that for this data.
    motor_positions: Vec<i64>,

    // /// If true,
    // homing_mode: bool,
}

struct ActiveState {

    start_time: Instant,

    /// Start time of the first motion in the planner.
    planner_start_time: Instant,

    /// Up to what point in time in the queue we have consumed commands.
    queue_consumed_time: Instant,

    queue: StepperMotionGenerator,
}

#[derive(Default)]
struct MotorState {
    /// Position of the motor in units of steps.
    ///
    /// When all motors are at position 0, the expectation is that we are
    /// also at the 'XYZ..' zero position.
    position: i64,

    // last_motion_end_time: Option<u32>,
}

/*
The next challenge:
- For some motors, some motions may be doing nothing so I don't really know the right time for them.

- For every step, there is a time at which it starts moving

*/

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

        for endstop in config.endstops_mut() {
            endstop.clear_motor_indexes();
            for i in 0..endstop.motors().len() {
                let i = get_motor_index(&endstop.motors()[i])?;
                endstop.add_motor_indexes(i);
            }
        }

        Ok(())
    }

    pub async fn create(mut config: MotionControllerConfig, devices: Arc<DevicesController>) -> Result<Self> {
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
            state: AsyncMutex::new(State {
                first_motion_time: Instant::now(), // doesn't matter
                planner: MotionControllerLinearPlanner::new(config.clone()),
                motors_on: false,
                active_state: None,
                motor_positions: vec![0; config.motors().len()]
            })
        });

        let task = TaskResource::spawn_interruptable("MotionController", Self::background_thread(shared.clone()));

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


    async fn background_thread(shared: Arc<Shared>) -> Result<()> {
        // TODO: Most important thing is to make sure we don't lose track of steps between the floating
        // point and step dimensions.

        // TODO: Need guarantees that we are only giving out monotonic timestamps to each motor.

        // // Steps we have prepared but we can't yet enqueue since the motor has no space.
        // let mut pending_steps = VecDequeue::default();
        // let mut pending_steps_duration = 0.0;

        let mut last_faults = 0;

        // let mut active_motions = vec![];

        // For each motor, these are motions that haven't yet been sent to the MCU.
        let mut pending_step_motions = vec![];
        for _ in shared.config.motors() {
            pending_step_motions.push(VecDeque::new());
        }
        let mut have_pending_motions = false;

        let mut last_directions = vec![StepperMotorMotion_Direction::UNCHANGED; shared.config.motors().len()];

        // TODO: Whenever in homing mode, it would be good to explicitly decrease all the motor currents a lot.

        /*
        If I enqueue a new motion, I want to guarantee that it is never before a previous motion that has completed.
        */

        let mut last_estop_check = Instant::now();

        loop {
            // Check all motors states (mainly need to know if limits are tripped and if there are queue spots available).

            let cycle_start = Instant::now();

            let mut statuses = vec![];
            let mut some_motors_active = false;
            let mut stopping = false;

            let status_responses = {
                let mut reqs = vec![];
                for (motor_i, motor) in shared.config.motors().iter().enumerate() {
                    let dev = &shared.motors[motor_i];
                    reqs.push(dev.get_stepper_motor_status_request()?);
                }

                shared.motors[0].device().send_request_batch(&reqs).await?
            };

            let mut new_faults = 0;
            for (motor_i, res) in status_responses.into_iter().enumerate() {
                let dev = &shared.motors[motor_i];
                let s = res.stepper_status(); // dev.get_stepper_motor_status().await?;
                
                new_faults += s.faults();

                // TODO: If the status is idle but we have recently enqueued stuff, then that is bad
                // since there is a gap in time.

                // TODO: Check if any motors skipped motions.

                some_motors_active |= s.active();

                statuses.push(s.clone());

                // println!("SG_RESULT: {:?}", dev.sg_result().await?);
                // println!("TSTEP: {:?}", dev.tstep().await?);
            }

            /*
            // TODO: Main issue with this logic being here is that the poll rate is fairly low.
            for endstop in shared.config.endstops() {
                let now = Instant::now();
                
                // println!("ESTOP Check Interval: {:?}", now - last_estop_check);

                last_estop_check = now;


                let dev = shared.devices.get_peripherals_device(endstop.device_name()).await?;

                let active = dev.gpio_read(endstop.peripheral_name()).await?;
                // println!("STOP: {}", active);
                
                if active {
                    let stop_time = Instant::now();


                    println!("ESTOP");

                    // TODO: Avoid doing the same thing for the same motor if both motors configure an overlapping diag stall protection.

                    for motor_i in endstop.motor_indexes().iter().cloned() {
                        // println!("SG_RESULT: {:?}", shared.motors[motor_i as usize].sg_result().await?);
                        // println!("TSTEP: {:?}", shared.motors[motor_i as usize].tstep().await?);

                        shared.motors[motor_i as usize].disable().await?;
                    }
                    for motor_i in endstop.motor_indexes().iter().cloned() {
                        // TODO: Clear all motors?
                        pending_step_motions[motor_i as usize].clear();

                        shared.motors[motor_i as usize].clear_stepper_queue().await?;
                    }

                    stopping = true;
                    

                    /*
                    Stop all motors (disable, clear queue, clear local queue)

                    TODO: If active moves didn't involve the motors marked in the endstop, kill everything (e.g. crashing into an uneven bed when doing a X/Y move)

                    Estimate the current position based on the time in the sent moves.
                    */

                    
                }

            }
            */

            if new_faults != last_faults {
                eprintln!("FAULTS: Motions skipped : {}", new_faults);
                last_faults = new_faults;
            }

            let now = Instant::now();

            // TODO: need to support motors on different MCUs
            // TODO: Still no guarantee that this will give a monotonic time.
            // TODO: Dedup with the other now below.
            let now_mcu_time_full: DeviceTime = shared.devices.time().to_device_time(
                shared.motors[0].device_name(),
                now
            ).await?;
            
            let now_mcu_time = now_mcu_time_full.lower();

            lock!(state <= shared.state.lock().await?, {
                let state: &mut State = &mut state;

                if stopping {
                    // TODO: Also clear the queue. Will need to grab the latest position though.
                    state.planner.clear();
                }

                // TODO: Currently there is not much value to having the 'active_state'

                if state.active_state.is_none() {
                    if !state.planner.is_empty() && state.first_motion_time + IDLE_START_TIMEOUT >= now {
                        println!("-> Active");

                        let start_time = now + MIN_MOTION_START_DELAY;
                        let remote_start_time = now_mcu_time_full.add_duration(MIN_MOTION_START_DELAY);

                        // TODO: Need to preserve motor positions across queue initializations.
                        state.active_state = Some(ActiveState {
                            start_time,
                            planner_start_time: start_time,
                            queue_consumed_time: start_time,
                            queue: StepperMotionGenerator::new(
                                shared.config.clone(),
                                &state.motor_positions,
                                remote_start_time
                            )
                        });

                    }
                }

                if let Some(active_state) = &mut state.active_state {
                    // TODO: Also check that the queue of commands to send is empty?
                    if state.planner.is_empty() && active_state.queue.is_empty() {
                        if !some_motors_active {
                            println!("=> Idle");

                            state.motor_positions.copy_from_slice(active_state.queue.motor_positions());

                            state.active_state = None;
                            // TODO: Have a 'continue' here?
                        }
                        
                    } else if !have_pending_motions {
                        
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
                            let next_planner_start_time = now + PLANNER_LOOK_AHEAD_WINDOW;

                            let time_step = next_planner_start_time - active_state.planner_start_time;

                            if time_step > PLANNER_MIN_TIME_STEP {
                                // TODO: Check these comments.
                                // NOTE: We only use the end_position and time in each of these objects.
                                let mut motions = vec![];
                                // TODO: Need to use the duration returned by this (yes since we don't know if we fully saturated the time span)
                                state.planner.next(time_step.as_secs_f32(), 10000, &mut motions);

                                for motion in motions {
                                    // TODO: Should warn if the motion was delayed relative to the previous one.
                                    active_state.queue.enqueue(motion);
                                }

                                active_state.planner_start_time = next_planner_start_time;
                            }
                        }

                        // Take from the 'queue' and prepare to send to the machine.
                        {
                            // TODO: Limit the minimum time step for this?

                            let next_queue_consumed_time = now + STEP_GENERATION_WINDOW;
                            let queue_max_time = next_queue_consumed_time - active_state.start_time;

                            let step_motions = active_state.queue.to_commands(queue_max_time.as_secs_f64()).unwrap();
                            active_state.queue_consumed_time = next_queue_consumed_time;

                            for (i, step_motion) in step_motions.into_iter().enumerate() {
                                for step_motion in step_motion {
                                    // println!("Enqueue: {:?}", step_motion);

                                    pending_step_motions[i].push_back(step_motion);
                                    have_pending_motions = true;
                                }
                            }
                        }

                        // TODO: Maybe propagate back the latest machine position to the planner as the new planner
                        // start_position to account for drift due to step quantization.

                    }
                    
                }



            });

            // Do enqueues.

            // TODO: Ideally prioritize enqueues based on start time (k-way merge).
            let s = Instant::now();

            // TODO: Complain if we are ever not able to maintain at least Xms of motions across all motors enqueued
            // on the MCU.

            have_pending_motions = false;
            let mut sent_something = 0;
            let mut requests = vec![];

            for (motor_i, queue) in pending_step_motions.iter_mut().enumerate() {
                let mut empty_slots = statuses[motor_i].empty_queue_slots() as usize;
                
                loop {
                    if empty_slots == 0 {
                        break;
                    }

                    let mut motion = match queue.pop_front() {
                        Some(v) => v,
                        None => break
                    };

                    if motion.direction() == last_directions[motor_i] {
                        motion.clear_direction();
                    } else {
                        last_directions[motor_i] = motion.direction();
                    }

                    // TODO: These should be easy to sequence compress and acks should also be compressable.
                    requests.push(shared.motors[motor_i].make_enqueue_stepper_motion(motion)?);

                    empty_slots -= 1;
                }


                have_pending_motions |= !queue.is_empty();
            }

            // Assumption here is that the stepper queue length is smaller than the max in flight requests we allow.
            // TODO: Need to cap this since we are dealing with big queues now (and prioritize by start time).
            if requests.len() > 0 {
                for reqs in requests.chunks(64) {
                    shared.motors[0].device().send_request_batch(reqs).await?;
                }

                sent_something += requests.len();
            }

            let e = Instant::now();

            let cycle_end = Instant::now();

            if sent_something > 0 {
                println!("Cycle: {:?} ; Enqueue {} in time: {:?}", cycle_end - cycle_start, sent_something, e - s);
            }

            if have_pending_motions {
                println!("Remote queue full!");
            }


            /*
            if idle
                Wait for moves to be available and timed out
                    Enter active state with a start time.

                - Check initial diag state to verify everything is ok.

            NOTE: Can't go into active mode without enabling the motors 

            if active
                - check for stale signals
                    - Ideally this is at a very high poll rate.
                    - On stale, we need to disable the driver and check the reason (e.g. could be overheating)

                - check MCU state to see how much stuff we can enqueue
                - enqueue everything we can
                - 

                - check if we are done (all enqueued and MCU says !active)
                    - is so, go back to the idle state.


                - We will have a callback to the high level controller


            */

            let cycle_time = cycle_end - cycle_start;
            if cycle_time < POLL_INTERVAL {
                executor::sleep(POLL_INTERVAL - cycle_time).await?;
            }
        }

    }

    pub async fn toggle_motors(&self, on: bool) -> Result<()> {
        for motor in &self.shared.motors {
            if on {
                motor.enable().await?;
            } else {
                motor.disable().await?;
            }
        }

        Ok(())
    }

    /// NOTE: The assumption is that the motion controller is idle and all queues are empty.
    pub async fn set_position(&self, position: VectorXf) -> Result<()> {
        let num_axes = self.shared.config.axes().len();

        lock!(state <= self.shared.state.lock().await?, {
            state.position_offset = position;
            state.planner.set_start_position(VectorXf::zero_with_shape(num_axes, 1));
            for p in &mut state.motor_positions {
                *p = 0;
            }
        });

        Ok(())
    }

    /// Returns the last position to which the motion controller will move to.
    pub async fn last_position(&self) -> Result<VectorXf> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.planner.last_position().clone())
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
            let done = lock!(state <= self.shared.state.lock().await?, {
                state.planner.is_empty() && state.active_state.is_none()
            });

            if done {
                break;
            }

            executor::sleep(Duration::from_millis(100)).await?;
        }

        Ok(())
    }

    pub fn num_axes(&self) -> usize {
        self.shared.config.axes().len()
    }

    /// Schedules a movement to be performed in the future.
    ///
    /// Note that this blocks until the movement is schedules but the actual motion
    /// will happen later.
    pub async fn move_to(&self, pos: VectorXf, feed_rate: f32) -> Result<()> {
        
        // TODO: Quantize to step unit boundaries.

        // TODO: MAX_PLANNER_QUEUE_LENGTH

        lock!(state <= self.shared.state.lock().await?, {

            if state.planner.is_empty() {
                state.first_motion_time = Instant::now();
            }

let next_pos = pos - &state.position_offset;

            state.planner.move_to(next_pos, feed_rate)?;

            Ok(())
        })
    }
}

use cnc::linear_motion::LinearMotion;

pub struct MotionControllerLinearPlanner {
    inner: LinearMotionPlanner,
    config: Arc<MotionControllerConfig>,
}

impl MotionControllerLinearPlanner {
    pub fn new(config: Arc<MotionControllerConfig>) -> Self {
        Self {
            inner: LinearMotionPlanner::new(
VectorXf::zero_with_shape(config.axes().len(), 1),
config.planner().clone()
),
            config
        }
    }

    pub fn next(
        &mut self,
        max_duration: f32,
        max_count: usize,
        out: &mut Vec<LinearMotion>
    ) {
        self.inner.next(max_duration, max_count, out);
    }

    pub fn last_position(&self) -> &VectorXf {
        self.inner.last_position()
    }

    pub fn set_start_position(&mut self, start_position: VectorXf) {
        self.inner.set_start_position(start_position);
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn move_to(&mut self, pos: Vector3f, feed_rate: f32) -> Result<()> {
        // TODO: Go into an error/alarm state if we have any failures like this.
        let last_pos = self.inner.last_position();

        let x_move = (last_pos.x() - pos.x()).abs() >= 0.001;
        let y_move = (last_pos.y() - pos.y()).abs() >= 0.001;
        let z_move = (last_pos.z() - pos.z()).abs() >= 0.001;

        // Mixing these doesn't work right now as the corning algorithm doesn't
        // work well with 
        // TODO: I will need this after leveling.
        if (x_move || y_move) && z_move {
            return Err(err_msg("Simulatenous X/Y and Z move not supported"));
        }

        // TODO: May want to base this on the orientation in X-Y
        let acceleration = {
            if z_move {
                self.config.max_acceleration_z()
            } else {
                self.config.max_acceleration_xy()
            }
        };

        // TODO: Set a limit on the max feed rate based on configured machine limits.

        self.inner.move_to(pos, feed_rate, acceleration);

        Ok(())
    } 
}




