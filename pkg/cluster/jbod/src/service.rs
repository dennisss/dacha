use std::sync::Arc;
use std::collections::{HashSet, HashMap};
use std::time::{Instant, Duration};

use common::errors::*;
use executor::sync::AsyncMutex;
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use cluster_jbod_proto::cluster::*;
use executor::{lock_async, lock};
use storage::scsi::{SCSISmartBytes, SCSIErrorCounters};

use crate::topology::*;
use crate::management::*;

const DISK_SCSI_DATA_CACHE_TIME: Duration = Duration::from_secs(30);


pub struct EnclosureServiceInst {
    shared: Arc<Shared>,
    resources: ServiceResourceGroup
}

// TODO: Need to also passthrough the management USB device.
impl_resource_passthrough!(EnclosureServiceInst, resources);

struct Shared {
    management: ManagementDevice,
    state: AsyncMutex<State>
}

struct State {
    psus: HashMap<EnclosureSide, PSUState>,
    
    /// Last time the power states changes.
    last_power_transition: Instant,

    fan_duty_cycle: f32,

    fan_speeds: Vec<f32>,

    sas_expander_positions: HashMap<String, EnclosureSide>,

    last_state: EnclosureState,

    scsi_data_cache: HashMap<String, DiskSCSIData>,

    led_mode: LEDStripMode,
}

#[derive(Debug, Default, Clone)]
struct PSUState {
    on: bool,
    sas_on: bool,
}

#[derive(Clone)]
struct DiskSCSIData {
    temperature: f32,
    serial: String,
    smart: SCSISmartBytes,
    error_counters: SCSIErrorCounters,
    retrieved_at: Instant,
}

impl EnclosureServiceInst {

    pub async fn create() -> Result<Self> {
        let resources = ServiceResourceGroup::new("EnclosureServiceInst");

        let mut management = ManagementDevice::create().await?;

        let fan_duty_cycle = 1.0;
        management.set_fan_speed(fan_duty_cycle).await?;

        let shared = Arc::new(Shared {
            management,
            state: AsyncMutex::new(State {
                psus: HashMap::new(),
                last_power_transition: Instant::now(),
                fan_duty_cycle,
                fan_speeds: vec![],
                sas_expander_positions: HashMap::new(),
                last_state: EnclosureState::default(),
                scsi_data_cache: HashMap::new(),
                led_mode: LEDStripMode::OFF
            })
        });

        resources.spawn_interruptable(
            "EnclosureServiceInst::run()",
            Self::run(shared.clone()),
        ).await;

        resources.spawn_interruptable(
            "EnclosureServiceInst::read_fan_speed()",
            Self::read_fan_speed(shared.clone()),
        ).await;

        resources.spawn_interruptable(
            "EnclosureServiceInst::update_leds()",
            Self::update_leds(shared.clone()),
        ).await;

        Ok(Self { shared, resources })
    }

    // TODO: Make CANCEL SAFE
    pub async fn execute(&self, request: &ExecuteRequest) -> Result<ExecuteResponse> {

        if request.has_toggle_psu() {
            let cmd = request.toggle_psu();

            let side = match cmd.psu_name() {
                "left" => EnclosureSide::Left,
                "right" => EnclosureSide::Right,
                _ => return Err(err_msg("Unknown side"))
            };

            lock_async!(state <= self.shared.state.lock().await?, {

                let state: &mut State = &mut state;

                let psu_state = state.psus.entry(side).or_default();
                
                if cmd.on() != psu_state.on {
                    // if cmd.on() {

                    // }

                    psu_state.on = cmd.on();
                    self.shared.management.toggle_psu_power(side, psu_state.on).await?;
                }

                if cmd.sas_on() != psu_state.sas_on {
                    // TODO: Verify outputs are stable and PSU is on.


                    psu_state.sas_on = cmd.sas_on();
                    self.shared.management.toggle_sas_power(side, psu_state.sas_on).await?;
                }


                state.last_power_transition = Instant::now();

                // TODO: Update the last_power_trnasition time.


                Result::<_, Error>::Ok(())
            })?;


        }

        if request.has_set_led_state() {
            let cmd = request.set_led_state();

            lock!(state <= self.shared.state.lock().await?, {
                state.led_mode = cmd.mode();
            });
        }

        if request.has_set_fan_speed() {
            //  request.set_fan_speed().duty_cycle()
        }

        Ok(ExecuteResponse::default())
    }

    async fn run(shared: Arc<Shared>) -> Result<()> {

        // TODO: retry errors.

        loop {
            let good = lock_async!(state <= shared.state.lock().await?, {
                match Self::read_state_proto(&shared, &mut state).await {
                    Ok(v) => {
                        state.last_state = v;
                        true   
                    },
                    Err(e) => {
                        eprintln!("Failed to read state: {}", e);
                        false
                    }
                }
            });

            if !good {
                executor::sleep(Duration::from_millis(5000)).await?;    
            }

            // println!("{:?}", proto);

            // TODO: Eventually subscribe to udev for updates.
            // there is a risk we will get permission denied errors if udev hasn't finished setting up a device.

            // TODO: Use a time remaining based sleep.
            executor::sleep(Duration::from_millis(200)).await?;
        }


    }

    // This is separate from the main loop as reading fan tachometers is relatively slow.
    async fn read_fan_speed(shared: Arc<Shared>) -> Result<()> {

        loop {
            let speeds = shared.management.get_fan_speeds().await?;

            // TODO: Ensure that we don't complain about stopped fans if the PSUs are all off.
            lock!(state <= shared.state.lock().await?, {
                state.fan_speeds = speeds;
            });

            // TODO: Base sleep time of amount of time spent reading fan speeds.
            executor::sleep(Duration::from_millis(100)).await?;
        }

        Ok(())
    }

    // sudo sg_start --stop /dev/sda && echo 1 | sudo tee /sys/block/sda/device/delete

    async fn update_leds(shared: Arc<Shared>) -> Result<()> {
        let ordering = led_grid_ordering();

        loop {
            let proto = lock!(state <= shared.state.lock().await?, {
                state.last_state.clone()
            });

            // NOTE: Ordering is GRB
            let mut led_data = vec![0u8; ordering.len() * 3];

            match proto.leds().mode() {
                LEDStripMode::UNKNOWN => {},
                LEDStripMode::OFF => {},
                LEDStripMode::STATUS_ID => {
                    for (i, (a, b)) in ordering.iter().cloned().enumerate() {
                        let mut good = true;
                        
                        let bay_i = {
                            if let Some(v) = a {
                                v
                            } else {
                                b.unwrap()
                            }
                        };

                        if proto.bays()[bay_i].connected_device_name().is_empty() {
                            good = false;
                        }

                        led_data[(3*i)..(3*(i + 1))].copy_from_slice(
                            if good {
                                // blue
                                &[0x00, 0x00, 0x20]
                            } else {
                                // red
                                &[0x00, 0x20, 0x00]
                            }
                        );
                    }
                }
                LEDStripMode::MANUAL => {
                    // TODO:
                }
            }

            shared.management.set_led_data(&leds_from_grid_order(&led_data)).await?;

            executor::sleep(Duration::from_millis(100)).await?;
        }

        Ok(())
    }

    // TODO: Refactor to not lock the state the entire time.
    async fn read_state_proto(shared: &Shared, state: &mut State) -> Result<EnclosureState> {

        let mut proto = EnclosureState::default();

        {
            let mut fan_group = proto.new_fan_groups();
            fan_group.set_duty_cycle(state.fan_duty_cycle);
            
            for (name, speed) in FAN_NAMES.iter().cloned().zip(state.fan_speeds.iter().cloned()) {
                let fan = fan_group.new_fans();
                fan.set_name(name);
                fan.set_measured_speed(speed);
            }
        }

        {
            proto.leds_mut().set_mode(state.led_mode);
            // TODO: Also export the current colors
        }

        for side in [EnclosureSide::Left, EnclosureSide::Right] {
            let proto = proto.new_psus();
            let voltages = shared.management.read_psu_voltages(side).await?;

            proto.set_name(side.to_str());

            proto.set_waiting_for_power_on(voltages.waiting_for_power_on());
            proto.set_output_stable(voltages.output_stable());
            proto.set_voltage_5(voltages.v5);
            proto.set_voltage_12(voltages.v12);
            proto.set_voltage_ps_on(voltages.ps_on);

            let psu_state = state.psus.entry(side).or_default();

            proto.set_on(psu_state.on);
            proto.set_sas_on(psu_state.sas_on);
        }

        // Map from device paths to the name of their generic driver.
        let generics_map = {
            let generics = storage::scsi::SCSIGenericDeviceEntry::list().await?;
            let mut out = HashMap::new();
            for g in generics {
                out.insert(g.device_path, g.name);
            }

            out
        };

        // Set of all device paths corresponding to enclosure devices.
        let enclosures_set = {
            let enclosures = storage::enclosure::EnclosureEntry::list().await?;
            let mut out = HashSet::new();
            for e in enclosures {
                out.insert(e.device_path);
            }

            out            
        };

        struct ExpanderConnection {
            phy_num: usize,
            expander_name: String,
            side: Option<EnclosureSide>
        }

        // Enumerating expanders.
        // Expanders should have only have devices connected via a single Phy
        // The connected devices are either disks or the special 'enclosure' device.
        let mut expander_connections = HashMap::new();
        {
            let expanders = storage::sas::SASExpander::list().await?;

            let now = Instant::now();
            if now - state.last_power_transition > Duration::from_secs(1) {
                let mut unknown_sides = HashSet::new();
                for (side, psu_state) in &state.psus {
                    if psu_state.sas_on {
                        unknown_sides.insert(*side);
                    }
                }

                for v in state.sas_expander_positions.values() {
                    unknown_sides.remove(v);
                }

                let mut unknown_expanders = vec![];
                for expander in &expanders {
                    if state.sas_expander_positions.contains_key(&expander.sas_address) {
                        continue;
                    }

                    unknown_expanders.push(expander.sas_address.clone());
                }

                if unknown_expanders.len() == 1 && unknown_sides.len() == 1 {
                    let addr = unknown_expanders.pop().unwrap();
                    let side = *unknown_sides.iter().next().unwrap();

                    println!("Registering SAS addr {} as {} expander", addr, side.to_str());
                    state.sas_expander_positions.insert(
                        addr,
                        side
                    );
                }
            }

            // TODO: Need to isolate individual expander failures.
            // (ideally we report individual device errors to the UI)
            for expander in expanders {

                let mut enclosure_name = None;

                let side = state.sas_expander_positions.get(&expander.sas_address).cloned();

                for port in expander.ports {
                    if port.phys.len() != 1 {
                        return Err(err_msg("Expected each expander port to correspond to one phy"));
                    }

                    // Probably not fully set up yet.
                    if port.inner_device_paths.is_empty() {
                        continue;
                    }

                    if port.inner_device_paths.len() != 1 {
                        return Err(err_msg("Expected a single inner device per expander port"));
                    }

                    let phy_num = port.phys[0];
                    let inner_device_path = &port.inner_device_paths[0];

                    if enclosures_set.contains(inner_device_path) {
                        if enclosure_name.is_some() {
                            return Err(err_msg("Found multiple enclosure devices for an expander"));
                        }

                        let generic_name = generics_map.get(inner_device_path)
                            .ok_or_else(|| err_msg("Missing generic for enclosure device"))?
                            .clone();

                        enclosure_name = Some(generic_name);

                        continue;
                    }

                    // Else, the device is probably a disk.

                    expander_connections.insert(inner_device_path.to_owned(), ExpanderConnection {
                        phy_num,
                        expander_name: expander.name.clone(),
                        side,
                    });
                }


                let enclosure_name = enclosure_name
                    .ok_or_else(|| err_msg("Failed to find enclosure device for expander"))?;

                let temperature = {
                    let mut disk = file::LocalFile::open_with_options(
                        format!("/dev/{}", enclosure_name),
                        &file::LocalFileOpenOptions::new().read(true).write(true),
                    )?;

                    let mut scsi = storage::scsi::SCSIDevice::create(disk)?;

                    scsi.scsi_enclosure_temperature()? as f32
                };

                let mut proto = proto.new_storage_devices();
                proto.set_name(expander.name);
                proto.set_usage(StorageDeviceUsage::EXPANDER);
                proto.set_model(format!("{} {} ({})", expander.vendor_id, expander.product_id, expander.product_rev));
                proto.set_wwid(expander.sas_address);
                proto.set_temperature(temperature);
                if let Some(side) = side {
                    proto.set_position(side.to_str());
                }
            }
        }

        for _ in 0..45 {
            proto.new_bays();
        }

        {
            let block_devs = storage::devices::BlockDevice::list().await?;

            // TODO: NEed error independence across disks.
            for block_dev in block_devs {
                if block_dev.sas_address.is_none() {
                    continue;
                }

                let sas_address = block_dev.sas_address.as_ref().unwrap();
                let device_path = block_dev.device_path.as_ref()
                    .ok_or_else(|| err_msg("Missing SAS drive device_path"))?;
                let model = block_dev.model.as_ref()
                    .ok_or_else(|| err_msg("Missing SAS drive model"))?;


                let scsi_data = Self::get_disk_scsi_data(&sas_address, &block_dev, state).await?;

                let mut dev_proto = proto.new_storage_devices();
                dev_proto.set_name(&block_dev.name);
                dev_proto.set_usage(StorageDeviceUsage::DISK);
                dev_proto.set_model(model);
                dev_proto.set_serial_number(scsi_data.serial);
                dev_proto.set_wwid(sas_address);
                dev_proto.set_temperature(scsi_data.temperature);

                let disk_stats = dev_proto.disk_stats_mut();

                disk_stats.set_smart_status(if scsi_data.smart.is_ok() {
                    "OK".to_string()
                } else {
                    format!(
                        "ERROR({:02x}, {:02x})",
                        scsi_data.smart.sense_code_byte,
                        scsi_data.smart.sense_qualifier
                    )
                });

                disk_stats.set_read_soft_errors(scsi_data.error_counters.read_soft_errors);
                disk_stats.set_read_hard_errors(scsi_data.error_counters.read_hard_errors);
                disk_stats.set_write_soft_errors(scsi_data.error_counters.write_soft_errors);
                disk_stats.set_write_hard_errors(scsi_data.error_counters.write_hard_errors);

                if let Some(connection) = expander_connections.get(device_path) {
                    dev_proto.set_parent(&connection.expander_name);

                    if let Some(side) = connection.side {
                        if let Some(bay_num) = expander_phy_to_bay_number(side, connection.phy_num) {
                            proto.bays_mut()[bay_num].set_connected_device_name(&block_dev.name);
                        }
                    }
                }
            }
        }

        /*

        message EnclosureState {
            repeated FanGroupState fan_groups = 1;

            repeated DiskBayState bays = 2;    

            repeated PowerSupplyState psus = 3;

            LEDStripState leds = 4;

            repeated StorageDeviceState storage_devices = 5;
        }

        */




        Ok(proto)
    }

    // NOTE: Disk SCSI logs or other data requires disk seeks to retrieve so we cache the data
    // and grab it infrequently.
    async fn get_disk_scsi_data(
        sas_address: &str,
        block_dev: &storage::devices::BlockDevice,
        state: &mut State
    ) -> Result<DiskSCSIData> {
        let now = Instant::now();

        if let Some(data) = state.scsi_data_cache.get(sas_address) {
            if data.retrieved_at + DISK_SCSI_DATA_CACHE_TIME > now {
                return Ok(data.clone());
            }
        }

        let mut scsi = storage::scsi::SCSIDevice::create({
            file::LocalFile::open_with_options(
                format!("/dev/{}", block_dev.name),
                &file::LocalFileOpenOptions::new().read(true).write(true),
            )?
        })?;

        let serial = scsi.unit_serial_number()?;
    
        let temperature = scsi.scsi_temperature()?.current_temp as f32;

        let smart = scsi.scsi_smart_sense()?;

        let error_counters = scsi.scsi_error_counters()?;


        let data = DiskSCSIData {
            temperature,
            serial,
            smart,
            error_counters,
            retrieved_at: now
        };

        state.scsi_data_cache.insert(sas_address.to_owned(), data.clone());

        Ok(data)
    }

}

#[async_trait]
impl EnclosureService for EnclosureServiceInst {
    async fn GetState(
        &self,
        request: rpc::ServerRequest<GetStateRequest>,
        response: &mut rpc::ServerResponse<EnclosureState>,
    ) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            response.value = state.last_state.clone();
        });

        Ok(())
    }

    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>,
    ) -> Result<()> {
        response.value = self.execute(&request.value).await?;
        Ok(())
    }
}