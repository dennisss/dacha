use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use common::errors::*;
use executor::lock;
use common::hash::FastHasherBuilder;
use executor::sync::AsyncRwLock;
use peripherals_service::device::PeripheralsDevice;
use peripherals_service::config::BoardConfigRegistry;
use cnc_controller_proto::cnc::*;

use crate::tmc2209::TMC2209Device;
use crate::time::*;


/// TODO: Think of a better name for this.
pub struct DevicesController {
    // TODO: This would benefit from a read-copy-update style approach
    // TODO: Eventually mark this as frozen so that writes aren't allowed but reads can be cheap.
    state: AsyncRwLock<State>,

    time: Arc<TimeSyncer>,
}

// TODO: Need to do propagation of ServiceResources.

struct State {
    entries: HashMap<String, DeviceEntry, FastHasherBuilder>
}

enum DeviceEntry {
    PeripheralsDevice(Arc<PeripheralsDevice>),
    TMC2209(Arc<TMC2209Device>)
}

impl DevicesController {

    pub async fn create(config: &ControllerConfig) -> Result<Arc<Self>> {
        let mut board_registry = BoardConfigRegistry::defaults().await?;

        let mut inst = Arc::new(Self {
            state: AsyncRwLock::new(State {
                entries: HashMap::default()
            }),
            time: Arc::new(TimeSyncer::create())
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

                    DeviceEntry::PeripheralsDevice(device)

                } else if proto.has_tmc2209() {
                    // TODO: Eventually feed an Arc<DevicesController> to the TMC2209 instance.

                    let dev = inst.get_peripherals_device(proto.tmc2209().device_name()).await?;
                    let inst = TMC2209Device::create(proto.tmc2209().clone(), dev).await?;

                    DeviceEntry::TMC2209(Arc::new(inst))
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

    async fn get_peripherals_device(&self, name: &str) -> Result<Arc<PeripheralsDevice>> {
        let state = self.state.read().await?;
        match state.entries.get(name) {
            Some(DeviceEntry::PeripheralsDevice(d)) => Ok(d.clone()),
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

    pub async fn to_device_time(&self, device_name: &str, time: Instant) -> Result<u32> {
        self.time.to_device_time(device_name, time).await
    }

}
