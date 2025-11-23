use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::lock;
use executor::sync::AsyncMutex;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use peripherals_service::device::PeripheralsDevice;
use cnc_controller_proto::cnc::HeaterControllerConfig;

use crate::devices::*;
use crate::pid::*;

const STABILIZE_PERIOD: Duration = Duration::from_secs(5);

const STABLE_MAX_ERROR: f32 = 10.0;

pub struct HeaterController {
    task: TaskResource,

    shared: Arc<Shared>,
}

impl_resource_passthrough!(HeaterController, task);

struct Shared {
    config: HeaterControllerConfig,
    devices: Arc<DevicesController>,
    state: AsyncMutex<State>
}

#[derive(Default)]
struct State {
    active: Option<ActiveState>
}

struct ActiveState {
    target_temp: f32,
    pid: PIDController,
    first_met: Option<Instant>,
    stable: bool,
}

impl HeaterController {
    pub fn create(devices: Arc<DevicesController>, config: HeaterControllerConfig) -> Self {
        let shared = Arc::new(Shared {
            config,
            devices,
            state: AsyncMutex::default()
        });

        // TODO: Add a heater name to this.
        let task = TaskResource::spawn_interruptable("HeaterController", Self::background_thread(shared.clone()));

        Self {
            task,
            shared
        }
    }

    pub async fn set_target_temperature(&self, target_temp: f32) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            // TODO: If already nearly correct, just update the target temp?

            // TODO: Eventually need active cooling.
            if target_temp <= 30.0 {
                state.active = None;
            }

            state.active = Some(ActiveState {
                target_temp,
                pid: PIDController::new(),
                first_met: None,
                stable: false
            });
        });

        Ok(())
    }

    pub async fn wait_until_stable(&self) -> Result<()> {
        loop {
            let stable = lock!(state <= self.shared.state.lock().await?, {
                let active_state = state.active.as_ref()
                    .ok_or_else(|| err_msg("No target temperature"))?;
                Result::<_, Error>::Ok(active_state.stable)
            })?;

            if stable {
                return Ok(());
            }

            executor::sleep(Duration::from_millis(1000)).await?;
        }
    }

    async fn background_thread(shared: Arc<Shared>) -> Result<()> {
        let dev = shared.devices.get_peripherals_device("toolhead").await?;
        
        dev.pwm_write("fan_mid_pwm", 1.0).await?;

        // TODO: Need min and max temp protections.
        // TODO: Always poll temps and not just when active.

        loop {
            let active = lock!(state <= shared.state.lock().await?, {
                state.active.is_some()
            });

            let mut heater_target = 0.0;
            if active {
                // TODO: Maybe have a low pass filter on this?
                let current_temp = dev.analog_read("thermistor_sense").await?;
                let sample_time = Instant::now();

                lock!(state <= shared.state.lock().await?, {
                    if let Some(active_state) = &mut state.active {
                        println!("[Temp: {:.2} / {:.2}]", current_temp, active_state.target_temp);

                        let error = active_state.target_temp - current_temp;

                        heater_target = active_state.pid.next(error, sample_time);

                        if error.abs() < STABLE_MAX_ERROR {
                            let first_met = active_state.first_met.get_or_insert(sample_time).clone();
                            if sample_time - first_met > STABILIZE_PERIOD {
                                active_state.stable = true;
                            }
                        } else {
                            active_state.first_met = None;
                            active_state.stable = false;
                        }
                    }
                });
            }

            dev.pwm_write("heater_pwm", heater_target).await?;

            // TODO: Subtract elapsed time.
            executor::sleep(Duration::from_millis(500)).await?;
        }
    }

}