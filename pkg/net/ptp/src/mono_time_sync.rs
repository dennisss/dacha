use std::sync::Arc;
use std::collections::VecDeque;
use std::time::Duration;

use common::errors::*;
use sys::ClockId;
use executor::sync::SyncMutex;
use executor_multitask::{TaskResource, impl_resource_passthrough};

use crate::device::PTPDevice;


const MAX_ENTRIES: usize = 8;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// Background service that maintains a mapping from Linux's CLOCK_MONOTONIC to a
/// PTP clock.
pub struct MonotonicClockTimeSyncer {
    task: TaskResource,
    shared: Arc<Shared>
}

impl_resource_passthrough!(MonotonicClockTimeSyncer, task);

#[derive(Default)]
struct Shared {
    state: SyncMutex<State>
}

#[derive(Default)]
struct State {
    entries: VecDeque<Entry>
}

#[derive(Clone)]
struct Entry {
    monotonic_time: Duration,
    ptp_time: Duration,
}


impl MonotonicClockTimeSyncer {

    pub fn create(ptp_device: Arc<PTPDevice>) -> Result<Self> {

        let shared = Arc::new(Shared::default());
        Self::collect_sample(&shared, &ptp_device)?;

        let task = TaskResource::spawn_interruptable(
            "MonotonicClockTimeSyncer",
            Self::background_thread(shared.clone(), ptp_device)
        );

        Ok(Self {
            task, shared
        })
    }

    fn collect_sample(shared: &Shared, ptp_device: &PTPDevice) -> Result<()> {
        // TODO: Sometimes this gets "[ETIMEDOUT] Connection timed out"
        let offset = ptp_device.offset_to(ClockId::MONOTONIC)?;

        if offset.rtt > Duration::from_millis(1) {
            return Err(format_err!("Took too long to sync the monotonic and ptp clocks: {:?}", offset.rtt));
        }

        let entry = Entry {
            monotonic_time: offset.other_time,
            ptp_time: offset.ptp_time
        };

        shared.state.apply(|state| {
            state.entries.push_back(entry);
            while state.entries.len() > MAX_ENTRIES {
                state.entries.pop_front();
            }
        })?;

        Ok(())
    }

    async fn background_thread(shared: Arc<Shared>, ptp_device: Arc<PTPDevice>) -> Result<()> {
        loop {
            if let Err(e) = Self::collect_sample(&shared, &ptp_device) {
                eprintln!("[MonotonicClockTimeSyncer] Error: {}", e);
            }
            executor::sleep(SAMPLE_INTERVAL).await?;
        }
    }

    // TODO: Need to return None if we don't have any recent data.
    // TODO: This may produce errors due to some times being near zero if we use it before PTP is fully initialized to a non-zero time. 
    pub fn monotonic_to_ptp_time(&self, time: Duration) -> Duration {
        let nearest = self.shared.state.apply(|state| {
            state.entries.iter().min_by_key(|entry| {
                time.abs_diff(entry.monotonic_time)
            }).unwrap().clone()
        }).unwrap();

        if time >= nearest.monotonic_time {
            let offset = time - nearest.monotonic_time;
            nearest.ptp_time + offset
        } else {
            let offset = nearest.monotonic_time - time;
            nearest.ptp_time - offset
        }
    }

}
