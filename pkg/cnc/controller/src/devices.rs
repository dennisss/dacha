use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use common::io::*;
use common::errors::*;
use executor::lock;
use common::hash::FastHasherBuilder;
use executor::sync::{AsyncRwLock, AsyncMutex};
use peripherals_service::device::PeripheralsDevice;
use peripherals_service::config::BoardConfigRegistry;
use executor::bundle::TaskResultBundle;
use cnc_controller_proto::cnc::*;
use peripherals_proto::peripherals::{PeripheralRequest, PeripheralResponse};
use peripherals_service::utilization_tracker::*;

use crate::tmc2209::TMC2209Device;
use crate::time::*;
use crate::bed::client::BedClient;


/// TODO: Think of a better name for this.
pub struct DevicesController {
    // TODO: This would benefit from a read-copy-update style approach
    // TODO: Eventually mark this as frozen so that writes aren't allowed but reads can be cheap.
    state: AsyncRwLock<State>,

    time: Arc<TimeSyncer>,

    utilization_tracker: Arc<RemoteUtilizationTracker>,
}

// TODO: Need to do propagation of ServiceResources (especially for each device's background thread).

struct State {
    entries: HashMap<String, DeviceEntry, FastHasherBuilder>
}

enum DeviceEntry {
    PeripheralsDevice(Arc<PeripheralsDevice>),
    TMC2209(Arc<TMC2209Device>),
    BedClient(Arc<BedClient>),
}

impl DevicesController {

    pub async fn create(config: &ControllerConfig) -> Result<Arc<Self>> {
        let mut board_registry = BoardConfigRegistry::defaults().await?;

        let mut inst = Arc::new(Self {
            state: AsyncRwLock::new(State {
                entries: HashMap::default()
            }),
            time: Arc::new(TimeSyncer::create()),
            utilization_tracker: Arc::new(RemoteUtilizationTracker::create()),
        });

        for proto in config.devices() {

            let device_entry = {
                if proto.has_peripheral_config() {
                    let board_config = board_registry.compile(proto.peripheral_config())?;
                    let (device, _) = PeripheralsDevice::create(&board_config).await?;
                    let device = Arc::new(device);

                    // TODO: We can optimize some of the locks in the TimeSyncer if we just
                    // pre-initialize all the PeripheralsDevices before other devices.
                    inst.time.add_device(proto.name(), device.clone()).await?;
                    inst.utilization_tracker.add_device(proto.name(), device.clone()).await?;

                    DeviceEntry::PeripheralsDevice(device)

                } else if proto.has_tmc2209() {
                    // TODO: Eventually feed an Arc<DevicesController> to the TMC2209 instance.

                    let dev = inst.get_peripherals_device(proto.tmc2209().device_name()).await?;
                    let inst = TMC2209Device::create(proto.tmc2209().clone(), dev).await?;

                    DeviceEntry::TMC2209(Arc::new(inst))
                } else if proto.has_bed_client() {
                    
                    let dev = inst.get_peripherals_device(proto.bed_client().serial_peripheral().device_name()).await?;

                    // serial_peripheral

                    todo!()

                } else {
                    return Err(err_msg("Unknown type of device entry"));
                }
            };

            lock!(state <= inst.state.write().await?, {
                if state.entries.insert(proto.name().to_string(), device_entry).is_some() {
                    return Err(format_err!("Duplicate device named: {}", proto.name()));
                }

                Result::<_, Error>::Ok(())
            })?;
        }

        Ok(inst)
    }

    pub async fn get_peripherals_device(&self, name: &str) -> Result<Arc<PeripheralsDevice>> {
        let state = self.state.read().await?;
        match state.entries.get(name) {
            Some(DeviceEntry::PeripheralsDevice(d)) => Ok(d.clone()),
            Some(_) => Err(format_err!("Wrong type of device for '{}'", name)),
            None => Err(format_err!("No such device named '{}'", name))
        }
    }

    pub async fn get_bed_client(&self, name: &str) -> Result<Arc<BedClient>> {
        let state = self.state.read().await?;
        match state.entries.get(name) {
            Some(DeviceEntry::BedClient(d)) => Ok(d.clone()),
            Some(_) => Err(format_err!("Wrong type of device for '{}'", name)),
            None => Err(format_err!("No such device named '{}'", name))
        }
    }

    pub async fn get_motor(&self, name: &str) -> Result<Arc<TMC2209Device>> {
        let state = self.state.read().await?;
        match state.entries.get(name) {
            Some(DeviceEntry::TMC2209(d)) => Ok(d.clone()),
            Some(_) => Err(format_err!("Wrong type of device for '{}'", name)),
            None => Err(format_err!("No such device named '{}'", name))
        }
    }

    pub fn time(&self) -> &TimeSyncer {
        &self.time
    }

    pub fn new_batch(self: &Arc<Self>) -> DevicesPeripheralRequestBatch {
        DevicesPeripheralRequestBatch {
            devices: self.clone(),
            requests_by_device: HashMap::default(),
            results: vec![],
        }
    }
}


pub struct DevicesPeripheralRequestBatch {
    devices: Arc<DevicesController>,
    results: Vec<PeripheralResponse>,
    requests_by_device: HashMap<String, DeviceBatchEntry, FastHasherBuilder>
}

#[derive(Default)]
struct DeviceBatchEntry {
    indexes: Vec<usize>,
    requests: Vec<PeripheralRequest>
}

const MAX_BATCH_SIZE: usize = 64;

impl DevicesPeripheralRequestBatch {

    // TODO: Consider using some other request format that allows the generator of the request to
    // include the device name.
    pub fn add(&mut self, device_name: &str, request: PeripheralRequest) {
        let index = self.results.len();
        self.results.push(PeripheralResponse::default());

        let entry = self.requests_by_device.entry(device_name.to_string()).or_default();
        entry.indexes.push(index);
        entry.requests.push(request);
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }

    pub async fn send(self) -> Result<Vec<PeripheralResponse>> {
        let results = Arc::new(AsyncMutex::new(self.results));

        let mut bundle = TaskResultBundle::new();

        for (device_name, entry) in self.requests_by_device {
            let dev = self.devices.get_peripherals_device(&device_name).await?;

            let results = results.clone();
            bundle.add(&format!("{} send", device_name), async move {

                let iter = entry.requests.chunks(MAX_BATCH_SIZE).zip(entry.indexes.chunks(MAX_BATCH_SIZE));

                for (requests, indexes) in iter {
                    let responses = dev.send_request_batch(requests).await?;

                    lock!(results <= results.lock().await?, {
                        for (res, index) in responses.into_iter().zip(indexes.iter()) {
                            results[*index] = res;
                        }
                    });
                }

                Ok(())
            });
        }

        bundle.join().await?;

        let mut final_results = vec![];
        lock!(results <= results.lock().await?, {
            core::mem::swap(&mut *results, &mut final_results);
        });

        Ok(final_results)
    }

}

pub struct RemoteSerialPort {
    device: Arc<PeripheralsDevice>,
    peripheral_name: String
}

#[async_trait]
impl Readable for RemoteSerialPort {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.device.uart_transfer(
            &self.peripheral_name,
            &[],
            buf
        ).await
    }
}

#[async_trait]
impl Writeable for RemoteSerialPort {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.device.uart_transfer(
            &self.peripheral_name,
            &buf,
            &mut []
        ).await?;
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

