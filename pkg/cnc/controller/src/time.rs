use std::time::{Instant, Duration};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::lock;
use executor::bundle::TaskBundle;
use executor::sync::{AsyncRwLock, AsyncMutex};
use executor_multitask::{TaskResource, impl_resource_passthrough};
use peripherals_service::device::PeripheralsDevice;
use cnc_controller_proto::cnc::TimeControllerConfig;

use crate::stats::MinMaxStats;
use crate::time_relation::*;


const MAX_MEASUREMENT_AGE: Duration = Duration::from_secs(10);

/// The maximum RTT allowed for a sample between the host and one of the MCUs
/// when computing the skew/offset between the host and MCU clocks.
///
/// (not relevant when computing MCU to MCU SOF timings.) 
const MAX_HOST_SYNC_RTT: Duration = Duration::from_micros(500);

const POLL_PERIOD: Duration = Duration::from_millis(100);

const MCU_CLOCK_FREQUENCY: usize = 16_000_000;

const MAX_HISTORY_SIZE: usize = 10;

const LOG_INTERVAL: usize = 10;

const HOST_DEVICE_ID: u64 = 0;

// TODO: Ideally start using CLOCK_MONOTONIC_RAW


/*
For nRF52840, expectation is that we have a ±30	ppm 32Mhz crystal to meet the bluetoth specs.

TODO: Switch to CLOCK_MONOTONIC_RAW for the local clock

TODO: Support getting a boot time measurement from an MCU

*/

// TODO: Have a virtual USB StartOfFrame clock (based on the frame counter) so that we can compare clocks.


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


struct Shared {
    config: TimeControllerConfig,
    state: AsyncRwLock<State>,
    start_host_time: Instant,
}

#[derive(Default)]
struct State {
    /// Device id of the last synced time device that was registered.
    last_device_id: u64,

    /// Map from device name to device id.
    device_ids: HashMap<String, u64, FastHasherBuilder>,

    devices: HashMap<u64, DeviceEntry, FastHasherBuilder>,

    /// Between two 
    relations: HashMap<(u64, u64), TimeRelation, FastHasherBuilder>, 
}

struct DeviceEntry {
    name: String,

    device: Arc<PeripheralsDevice>,

    /// Last time received from this device.
    /// This is mainly used to sign extension of the u32 times to u64.
    time_epoch: TimeEpoch,
}

struct TimeEpoch {
    num_lower_bits: usize,
    last_time: Option<u64>,
}

impl TimeEpoch {
    fn new(num_lower_bits: usize) -> Self {
        Self {
            num_lower_bits,
            last_time: None
        }
    }

    fn extend_monotonic_time(&mut self, mut time: u64) -> u64 {
        // TODO: We can't trust this if the previous time wa received too long ago.

        if let Some(last_time) = self.last_time {
            let mut cycle = last_time >> self.num_lower_bits;

            let lower_mask = ((1 << self.num_lower_bits) - 1);

            if (last_time & lower_mask) > time {
                // Assumption is that we get at least one new sample each time before the clock rolls over
                cycle += 1;
            }

            time |= cycle << self.num_lower_bits;
        }

        self.last_time = Some(time);

        time
    }

    // TODO: This doesn't work for an alternative number of bits other than 32.
    fn extend_recent_time(&self, time: u32) -> u64 {
        let last_time = match self.last_time.clone() {
            Some(v) => v,
            None => return (time as u64)
        };
     
        let delta = cnc::time_difference_u32(time, last_time as u32) as i64; 

        ((last_time as i64) + delta) as u64
    }

}


#[derive(Clone)]
struct TimeSample {
    local_time: Instant,
    mcu_time: u64,
    rtt: Duration,

    frame_start_time: u64,
    frame_counter: u32,
}


impl TimeSyncer {
    pub fn create(config: &TimeControllerConfig) -> Self {
        let shared = Arc::new(Shared {
            config: config.clone(),
            state: AsyncRwLock::default(),
            start_host_time: Instant::now(),
        });
        let task = TaskResource::spawn_interruptable("TimeSyncer", Self::background_thread(shared.clone()));

        Self {
            task,
            shared,
        }
    }

    pub async fn add_device(&self, name: &str, device: Arc<PeripheralsDevice>) -> Result<()> {
        lock!(state <= self.shared.state.write().await?, {

            let state: &mut State = &mut *state;

            let id = state.last_device_id + 1;
            state.last_device_id = id;

            // Pre-initialize all relations so that we have a record of which ones are missing data.
            for other_id in [HOST_DEVICE_ID].into_iter().cloned().chain(state.devices.keys().cloned()) {
                state.relations.entry((other_id, id)).or_default();
            }

            let entry = DeviceEntry {
                name: name.to_string(),
                device,
                time_epoch: TimeEpoch::new(32),
            };
            
            if state.device_ids.insert(name.to_string(), id).is_some() {
                return Err(format_err!("Duplicate device named: {}", name));
            }
            
            state.devices.insert(id, entry);

            Ok(())
        })
    }

    async fn background_thread(shared: Arc<Shared>) -> Result<()> {
        
        // tODO: this is only valid if we get things back within 2 seconds.
        let mut absolute_frame_counter = TimeEpoch::new(11);

        loop {
            // Collect samples from all devices:
            //
            // Some important notes:
            // - We never run requests in parallel to one device so we always get an ordered list
            //   of u32 remote times 
            //   (so it is easy to expand to u64 when it overflows).
            // - We run USB requests across devices in parallel so that ideally we hit them all
            //   in the same USB frame so that we can correlate 'start of frame' events.
            //
            // TODO: If we rely on Start of Frame timings, we need a field in the config
            // to allow explicit opt in to this since it depends on the devices actually being
            // on the same USB controller and ideally the same USB hub. 
            let mut new_samples: Vec<(u64, TimeSample)> = {
                let state = shared.state.read().await?;

                let mut samples = Arc::new(AsyncMutex::new(vec![]));
                let mut bundle = TaskBundle::new();

                for (device_id, device) in &state.devices {
                    let samples = samples.clone();
                    let device_id = *device_id;
                    let device_name = device.name.clone();
                    let device = device.device.clone();
                    
                    bundle.add(async move {
                        // TODO: Need a timeout on this.
                        match Self::get_time_sample(&device_name, &device).await {
                            Ok(sample) => {
                                lock!(samples <= samples.lock().await.unwrap(), {
                                    samples.push((device_id, sample));
                                });
                            }
                            Err(e) => {
                                eprintln!("Time sync with {} failed: {}", device_name, e);
                            }
                        }
                    });
                }

                bundle.join().await;

                lock!(samples <= samples.lock().await?, {
                    samples.clone()
                })
            };

            lock!(state <= shared.state.write().await?, {

                // Extend all times to full u64
                // NOTE: Start of Frame times are always captured before the regular time on the MCU
                for (id, sample) in &mut new_samples {
                    let entry = state.devices.get_mut(id).unwrap();
                    sample.frame_start_time = entry.time_epoch.extend_monotonic_time(sample.frame_start_time);
                    sample.mcu_time = entry.time_epoch.extend_monotonic_time(sample.mcu_time);
                }

                Self::process_host_rtt_samples(
                    &shared,
                    &new_samples,
                    &mut state
                );

                if shared.config.trust_usb_sof_timing() {
                    Self::process_sof_samples(
                        &shared,
                        &new_samples,
                        &mut absolute_frame_counter,
                        &mut state
                    );
                }
            });

            // TODO: Subtract elapsed time.
            executor::sleep(POLL_PERIOD).await?;
        }
    }

    fn process_host_rtt_samples(
        shared: &Shared,
        new_samples: &[(u64, TimeSample)],
        state: &mut State,
    ) {
        for (device_id, sample) in new_samples {
            // TODO: Even if the RTT is too high, we can still use it to update the high bits of the clock.
            if sample.rtt > MAX_HOST_SYNC_RTT {
                eprintln!("RTT too large for host time sync: {:?}", sample.rtt);
                continue;
            }

            let relation_id = (HOST_DEVICE_ID, *device_id);

            let entry = state.relations.entry(relation_id).or_default();

            let point = TimeRelationPoint {
                time1: ((sample.local_time - shared.start_host_time).as_secs_f64()
                    * (MCU_CLOCK_FREQUENCY as f64)).round() as u64,
                time2: sample.mcu_time,
                frame_counter: None,
                rtt: Some(sample.rtt),
            };

            entry.add_point(point);

            if entry.total_seen_points() % LOG_INTERVAL == 0 {
                let device = state.devices.get(device_id).unwrap();

                let stats = entry.stats();
                let rtt = stats.rtt;
                println!("[{} rtt] {:?} (jitter: {:?})", device.name, rtt.min(), rtt.max() - rtt.min());
            }
        }

    }

    /// Processed time samples received from MCUs in order to populate time relations
    /// based on pairs of MCUs which have recieved the same 
    ///
    /// TODO: Don't populate any relationships that don't include the primary MCU
    ///
    // For every pair of MCUs, if we observed the time of the same USB frame counter, then we can 
    fn process_sof_samples(
        shared: &Shared,
        new_samples: &[(u64, TimeSample)],
        absolute_frame_counter: &mut TimeEpoch,
        state: &mut State,
    ) {
        for (device1_id, sample1) in new_samples {
            for (device2_id, sample2) in new_samples {
                if *device1_id >= *device2_id {
                    continue;
                }

                // TODO: Ensure always using the same device.
                let extended_frame_counter = absolute_frame_counter.extend_monotonic_time(sample1.frame_counter as u64);

                if sample1.frame_counter != sample2.frame_counter {
                    continue;
                }

                // Despite my best efforts, the MCU still sometimes produces values where the
                // frame time and frame counter don't line up. So here we do some extra filtering
                // to get rid of values that don't make sense.
                //
                // TODO: Eventually also do differential filtering across multiple packets (time should go up 
                // by a known amount based on the difference in frame counts.)
                {
                    let time_since_sof1 = sample1.mcu_time - sample1.frame_start_time;
                    let time_since_sof2 = sample2.mcu_time - sample2.frame_start_time;
                    // Frame interval is 1ms which is 16000 ticks.
                    if ((time_since_sof1 as i64) - (time_since_sof2 as i64)).abs() > 4000 ||
                        time_since_sof1 > 15900 || time_since_sof2 > 15900 {
                        eprintln!("!!! Reject disprepancy: {} vs {}", time_since_sof1, time_since_sof2);
                        continue;
                    }
                }

                let point = TimeRelationPoint {
                    time1: sample1.frame_start_time,
                    time2: sample2.frame_start_time,
                    frame_counter: Some(extended_frame_counter),
                    rtt: None,
                };

                let entry = state.relations.entry((*device1_id, *device2_id)).or_default();

                entry.add_point(point);

                if entry.total_seen_points() % 10 == 0 {
                    let device1 = state.devices.get(device1_id).unwrap();
                    let device2 = state.devices.get(device2_id).unwrap();

                    let stats = entry.stats();

                    println!("[{}<->{} timing] skew: {:.2}; error: {}",
                        device1.name, device2.name,
                        ((1.0 - stats.skew) * (MCU_CLOCK_FREQUENCY as f64)),
                        stats.max_error,
                    );
                }
            }
        }

    }

    // TODO: NEed to complain if our estimate drastically changes.

    /// Polls a single device for its current time (and SOF information).
    ///
    /// NOTE: This will return a mcu_time sample with only 32bits of informaiton.
    async fn get_time_sample(device_name: &str, device: &PeripheralsDevice) -> Result<TimeSample> {
        let mcu_time = device.get_usb_sof_time().await?;

        let rtt = mcu_time.timing.local_response_time - mcu_time.timing.local_request_time;

        let local_time = mcu_time.timing.local_response_time + (rtt / 2);

        Ok(TimeSample {
            local_time,
            mcu_time: mcu_time.timing.remote_time as u64,
            rtt,
            frame_start_time: mcu_time.frame_start_time as u64,
            frame_counter: mcu_time.frame_counter,
        })
    }

    /// Blocks until all MCUs have a synced time.
    pub async fn wait_for_sync(&self) -> Result<()> {
        // TODO: Also check that the time isn't too old.
        
        loop {
            {
                let state = self.shared.state.read().await?;

                let mut all_good = true;

                for rel in state.relations.values() {
                    if !rel.is_healthy() {
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

    /// Converts a recent device time to an absolute value.
    pub async fn wrap_raw_time(&self, device_name: &str, time: u32) -> Result<DeviceTime> {
        let state = self.shared.state.read().await?;

        let dev_id = state.device_ids.get(device_name)
            .ok_or_else(|| format_err!("No device named: {}", device_name))?;

        let dev = state.devices.get(dev_id).unwrap();

        Ok(DeviceTime {
            device_id: *dev_id,
            value: dev.time_epoch.extend_recent_time(time)
        })
    }

    /// Given a time found on one clock, estimates the time on all other devices.
    ///
    /// Currently this always converts directly from the given device to all other
    /// devices in one conversion hop.
    pub async fn all_times_at(&self, time: DeviceTime) -> Result<DevicesTimeVector> {
        let state = self.shared.state.read().await?;
        self.all_times_at_inner(time, &state)
    }

    fn all_times_at_inner(&self, time: DeviceTime, state: &State) -> Result<DevicesTimeVector> {

        /*
        TODO: If given the local time, we want to support 
        */

        // TODO: This should support using multiple hops if there is no direct relationship available
        // between two devices.

        let mut out = DevicesTimeVector::default();

        for (dev_id, dev) in &state.devices {
            out.insert(dev.name.clone(), self.convert_time(time, *dev_id, state)?);
        }

        Ok(out)
    }

    // TODO: Do not allow very far into the future estimates (or far into the past).
    pub async fn to_device_time(&self, device_name: &str, time: Instant) -> Result<DeviceTime> {
        let state = self.shared.state.read().await?;
        self.to_device_time_inner(device_name, time, &state)
    }


    fn to_device_time_inner(
        &self, device_name: &str, time: Instant, state: &State
    ) -> Result<DeviceTime> {
        let dev_id = state.device_ids.get(device_name)
            .ok_or_else(|| format_err!("No device named: {}", device_name))?;

        let dev = state.devices.get(dev_id).unwrap();

        self.convert_host_to_device_time(*dev_id, dev, time, &state)
    }

    pub async fn to_all_device_times(&self, time: Instant) -> Result<DevicesTimeVector> {
        let state = self.shared.state.read().await?;
        let mut out = HashMap::default();

        if !self.shared.config.primary_device_name().is_empty() {
            let primary_device_time = self.to_device_time_inner(
                self.shared.config.primary_device_name(), time, &state)?;

            return self.all_times_at_inner(primary_device_time, &state);
        }

        for (dev_id, dev) in &state.devices {
            out.insert(dev.name.clone(), self.convert_host_to_device_time(*dev_id, dev, time, &state)?);
        }

        Ok(out)
    }

    pub async fn to_primary_clock(&self, time: DeviceTime) -> Result<DeviceTime> {
        let state = self.shared.state.read().await?;

        let primary_dev_id = *state.device_ids.get(self.shared.config.primary_device_name())
            .ok_or_else(|| err_msg("Unknown primary device name"))?;

        self.convert_time(time, primary_dev_id, &state)
    }

    fn convert_host_to_device_time(
        &self, dev_id: u64, dev: &DeviceEntry, time: Instant, state: &State
    ) -> Result<DeviceTime> {

        // TODO: Dedup this.
        let time_ticks = ((time - self.shared.start_host_time).as_secs_f64() * (MCU_CLOCK_FREQUENCY as f64))
            .round() as u64;

        let host_time = DeviceTime {
            device_id: HOST_DEVICE_ID,
            value: time_ticks
        };

        self.convert_time(host_time, dev_id, state)
    }

    fn convert_time(&self, time: DeviceTime, target_device_id: u64, state: &State) -> Result<DeviceTime> {
        if target_device_id == time.device_id {
            return Ok(time);
        }

        let (forward, relation_id) = {
            if time.device_id < target_device_id {
                (true, (time.device_id, target_device_id))
            } else {
                (false, (target_device_id, time.device_id))
            }
        };

        let relation = state.relations.get(&relation_id).unwrap();

        let dev_time = relation.convert_time(time.value, forward)?;

        Ok(DeviceTime {
            device_id: target_device_id,
            value: dev_time
        })
    }

}

pub type DevicesTimeVector = HashMap<String, DeviceTime, FastHasherBuilder>;

// TODO: Prevent comparisons across different devices.
#[derive(Debug, Clone, Copy, PartialOrd, PartialEq)]
pub struct DeviceTime {
    device_id: u64,
    value: u64,
}

impl DeviceTime {
    // #[cfg(test)]
    pub fn new_test_only(device_id: u64, v: u64) -> Self {
        Self { device_id, value: v }
    }

    pub fn lower(&self) -> u32 {
        self.value as u32
    }

    pub fn raw(&self) -> u64 {
        self.value
    }

    pub fn add_ticks(self, ticks: u32) -> Self {
        Self { device_id: self.device_id, value: self.value + (ticks as u64) }
    }

    pub fn add_ticks_u64(self, ticks: u64) -> Self {
        Self { device_id: self.device_id, value: self.value + (ticks as u64) }
    }

    pub fn add_secs(self, secs: f64) -> Self {
        let mut delta = (secs * (MCU_CLOCK_FREQUENCY as f64)).round() as u64;

        // let delta = (1000000000u64 * (dur.as_nanos() as u64)) / MCU_CLOCK_FREQUENCY;
        Self { device_id: self.device_id, value: self.value + delta }
    }

    pub fn add_duration(self, dur: Duration) -> Self {
        let mut delta = (dur.as_secs_f64() * (MCU_CLOCK_FREQUENCY as f64)).round() as u64;

        // let delta = (1000000000u64 * (dur.as_nanos() as u64)) / MCU_CLOCK_FREQUENCY;
        Self { device_id: self.device_id, value: self.value + delta }
    }

    pub fn sub_duration(self, dur: Duration) -> Self {
        let mut delta = (dur.as_secs_f64() * (MCU_CLOCK_FREQUENCY as f64)).round() as u64;

        // let delta = (1000000000u64 * (dur.as_nanos() as u64)) / MCU_CLOCK_FREQUENCY;
        Self { device_id: self.device_id, value: self.value - delta }
    }

    pub fn sub(self, other: DeviceTime) -> Self {
        assert_eq!(self.device_id, other.device_id);

        Self {
            device_id: self.device_id,
            value: self.value - other.value
        }

    }
}



