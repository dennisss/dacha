use std::time::Instant;
use std::time::Duration;
use std::sync::Arc;
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::{lock, lock_async};
use executor::sync::AsyncMutex;
use cnc::linear_motion_planner::*;
use math::matrix::VectorXf;
use math::vecxf;
use peripherals_proto::peripherals::StepperMotorMotion_Direction;
use peripherals_service::device::PeripheralsDevice;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use cnc_controller_proto::cnc::*;
use executor::child_task::ChildTask;
use executor::sync::AsyncVariable;
use cnc::constrained_vector::constrained_vector;

use crate::devices::DevicesController;
use crate::tmc2209::TMC2209Device;
use crate::time::DeviceTime;
use crate::motion_controller::MotionController;


/// Monitors machine endstops and triggers the MotionController to stop when certain
/// ones are triggered.
///
/// - On creation, this is by default not monitoring any endstops.
/// - Endstops start getting monitored after a call to start().
/// - Once an endstop fires, it is latching meaning that it won't fire again.
///   - To make an endstop fire again, the owner of this controller must call start()
///     again.
pub struct EndstopController {
    // task: TaskResource,
    shared: Arc<Shared>,
}

// impl_resource_passthrough!(EndstopController, task);

struct Shared {
    config: EndstopControllerConfig,
    devices: Arc<DevicesController>,
    motion_controller: Arc<MotionController>,
    state: AsyncVariable<State>,

}

#[derive(Default)]
struct State {
    /// Tasks for polling all of the individual endstops.
    /// 
    /// TODO: Currently there is a cyclic dependency since the task threads hold an Arc<Shared>
    ///
    /// TODO; No need for this to be a HashMap.
    tasks: HashMap<String, ChildTask, FastHasherBuilder>,

    hit_expected: bool,
    hit_unexpected: bool,

    // TODO: Need to handle background thread failures.

    // /// If true, we are actively 
    // running: bool,

    // synced: bool,

    // /// Set of 
    // monitored_endstops: HashSet<String>,

    // expected_endstops: HashSet<String>,
}

struct EndstopState {
    hit: bool,
    failed: bool,
}

impl EndstopController {

    pub async fn create(
        config: EndstopControllerConfig,
        motion_controller: Arc<MotionController>,
        devices: Arc<DevicesController>
    ) -> Result<Self> {
        /*
        for endstop in config.endstops_mut() {
            endstop.clear_motor_indexes();
            for i in 0..endstop.motors().len() {
                let i = get_motor_index(&endstop.motors()[i])?;
                endstop.add_motor_indexes(i);
            }
        }
        */

        // TODO: Check all endstops have unique names and refer to valid motors.

        Ok(Self {
            shared: Arc::new(Shared {
                config,
                devices,
                motion_controller,
                state: AsyncVariable::default(),
            })

        })
    }

    /// NOT CANCEL SAFE
    ///
    /// TODO: Make this cancel safe
    pub async fn start(
        &self,
        monitored_endstops: &[String],
        expected_endstops: &[String]
    ) -> Result<()> {
        let monitored_endstops = monitored_endstops.iter().cloned().collect::<HashSet<String>>();
        let expected_endstops = expected_endstops.iter().cloned().collect::<HashSet<String>>();

        lock_async!(state <= self.shared.state.lock().await?, {
            for (_, task) in state.tasks.drain() {
                task.cancel().await;
            }

            // TODO: Reset internal triggered state.

            for endstop_name in monitored_endstops {
                // TODO: Check that it has a config.

                state.tasks.insert(endstop_name.clone(), ChildTask::spawn(
                    Self::endstop_watcher_thread(
                        self.shared.clone(),
                        endstop_name.clone(),
                        expected_endstops.contains(&endstop_name)
                    )));
            }

            // TODO: Ideally wait for all endstops to register their initial state.

            Ok(())
        })
    }

    pub async fn check_hit_something(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if state.hit_unexpected {
                return Err(err_msg("Unexpected endstop hit!"));
            }

            if !state.hit_expected {
                return Err(err_msg("No endstops hit"));
            }

            Ok(())
        })
    }

    async fn endstop_watcher_thread(
        shared: Arc<Shared>,
        endstop_name: String,
        expected: bool
    ) {
        let res = Self::endstop_watcher_thread_inner(&shared, endstop_name.clone(), expected).await;

        if let Err(e) = res {
            eprintln!("Endstop '{}' failed: {}", endstop_name, e);

            // TODO: This may also fail.
            let _ = shared.motion_controller.trigger_alarm().await;
        }

    }

    async fn endstop_watcher_thread_inner(
        shared: &Shared,
        endstop_name: String,
        expected: bool
    ) -> Result<()> {

        let endstop_config = shared.config.endstops().iter().find(|e| e.name() == endstop_name)
            .ok_or_else(|| err_msg("No config for endstop"))?;

        let dev = shared.devices.get_peripherals_device(endstop_config.peripheral().device_name()).await?;

        let mut hit_time = None;

        if !endstop_config.analog_buffers().is_empty() {

            let mut next_buffer_index = 0;
            let mut enqueued_requests = vec![];
            
            loop {
                while enqueued_requests.len() < endstop_config.analog_buffers().len() {
                    enqueued_requests.push(dev.enqueue_analog_read_window(
                        endstop_config.peripheral().peripheral_name(),
                        &endstop_config.analog_buffers()[next_buffer_index]
                    ).await?);

                    next_buffer_index = (next_buffer_index + 1) % endstop_config.analog_buffers().len();
                }

                let req = enqueued_requests.remove(0);
                let res = req.await?;

                if res.triggered {
                    // TODO: Need to make sure this is using a precise time.
                    let t = shared.devices.time().wrap_raw_time(
                        endstop_config.peripheral().device_name(),
                        res.sampling_completion_time
                    ).await?;
                    hit_time = Some(t);

                    break;
                }
            }

        } else {
            // TODO: I sometimes get RESOURCE_BUSY for this.
            dev.poll_gpio_interrupt(endstop_config.peripheral().peripheral_name()).await?;
        }

        lock!(state <= shared.state.lock().await?, {
            if expected {
                state.hit_expected = true;
            } else {
                state.hit_unexpected = true;
            }
        });

        // Mark the hit.
        // TODO:

        // TODO: Need to implement some reasonable behavior if two endstops trigger that touch the same motors (e.g. A and B diag pins)

        let t1 = Instant::now();

        // Stop motors.
        let disable_motors = endstop_config.hard();
        let alarm = !expected;
        shared.motion_controller.stop_motors(endstop_config.motors(), disable_motors, alarm, hit_time).await?;

        let t2 = Instant::now();

        println!("Endstop hit: {} ; {:?}", endstop_config.name(), t2 - t1);

        Ok(())
    }

}