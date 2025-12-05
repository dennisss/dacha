use std::time::{Instant, Duration};
use std::collections::HashMap;
use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::lock;
use executor::sync::AsyncRwLock;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use peripherals_service::device::PeripheralsDevice;

const POLL_PERIOD: Duration = Duration::from_secs(2);

pub struct RemoteUtilizationTracker {
    task: TaskResource,

    shared: Arc<Shared>,
}

impl_resource_passthrough!(RemoteUtilizationTracker, task);

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
    last_sample: Option<Sample>
}

struct Sample {
    local_time: Instant,
    remote_counter: u32
}


impl RemoteUtilizationTracker {
    pub fn create() -> Self {
        let shared = Arc::new(Shared::default());
        let task = TaskResource::spawn_interruptable("RemoteUtilizationTracker", Self::background_thread(shared.clone()));

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
                            eprintln!("Utilization check with {} failed: {}", device_name, e);
                        }
                    }
                }
            }

            lock!(state <= shared.state.write().await?, {
                for (name, sample) in new_samples {
                    let entry = state.devices.get_mut(&name).unwrap();
                    
                    if let Some(last_sample) = &entry.last_sample {
                        let time_delta = sample.local_time - last_sample.local_time;

                        // TODO: Make a helper function for doing this.
                        let count_delta = {
                            let mut v = sample.remote_counter.wrapping_sub(last_sample.remote_counter);
                            if sample.remote_counter < last_sample.remote_counter {
                                v = v.wrapping_add(u32::max_value());
                            }

                            v
                        };

                        // TODO: Pull the ticks_per_second number from the MCU.
                        let idle_utilization = ((count_delta as f64) / (64_000_000.0 / 10.0)) / time_delta.as_secs_f64();

                        println!("[{} utilization] {:.3}", name, 1.0 - idle_utilization);
                    }
                    
                    entry.last_sample = Some(sample);
                }
            });

            // TODO: Subtract elapsed time.
            executor::sleep(POLL_PERIOD).await?;
        }
    }

    async fn get_time_sample(device: &PeripheralsDevice) -> Result<Sample> {
        let remote_counter = executor::timeout(Duration::from_millis(1000), device.get_idle_counter()).await??;
        let local_time = Instant::now();

        Ok(Sample {
            local_time,
            remote_counter
        })
    }
}


