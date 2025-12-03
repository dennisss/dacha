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
    mcu_time: u64
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
                    match Self::get_time_sample(device_name, &device.device).await {
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
                for (name, mut sample) in new_samples {
                    let entry = state.devices.get_mut(&name).unwrap();
                    
                    match &entry.last_sample {
                        Some(last_sample) => {
                            let mut cycle = last_sample.mcu_time >> 32;

                            let lower_mask = 0xFFFFFFFF;

                            if (last_sample.mcu_time & lower_mask) > sample.mcu_time {
                                // Assumption is that we get at least one new sample each time before the clock rolls over
                                println!("Roll over...");
                                cycle += 1;
                            }

                            sample.mcu_time |= cycle << 32;

                        }
                        None => {
                            eprintln!("Time synced for {}", name);
                        }
                    }
                    
                    entry.last_sample = Some(sample);
                }
            });

            // TODO: Subtract elapsed time.
            executor::sleep(POLL_PERIOD).await?;
        }
    }

    // NOTE: This will return a mcu_time sample with only 32bits of informaiton.
    async fn get_time_sample(device_name: &str, device: &PeripheralsDevice) -> Result<TimeSample> {
        let mcu_time = device.get_clock_time().await?;

        let rtt = mcu_time.local_response_time - mcu_time.local_request_time;

        let local_time = mcu_time.local_response_time + (rtt / 2);

        // TODO: Even if the RTT is too high, we can still use it to update the high bits of the clock.
        if rtt > MAX_RTT {
            return Err(format_err!("RTT too large: {:?}", rtt));
        }

        println!("[{} rtt] {:?}", device_name, rtt);

        Ok(TimeSample {
            local_time,
            mcu_time: mcu_time.remote_time as u64
        })
    }

    /// Blocks untill all MCUs have a synced time.
    pub async fn wait_for_sync(&self) -> Result<()> {
        // TODO: Also check that the time isn't too old.
        
        loop {
            {
                let state = self.shared.state.read().await?;

                let mut all_good = true;

                for dev in state.devices.values() {
                    if dev.last_sample.is_none() {
                        all_good = false;
                        break;
                    }
                }

                if all_good {
                    break;
                }
            }

            executor::sleep(Duration::from_millis(100)).await?;
        }

        Ok(())
    }

    // TODO: Do not allow very far into the future estimates (or far into the past).
    pub async fn to_device_time(&self, device_name: &str, time: Instant) -> Result<DeviceTime> {
        let state = self.shared.state.read().await?;

        let dev = state.devices.get(device_name)
            .ok_or_else(|| format_err!("No device named: {}", device_name))?;

        self.make_device_time(dev, time)
    }

    pub async fn to_all_device_times(&self, time: Instant) -> Result<HashMap<String, DeviceTime, FastHasherBuilder>> {
        let state = self.shared.state.read().await?;
        let mut out = HashMap::default();

        for (dev_name, dev) in &state.devices {
            out.insert(dev_name.clone(), self.make_device_time(dev, time)?);
        }

        Ok(out)
    }

    fn make_device_time(&self, dev: &DeviceEntry, time: Instant) -> Result<DeviceTime> {
        let sample = dev.last_sample.as_ref()
            .ok_or_else(|| err_msg("Device is not time synced yet"))?;

        let delta: Duration;
        let sign: i64;

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

        // TODO: Handle times that are before the start of the MCU clock?

        // TODO: Use integer math?
        let mut delta = (delta.as_secs_f64() * (MCU_CLOCK_FREQUENCY as f64)).round() as i64;
        delta *= sign;

        let out = (sample.mcu_time as i64) + delta;
        assert!(out >= 0);

        Ok(DeviceTime {
            value: out as u64
        })
    }

}

// TODO: Prevent comparisons across different devices.
#[derive(PartialOrd, PartialEq, Debug, Clone, Copy, Eq, Ord)]
#[repr(transparent)]
pub struct DeviceTime {
    value: u64,
}

impl DeviceTime {
    // #[cfg(test)]
    pub fn new_test_only(v: u64) -> Self {
        Self { value: v }
    }

    pub fn lower(&self) -> u32 {
        self.value as u32
    }

    pub fn add_ticks(self, ticks: u32) -> Self {
        Self { value: self.value + (ticks as u64) }
    }

    pub fn add_duration(self, dur: Duration) -> Self {
        let mut delta = (dur.as_secs_f64() * (MCU_CLOCK_FREQUENCY as f64)).round() as u64;

        // let delta = (1000000000u64 * (dur.as_nanos() as u64)) / 16_000_000;
        Self { value: self.value + delta }
    }
}



