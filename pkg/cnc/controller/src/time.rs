use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::lock;
use executor::sync::AsyncRwLock;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use peripherals_service::device::PeripheralsDevice;

const MAX_MEASUREMENT_AGE: Duration = Duration::from_secs(10);

const MAX_RTT: Duration = Duration::from_millis(5);

const POLL_PERIOD: Duration = Duration::from_secs(1);

const MCU_CLOCK_FREQUENCY: usize = 16_000_000;

/*
For nRF52840, expectation is that we have a ±30	ppm 32Mhz crystal to meet the bluetoth specs.
*/

/// Component which maintains an offset between the local machine's monotonic
/// clock and each MCU device's clock.
///
/// TODO: Improve the time syncronization by estimating the offset + clock
/// frequency multiplier of each external MCU. Ideally something like a Kalman
/// filter.
///
/// TODO: I would also want to scale the time values by the clock frequency.
///
/// Uncertainty factors:
/// - RTT of measurements
/// - Temperature of MCU crystal (will change the frequency)
/// - Recency of measurements (older measurements will accumulate more error
///   in estimating the other two factors).
pub struct TimeSyncer {
    task: TaskResource,

    shared: Arc<Shared>,

    // HashMap<mcu_name, { my_time, their_time }>
}

impl_resource_passthrough!(TimeSyncer, task);

#[derive(Default)]
struct Shared {
    state: AsyncRwLock<State>
}

#[derive(Default)]
struct State {
    devices: HashMap<String, DeviceEntry, FastHasherBuilder>,
}

struct DeviceEntry {
    device: Arc<PeripheralsDevice>,
    last_sample: Option<TimeSample>
}

struct TimeSample {
    local_time: Instant,
    mcu_time: u32
}


impl TimeSyncer {
    pub fn create() -> Self {
        let shared = Arc::new(Shared::default());
        let task = TaskResource::spawn_interruptable("TimeSyncer", Self::background_thread(shared.clone()));

        Self {
            task,
            shared
        }
    }

    pub async fn add_device(&self, name: &str, device: Arc<PeripheralsDevice>) -> Result<()> {
        lock!(state <= self.shared.state.write().await?, {

            let entry = DeviceEntry {
                device,
                last_sample: None
            };

            if state.devices.insert(name.to_string(), entry).is_some() {
                return Err(format_err!("Duplicate device named: {}", name));
            }

            Ok(())
        })
    }

    async fn background_thread(shared: Arc<Shared>) -> Result<()> {
        loop {
            let mut new_samples = vec![];

            {
                let state = shared.state.read().await?;

                for (device_name, device) in &state.devices {
                    // TODO: Need a timeout on this.
                    match Self::get_time_sample(&device.device).await {
                        Ok(sample) => {
                            new_samples.push((device_name.to_string(), sample));
                        }
                        Err(e) => {
                            eprintln!("Time sync with {} failed: {}", device_name, e);
                        }
                    }
                }
            }

            lock!(state <= shared.state.write().await?, {
                for (name, sample) in new_samples {
                    let entry = state.devices.get_mut(&name).unwrap();
                    if entry.last_sample.is_none() {
                        eprintln!("Time synced for {}", name);
                    }
                    
                    entry.last_sample = Some(sample);
                }
            });

            // TODO: Subtract elapsed time.
            executor::sleep(POLL_PERIOD).await?;
        }
    }

    async fn get_time_sample(device: &PeripheralsDevice) -> Result<TimeSample> {
        let start_time = Instant::now();

        let mcu_time = device.get_clock_time().await?;

        let end_time = Instant::now();

        let rtt = end_time - start_time;

        let local_time = start_time + (rtt / 2);

        if rtt > MAX_RTT {
            return Err(format_err!("RTT too large: {:?}", rtt));
        }

        Ok(TimeSample {
            local_time,
            mcu_time
        })
    }

    // TODO: Do not allow very far into the future estimates (or far into the past).
    pub async fn to_device_time(&self, device_name: &str, time: Instant) -> Result<u32> {
        let state = self.shared.state.read().await?;

        let dev = state.devices.get(device_name)
            .ok_or_else(|| format_err!("No device named: {}", device_name))?;

        let sample = dev.last_sample.as_ref()
            .ok_or_else(|| err_msg("Device is not time synced yet"))?;

        let delta: Duration;
        let sign: i32;

        if time >= sample.local_time {
            delta = time - sample.local_time;
            sign = 1;
        } else {
            delta = sample.local_time - time;
            sign = -1;
        }

        if delta > MAX_MEASUREMENT_AGE {
            return Err(err_msg("Time too far in the future or device sync too stale"));
        }

        // TODO: Use integer math?
        let mut delta = (delta.as_secs_f64() * (MCU_CLOCK_FREQUENCY as f64)).round() as i32;
        delta *= sign;

        let out = sample.mcu_time.wrapping_add(delta as u32);

        Ok(out)
    }
}

