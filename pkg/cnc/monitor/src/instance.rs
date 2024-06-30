use std::collections::HashSet;
use std::sync::Weak;
use std::time::{Duration, Instant, SystemTime};
use std::{collections::HashMap, sync::Arc};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::io::{Readable, Writeable};
use crypto::random::SharedRngExt;
use executor::cancellation::AlreadyCancelledToken;
use executor::child_task::ChildTask;
use executor::sync::AsyncMutex;
use executor::sync::AsyncRwLock;
use executor::{channel, lock, lock_async};
use executor_multitask::{impl_resource_passthrough, ServiceResource, TaskResource};
use file::{LocalPath, LocalPathBuf};
use media_web::camera_manager::CameraManager;
use protobuf::Message;

use crate::camera_controller::CameraController;
use crate::change::{ChangeDistributer, ChangeEvent};
use crate::config::MachineConfigContainer;
use crate::devices::*;
use crate::files::{FileManager, FileReference};
use crate::player::Player;
use crate::program::ProgramSummary;
use crate::protobuf_table::ProtobufDB;
use crate::serial_controller::DEFAULT_COMMAND_TIMEOUT;
use crate::tables::{FILE_TABLE_TAG, MACHINE_TABLE_TAG};
use crate::{presets::get_machine_presets, serial_controller::SerialController};

const RETRY_BACKOFF: Duration = Duration::from_secs(10);

/// Maximum number of locally connected machines.
const MAX_NUM_MACHINES: usize = 10;

type MachineId = u64;

type FileId = u64;

pub struct MonitorImpl {
    shared: Arc<Shared>,

    /// Resource running Self::run().
    task_resource: TaskResource,
}

impl_resource_passthrough!(MonitorImpl, task_resource);

struct Shared {
    local_data_dir: LocalPathBuf,
    config_presets: Vec<MachineConfig>,
    changes: ChangeDistributer,
    camera_manager: Arc<CameraManager>,
    db: Arc<ProtobufDB>,
    files: FileManager,
    state: AsyncMutex<State>,
    force_reconcile: channel::Sender<()>,
}

#[derive(Default)]
struct State {
    // Machines indexed by id.
    machines: HashMap<MachineId, MachineEntry>,

    all_devices: Vec<DeviceEntry>,
}

struct MachineEntry {
    id: u64,

    config: Arc<AsyncRwLock<MachineConfigContainer>>,

    // TODO: Dynamically add these to the resource group.
    /// If not None,
    serial: SerialEntry,

    loaded_file: Option<FileReference>,

    player: Option<PlayerEntry>,

    /// This will usually contain one entry for every camera defined in
    /// 'config'.
    cameras: HashMap<u64, CameraEntry>,
    /*
    - Loaded file.
    - Mesh leveling grid (when external to the machine)
    */
}

impl MachineEntry {
    fn new(id: u64, config: MachineConfigContainer) -> Self {
        Self {
            id,
            config: Arc::new(AsyncRwLock::new(config)),
            serial: SerialEntry::default(),
            loaded_file: None,
            player: None,
            cameras: HashMap::new(),
        }
    }

    /// NOTE: This should mainly be used for errors that don't require backoff.
    fn set_role_error(&mut self, role: DeviceRole, error: String) {
        match role {
            DeviceRole::SerialInterface => {
                self.serial.last_error.get_or_insert(error);
            }
            DeviceRole::Camera(camera_id) => {
                self.cameras
                    .entry(camera_id)
                    .or_default()
                    .last_error
                    .get_or_insert(error);
            }
        }
    }

    // fn set_
}

struct DeviceEntry {
    device: AvailableDevice,
    used_by_machine_id: Option<u64>,
}

#[derive(Default)]
struct SerialEntry {
    device: Option<AvailableDevice>,

    controller: Option<Arc<SerialController>>,

    // TODO: Need better propagation of this to the UI. There may be multiple errors if there is a
    // camera and serial device on one machine.
    last_error: Option<String>,

    /// If set, connecting to the machine errored out so
    ///
    /// TODO: Need a gneeral backoff that limits max connect attempt rate (e.g.
    /// if machines fail very fast).
    start_after: Option<Instant>,

    watcher_task: Option<ChildTask>,

    /// The user has explicitly requested we connect to this machine
    /// - Only allowed to be true when controller.is_none() && device.is_some()
    connect_requested: bool,

    /// - Only allowed to be true when controller.is_some()
    disconnect_requested: bool,

    /// If true, we have issues a cancellation on the 'controller' resource.
    /// The disconnect will be complete once the 'watcher_task'
    shutting_down: bool,
}

struct PlayerEntry {
    player: Arc<Player>,
}

#[derive(Default)]
struct CameraEntry {
    /// Most recent device used to
    device: Option<AvailableDevice>,

    /// NOTE: If there is a controller, then there must be a 'device'.
    controller: Option<Arc<CameraController>>,

    // TODO: Implement me.
    start_after: Option<Instant>,

    // TODO: Need to expose in the UI
    last_error: Option<String>,

    device_error: Option<String>,

    /// Always present when controller.is_some()
    watcher_task: Option<ChildTask>,

    /// If true, the current controller is being shut down as it needs to be
    /// replaced with a newer device.
    ///
    /// - May only be true if controller.is_some()
    /// - Cleared when the watcher_task
    shutting_down: bool,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
enum DeviceRole {
    SerialInterface,
    Camera(u64),
}

impl MonitorImpl {
    // TODO: Eliminate possibly slow init logic like this that blocks the rest of
    // main() to run.
    pub async fn create(local_data_dir: &LocalPath) -> Result<Self> {
        let changes = ChangeDistributer::create();

        let db = Arc::new(ProtobufDB::create(&local_data_dir.join("db")).await?);

        let mut state = State::default();

        let mut config_presets = get_machine_presets()?;
        for i in 0..config_presets.len() {
            let mut fake_config = config_presets[i].clone();
            fake_config.set_base_config(format!("{}_fake", fake_config.base_config()));
            fake_config.clear_device();
            fake_config.device_mut().set_fake((i + 1) as u32);
            config_presets.push(fake_config);
        }

        let machines = db.list(&MACHINE_TABLE_TAG).await?;
        for machine in machines {
            let preset = config_presets
                .iter()
                .find(|c| c.base_config() == machine.config().base_config())
                .ok_or_else(|| {
                    format_err!("Missing preset named: {}", machine.config().base_config())
                })?;

            let config = MachineConfigContainer::create(machine.config().clone(), preset)?;

            // TODO: Hide information from the user about these machines until after the run
            // loop updates the presence state?
            state
                .machines
                .insert(machine.id(), MachineEntry::new(machine.id(), config));
        }

        let files = FileManager::create(
            &local_data_dir.join("files"),
            db.clone(),
            changes.publisher(),
        )
        .await?;

        let (reconcile_sender, reconcile_receiver) = channel::bounded(1);

        let shared = Arc::new(Shared {
            local_data_dir: local_data_dir.to_owned(),
            changes,
            config_presets,
            state: AsyncMutex::new(state),
            db,
            files,
            force_reconcile: reconcile_sender,
            camera_manager: Arc::new(CameraManager::default()),
        });

        let task_resource = TaskResource::spawn_interruptable(
            "MonitorImpl::run",
            Self::run(shared.clone(), reconcile_receiver),
        );

        Ok(Self {
            shared,
            task_resource,
        })
    }

    pub fn files(&self) -> &FileManager {
        &self.shared.files
    }

    /// Main loop that periodically reacts to hardware connect/disconnect events
    /// to instantiate all desired machines.
    async fn run(shared: Arc<Shared>, reconcile_receiver: channel::Receiver<()>) -> Result<()> {
        // The main loop has the job of periodically ensuring that we assign

        let usb_context = usb::Context::create()?;

        // TODO: Pass in a cancellation token for this part.

        loop {
            let made_new_devices = match Self::run_once(&shared, &usb_context).await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Device sync loop failed: {}", e);
                    // TODO: exponential backoff.
                    executor::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };

            // TODO: Adjust this based on the backoff time and also respond faster if we
            // detect hot plugging of devices.
            // If a new machine is created, we can immediately allocate devices to it.
            if !made_new_devices {
                // Publish broadcast events since something has probably changed and we don't
                // track individual changes well.
                shared.changes.publisher().publish(ChangeEvent::new(
                    EntityType::DEVICE,
                    None,
                    false,
                ));
                shared.changes.publisher().publish(ChangeEvent::new(
                    EntityType::MACHINE,
                    None,
                    false,
                ));

                executor::timeout(Duration::from_secs(5), reconcile_receiver.recv()).await;
            }
        }
    }

    async fn run_once(shared: &Arc<Shared>, usb_context: &usb::Context) -> Result<bool> {
        let devices = AvailableDevice::list_all(&usb_context).await?;

        let mut device_usage: HashMap<usize, MachineId> = HashMap::new();

        lock_async!(state <= shared.state.lock().await?, {
            // Try to assign all available devices to existing machine instances.
            // - role_to_device will contain every possible key and possibly empty vecs for
            //   keys if no device matches to it.
            let mut role_to_device = HashMap::<(MachineId, DeviceRole), Vec<usize>>::new();
            let mut device_to_role = HashMap::<usize, Vec<(MachineId, DeviceRole)>>::new();
            for (machine_id, machine) in &state.machines {
                let config = machine.config.read().await?;

                let serial_role = (*machine_id, DeviceRole::SerialInterface);
                let serial_devices = role_to_device.entry(serial_role).or_default();

                if config.has_device() {
                    for (i, dev) in devices.iter().enumerate() {
                        if dev.matches(config.device()) {
                            serial_devices.push(i);
                            device_to_role.entry(i).or_default().push(serial_role);
                        }
                    }
                }

                for camera_config in config.cameras() {
                    let camera_role = (*machine_id, DeviceRole::Camera(camera_config.id()));
                    let camera_devices = role_to_device.entry(camera_role).or_default();

                    if !camera_config.has_device() {
                        continue;
                    }

                    for (i, dev) in devices.iter().enumerate() {
                        if dev.matches(camera_config.device()) {
                            camera_devices.push(i);
                            device_to_role.entry(i).or_default().push(camera_role);
                        }
                    }
                }

                // Insert empty entries for instantitated but unconfigured cameras.
                // TODO: Clean up any camera entries that are dead and don't have a config.
                for camera_id in machine.cameras.keys() {
                    let camera_role = (*machine_id, DeviceRole::Camera(*camera_id));
                    role_to_device.entry(camera_role).or_default();
                }
            }

            // Apply the device changes.
            for ((machine_id, role), device_index) in &role_to_device {
                let machine = state.machines.get_mut(machine_id).unwrap();

                // Verify we made an unambiguous device assignment (part 1)
                if device_index.len() > 1 {
                    machine.set_role_error(
                        *role,
                        format!(
                            "Multiple devices satisfy the role of {:?} for machine {}",
                            *role, *machine_id
                        ),
                    );
                    continue;
                }

                let device = {
                    if device_index.len() == 0 {
                        None
                    } else {
                        let device_index = device_index[0];
                        let device = &devices[device_index];

                        // Verify we made an unambiguous device assignment (part 2)
                        {
                            let roles = device_to_role.get(&device_index).unwrap();
                            if roles.len() > 1 {
                                // TODO: There may be multiple errors for one machine if we count
                                // both camera and connection roles.
                                machine.set_role_error(
                                    *role,
                                    format!(
                                        "{} satifies roles for multiple machines.",
                                        device.label()
                                    ),
                                );
                                continue;
                            }
                        }

                        device_usage.insert(device_index, *machine_id);

                        Some(device)
                    }
                };

                // Apply the effects.
                match *role {
                    DeviceRole::SerialInterface => {
                        if let Err(e) = Self::open_serial_controller(&shared, device, machine).await
                        {
                            eprintln!("Serial Open Error: {}", e);
                            machine.serial.start_after = Some(Instant::now() + RETRY_BACKOFF);
                            machine.serial.last_error = Some(e.to_string());
                        }
                    }
                    DeviceRole::Camera(camera_id) => {
                        Self::open_camera_controller(shared, device, camera_id, machine).await?;
                    }
                }
            }

            // Try to use any unclaimed devices for instantiating new machines from presets.
            //
            // TODO: Don't block the state while this is running
            let mut made_new_devices = false;
            for (i, dev) in devices.iter().enumerate() {
                if device_to_role.contains_key(&i) {
                    continue;
                }

                for preset in &shared.config_presets {
                    if !preset.has_device() {
                        continue;
                    }

                    if dev.matches(preset.device()) {
                        let mut diff = MachineConfig::default();
                        diff.set_base_config(preset.base_config());
                        diff.set_device(dev.stable_selector());

                        let config = MachineConfigContainer::create(diff.clone(), preset)?;

                        if state.machines.len() >= MAX_NUM_MACHINES {
                            eprintln!("Too many machines conencted to allocate more");
                            continue;
                        }

                        let id = crypto::random::global_rng().uniform::<MachineId>().await;

                        device_usage.insert(i, id);

                        eprintln!(
                            "Creating new machine with id {} from preset {}",
                            id,
                            config.base_config()
                        );

                        {
                            let mut machine_proto = MachineProto::default();
                            machine_proto.set_id(id);
                            machine_proto.set_config(diff.clone());
                            shared.db.insert(&MACHINE_TABLE_TAG, &machine_proto).await?;
                        }

                        state.machines.insert(id, MachineEntry::new(id, config));
                        made_new_devices = true;
                    }
                }
            }

            // Save the whole list of devices so that clients can inspect this.
            state.all_devices = devices
                .into_iter()
                .enumerate()
                .map(|(i, device)| DeviceEntry {
                    device,
                    used_by_machine_id: device_usage.get(&i).copied(),
                })
                .collect();

            // TODO: Report events.

            // TODO: Need a self test for cameras so that we know that they are behaving
            // prior to us hitting play.

            // What we should do all the time is record event logs to a database.
            // - Ideally have a full traceable play/pause/connect/etc. history.

            Ok(made_new_devices)
        })
    }

    // TODO: Make this function fast and not blocking on any I/O.
    async fn open_serial_controller(
        shared: &Arc<Shared>,
        device: Option<&AvailableDevice>,
        machine: &mut MachineEntry,
    ) -> Result<()> {
        // TODO: If we don't have auto_connect enabled, should we do any
        // auto-disconnects.

        if let Some(old_device) = &machine.serial.device {
            let changed = device.is_none() || old_device.path() != device.unwrap().path();

            let want_shutdown = changed || machine.serial.disconnect_requested;
            machine.serial.disconnect_requested = false;

            if want_shutdown && !machine.serial.shutting_down {
                if let Some(controller) = &machine.serial.controller {
                    controller
                        .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                        .await;
                    machine.serial.shutting_down = true;
                }
            }

            // Wait for controller to finish shutting down before changing the device.
            if changed && machine.serial.controller.is_some() {
                return Ok(());
            }
        }

        machine.serial.device = device.cloned();

        // Maybe connect

        let connect_requested = machine.serial.connect_requested;
        machine.serial.connect_requested = false;

        let device = match device {
            Some(v) => v,
            None => return Ok(()),
        };

        if machine.serial.controller.is_some() {
            machine.serial.last_error = None;
            return Ok(());
        }

        let auto_connect = machine.config.read().await?.auto_connect();

        let after_start_after = match machine.serial.start_after {
            Some(v) => Instant::now() > v,
            None => true,
        };

        let should_connect = connect_requested || (auto_connect && after_start_after);
        if !should_connect {
            return Ok(());
        }

        machine.serial.start_after = None;

        let (reader, writer) = device
            .open_as_serial_port(machine.config.read().await?.value().baud_rate() as usize)
            .await?;

        let controller = Arc::new(
            SerialController::create(
                machine.id,
                machine.config.clone(),
                reader,
                writer,
                shared.changes.publisher(),
            )
            .await?,
        );

        machine.serial.watcher_task = Some(ChildTask::spawn(Self::watch_serial_port(
            Arc::downgrade(shared),
            machine.id,
            controller.clone(),
        )));

        machine.serial.controller = Some(controller);
        machine.serial.last_error = None;

        Ok(())
    }

    // TODO: Consider moving most of part of this and the retry loop for the
    // connection into the SerialController class (will require us to be able to
    // fully re-index driver paths based on one sysfs path).
    async fn watch_serial_port(
        shared: Weak<Shared>,
        machine_id: u64,
        controller: Arc<SerialController>,
    ) {
        // NOTE: If there was no error, then we assume there was a successful disconnect
        // requested by a user.
        let error = controller
            .wait_for_termination()
            .await
            .map_err(|e| e.to_string())
            .err();

        drop(controller);

        // TODO: Check that a disconnect was requested if we got no error.

        // TODO: Record a 'disconnect' event.

        let shared = match shared.upgrade() {
            Some(v) => v,
            None => return,
        };

        let state = match shared.state.lock().await {
            Ok(v) => v,
            Err(_) => return,
        };

        lock!(state <= state, {
            let entry = match state.machines.get_mut(&machine_id) {
                Some(v) => v,
                None => return,
            };

            entry.serial.controller.take();
            entry.serial.disconnect_requested = false;
            entry.serial.shutting_down = false;

            if let Some(error) = error {
                eprintln!("Seiral Controller Failure: {}", error);
                entry.serial.start_after = Some(Instant::now() + RETRY_BACKOFF);
                entry.serial.last_error = Some(error.to_string());
            } else {
                entry.serial.last_error = None;
            }
        });

        // May want to reconnect after the disconnect succedded.
        let _ = shared.force_reconcile.try_send(());

        shared.changes.publisher().publish(ChangeEvent::new(
            EntityType::MACHINE,
            Some(machine_id),
            false,
        ));
    }

    /// NOTE: This is meant to be a fast running function that is unlikely to
    /// fail.
    async fn open_camera_controller(
        shared: &Arc<Shared>,
        device: Option<&AvailableDevice>,
        camera_id: u64,
        machine: &mut MachineEntry,
    ) -> Result<()> {
        // TODO: Eventually clean up all unused camera entries.

        let camera_entry = machine
            .cameras
            .entry(camera_id)
            .or_insert_with(|| CameraEntry::default());

        if let Some(old_device) = &camera_entry.device {
            let changed = device.is_none() || old_device.path() != device.unwrap().path();

            if changed && !camera_entry.shutting_down {
                if let Some(controller) = &camera_entry.controller {
                    controller
                        .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                        .await;
                    camera_entry.shutting_down = true;
                }
            }

            // Must wait for the old controller to be cleaned up before we can
            // switch to a new device.
            if changed && camera_entry.controller.is_some() {
                return Ok(());
            }
        }

        camera_entry.device = device.cloned();

        // If we have both a device and no existing controller, create a new
        // controller.

        let device = match device {
            Some(v) => v,
            None => return Ok(()),
        };

        if camera_entry.controller.is_some() {
            camera_entry.last_error = None;
            return Ok(());
        }

        if let Some(start_after) = camera_entry.start_after {
            if Instant::now() <= start_after {
                return Ok(());
            }
        }
        camera_entry.start_after = None;

        let controller = Arc::new(CameraController::create(
            machine.id,
            camera_id,
            shared.camera_manager.clone(),
            device.clone(),
            machine.config.clone(),
            shared.local_data_dir.join("camera"),
            shared.db.clone(),
        ));

        if let Some(player) = &machine.player {
            controller
                .set_current_player(Some(player.player.clone()))
                .await?;
        }

        camera_entry.watcher_task = Some(ChildTask::spawn(Self::watch_camera_controller(
            Arc::downgrade(&shared),
            machine.id,
            camera_id,
            controller.clone(),
        )));

        camera_entry.controller = Some(controller);
        camera_entry.last_error = None;

        Ok(())
    }

    async fn watch_camera_controller(
        shared: Weak<Shared>,
        machine_id: u64,
        camera_id: u64,
        controller: Arc<CameraController>,
    ) {
        // Wait for it to terminate.

        let res = controller.wait_for_termination().await;
        drop(controller);

        let shared = match shared.upgrade() {
            Some(v) => v,
            None => {
                return;
            }
        };

        let state = match shared.state.lock().await {
            Ok(v) => v,
            Err(_) => return,
        };

        lock!(state <= state, {
            let machine_entry = match state.machines.get_mut(&machine_id) {
                Some(v) => v,
                None => return,
            };

            let camera_entry = match machine_entry.cameras.get_mut(&camera_id) {
                Some(v) => v,
                None => return,
            };

            if let Err(e) = res {
                eprintln!("Camera controller failed: {}", e);
                camera_entry.last_error = Some(e.to_string());
                camera_entry.start_after = Some(Instant::now() + RETRY_BACKOFF);
            } else {
                camera_entry.last_error = None;
            }

            camera_entry.controller = None;
            camera_entry.shutting_down = false;
        });

        // May want to immediately reconnect.
        let _ = shared.force_reconcile.try_send(());
    }

    async fn query_entities_impl(
        &self,
        request: rpc::ServerRequest<QueryEntitiesRequest>,
        response: &mut rpc::ServerStreamResponse<'_, QueryEntitiesResponse>,
    ) -> Result<()> {
        let filter = ChangeEvent {
            entity_type: request.value.entity_type(),
            id: {
                if request.value.has_entity_id() {
                    Some(request.value.entity_id())
                } else {
                    None
                }
            },
            verbose: request.value.verbose(),
        };

        let mut subscriber = self.shared.changes.subscribe(filter.clone());

        // TODO: Throttle this loop
        loop {
            response
                .send(self.query_entities_current_value(&filter).await?)
                .await?;

            if !request.value.watch() {
                break;
            }

            executor::timeout(Duration::from_secs(10), subscriber.wait()).await;
        }

        Ok(())
    }

    async fn query_entities_current_value(
        &self,
        filter: &ChangeEvent,
    ) -> Result<QueryEntitiesResponse> {
        let mut out = QueryEntitiesResponse::default();

        match filter.entity_type {
            EntityType::MACHINE => {
                self.list_machines_impl(filter.id, &mut out).await?;
            }
            EntityType::FILE => self.shared.files.query_files(filter.id, &mut out)?,
            EntityType::DEVICE => {
                lock!(state <= self.shared.state.lock().await?, {
                    for device in &state.all_devices {
                        let proto = out.new_devices();
                        proto.set_info(device.device.verbose_proto());
                        if let Some(id) = &device.used_by_machine_id {
                            proto.set_used_by_machine_id(*id);
                        }
                    }
                })
            }
            EntityType::PRESET => {
                for config in &self.shared.config_presets {
                    out.add_presets(config.clone());
                }
            }
            _ => {
                // TODO
            }
        }

        Ok(out)
    }

    async fn list_machines_impl(
        &self,
        id: Option<u64>,
        out: &mut QueryEntitiesResponse,
    ) -> Result<()> {
        let state = self.shared.state.lock().await?.read_exclusive();

        for (machine_id, machine) in &state.machines {
            // TODO: Seek directly to the right extra if this is set.
            if let Some(id) = id {
                if id != *machine_id {
                    continue;
                }
            }

            let machine_config = machine.config.read().await?.value().clone();

            let proto = out.new_machines();
            proto.set_id(*machine_id);
            proto.set_config(machine_config.clone());

            let state_proto = proto.state_mut();

            if let Some(device) = &machine.serial.device {
                state_proto.set_connection_device(device.verbose_proto());
            }

            if let Some(iface) = &machine.serial.controller {
                // In this case, we are in a CONNECTING | CONNECTED state.
                iface.state_proto(state_proto).await?;
            } else {
                if machine.serial.start_after.is_some() {
                    // TODO: Is this correct if auto_connect is disabled?
                    state_proto.set_connection_state(MachineStateProto_ConnectionState::ERROR);
                } else if machine.serial.device.is_some() {
                    state_proto
                        .set_connection_state(MachineStateProto_ConnectionState::DISCONNECTED);
                } else {
                    state_proto.set_connection_state(MachineStateProto_ConnectionState::MISSING);
                }
            }

            if let Some(e) = machine.serial.last_error.clone() {
                state_proto.set_last_connection_error(e);
            }

            if let Some(file_ref) = &machine.loaded_file {
                state_proto
                    .loaded_program_mut()
                    .set_file(file_ref.proto().clone());
            }

            if let Some(player_entry) = &machine.player {
                state_proto.set_running_program(player_entry.player.state_proto().await?);
            } else {
                // TODO: Mark as STOPPED and put in an estimated_time_remaining
                // based on the file's duration (in the appropriate mode).
            }

            for camera_config in machine_config.cameras() {
                let camera_id = camera_config.id();

                let camera_proto = state_proto.new_cameras();
                camera_proto.set_camera_id(camera_id);

                let camera = match machine.cameras.get(&camera_id) {
                    Some(v) => v,
                    None => {
                        camera_proto.set_status(CameraState_State::MISSING);
                        continue;
                    }
                };

                if let Some(device) = &camera.device {
                    camera_proto.set_device(device.verbose_proto());
                }

                if let Some(error) = &camera.last_error {
                    camera_proto.set_last_error(error.clone());
                }

                if camera.shutting_down {
                    camera_proto.set_status(CameraState_State::SETUP);
                } else {
                    if let Some(controller) = &camera.controller {
                        // TODO: Also implement the STARTING and SETUP states for this.

                        if controller.recording().await? {
                            camera_proto.set_status(CameraState_State::RECORDING);
                        } else {
                            camera_proto.set_status(CameraState_State::IDLE);
                        }
                    } else if camera.start_after.is_some() {
                        camera_proto.set_status(CameraState_State::ERROR);
                    } else if camera.device.is_some() {
                        camera_proto.set_status(CameraState_State::SETUP);
                    } else {
                        camera_proto.set_status(CameraState_State::MISSING);
                    }
                }
            }
        }

        Ok(())
    }

    async fn read_serial_log_impl(
        &self,
        request: &ReadSerialLogRequest,
        response: &mut rpc::ServerStreamResponse<'_, ReadSerialLogResponse>,
    ) -> Result<()> {
        // TODO: This must not get any locks on the machine.
        let serial_controller = self.acquire_machine_control(request.machine_id()).await?;

        serial_controller.read_serial_log(response).await?;

        Ok(())
    }

    async fn start_file_upload_impl(
        &self,
        request: &StartFileUploadRequest,
    ) -> Result<StartFileUploadResponse> {
        let mut res = StartFileUploadResponse::default();
        res.set_file(
            self.shared
                .files
                .start_file_upload(request.name(), request.size() as u64)
                .await?,
        );
        Ok(res)
    }

    async fn delete_file_impl(&self, request: &DeleteFileRequest) -> Result<()> {
        // TODO: Try to remove all loaded locks if possible

        self.shared.files.delete_file(request.file_id()).await?;
        Ok(())
    }

    // TODO: Ideally this would send back some revision metadata so that any
    // QueryEntitites requests from the client can wait for the result of the
    // command to propagate.
    async fn run_machine_command_impl(&self, request: &RunMachineCommandRequest) -> Result<()> {
        match request.command_case() {
            RunMachineCommandRequestCommandCase::NOT_SET => {
                return Err(rpc::Status::invalid_argument("Unknown command requested").into());
            }
            RunMachineCommandRequestCommandCase::Connect(_) => {
                // Get the machine entry and request a

                lock!(state <= self.shared.state.lock().await?, {
                    let entry = state
                        .machines
                        .get_mut(&request.machine_id())
                        .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                    if entry.serial.controller.is_some() {
                        return Err(
                            rpc::Status::failed_precondition("Machine already connected.").into(),
                        );
                    }

                    if entry.serial.device.is_none() {
                        return Err(rpc::Status::failed_precondition(
                            "Machine has no device attached for connecting.",
                        )
                        .into());
                    }

                    entry.serial.connect_requested = true;
                    entry.serial.disconnect_requested = false;

                    Ok::<_, Error>(())
                })?;

                let _ = self.shared.force_reconcile.try_send(());
            }
            RunMachineCommandRequestCommandCase::Disconnect(_) => {
                lock!(state <= self.shared.state.lock().await?, {
                    let entry = state
                        .machines
                        .get_mut(&request.machine_id())
                        .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                    if entry.serial.controller.is_none() {
                        return Err(
                            rpc::Status::failed_precondition("Machine is not connected.").into(),
                        );
                    }

                    if entry.serial.device.is_none() {
                        return Err(rpc::Status::failed_precondition(
                            "Machine has no device attached for connecting.",
                        )
                        .into());
                    }

                    entry.serial.connect_requested = false;
                    entry.serial.disconnect_requested = true;

                    Ok::<_, Error>(())
                })?;

                let _ = self.shared.force_reconcile.try_send(());
            }
            RunMachineCommandRequestCommandCase::EmergencyStop(_) => todo!(),
            RunMachineCommandRequestCommandCase::SendSerialCommand(cmd) => {
                // TODO: While we are sending commands, we should disable the player to be
                // created.

                let serial_controller = self.acquire_machine_control(request.machine_id()).await?;

                let cmd = cmd.replace("\n", " ").replace("\r", " ");

                serial_controller
                    .send_command(format!("{}\n", cmd), Duration::from_secs(10))
                    .await?;
            }
            RunMachineCommandRequestCommandCase::PlayProgram(_) => {
                self.play_impl(request.machine_id()).await?;
            }
            RunMachineCommandRequestCommandCase::PauseProgram(_) => {
                self.pause_impl(request.machine_id()).await?;
            }
            RunMachineCommandRequestCommandCase::StopProgram(_) => {
                self.stop_impl(request.machine_id()).await?;
            }

            RunMachineCommandRequestCommandCase::LoadProgram(cmd) => {
                let file_ref = self.shared.files.lookup(cmd.file_id())?;

                // TODO: Verify there were no errors while processing the file (also need to
                // handle machine gcode compatibility )

                lock!(state <= self.shared.state.lock().await?, {
                    let entry = state
                        .machines
                        .get_mut(&request.machine_id())
                        .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                    if let Some(player) = &entry.player {
                        if !player.player.terminated() {
                            return Err(rpc::Status::failed_precondition(
                                "Machine still has an active player instance.",
                            )
                            .into());
                        }

                        entry.player = None;
                    }

                    entry.loaded_file = Some(file_ref);

                    Ok::<_, Error>(())
                })?;

                self.shared.changes.publisher().publish(ChangeEvent::new(
                    EntityType::MACHINE,
                    Some(request.machine_id()),
                    true,
                ));
            }
            RunMachineCommandRequestCommandCase::UnloadProgram(_) => {
                lock!(state <= self.shared.state.lock().await?, {
                    let entry = state
                        .machines
                        .get_mut(&request.machine_id())
                        .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                    if entry.loaded_file.is_none() {
                        return Err(rpc::Status::failed_precondition(
                            "No file currently loaded on the machine",
                        )
                        .into());
                    }

                    if let Some(player) = &entry.player {
                        if !player.player.terminated() {
                            return Err(rpc::Status::failed_precondition(
                                "Machine still has an active player instance.",
                            )
                            .into());
                        }

                        entry.player = None;
                    }

                    entry.loaded_file = None;
                    Ok::<_, Error>(())
                })?;

                self.shared.changes.publisher().publish(ChangeEvent::new(
                    EntityType::MACHINE,
                    Some(request.machine_id()),
                    true,
                ));
            }
            RunMachineCommandRequestCommandCase::UpdateConfig(new_config) => {
                // TODO: Make this cancel safe.

                let state = self.shared.state.lock().await?.read_exclusive();
                let entry = state
                    .machines
                    .get(&request.machine_id())
                    .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                // TODO: Don't update if the merge fails?
                let diff = lock!(config <= entry.config.write().await?, {
                    config.merge_from(new_config)?;
                    Ok::<_, Error>(config.diff().clone())
                })?;

                // NOTE: We are still holding an exclusive lock on 'state' while this happens.
                {
                    let mut machine_proto = MachineProto::default();
                    machine_proto.set_id(request.machine_id());
                    machine_proto.set_config(diff);
                    self.shared
                        .db
                        .insert(&MACHINE_TABLE_TAG, &machine_proto)
                        .await?;
                }

                // TODO: Must save the change to the db.

                // May trigger things like camera connect/disconnects.
                let _ = self.shared.force_reconcile.try_send(());

                self.shared.changes.publisher().publish(ChangeEvent::new(
                    EntityType::MACHINE,
                    Some(request.machine_id()),
                    true,
                ));
            }

            RunMachineCommandRequestCommandCase::SetTemperature(cmd) => {
                let serial_controller = self.acquire_machine_control(request.machine_id()).await?;
                serial_controller
                    .set_temperature(cmd.axis_id(), cmd.target())
                    .await?;
                serial_controller.request_state_update().await?;
            }
            RunMachineCommandRequestCommandCase::HomeX(_) => {
                let serial_controller = self.acquire_machine_control(request.machine_id()).await?;
                serial_controller.home_x().await?;
            }
            RunMachineCommandRequestCommandCase::HomeY(_) => {
                let serial_controller = self.acquire_machine_control(request.machine_id()).await?;
                serial_controller.home_y().await?;
            }
            RunMachineCommandRequestCommandCase::ProbeZ(_) => todo!(),
            RunMachineCommandRequestCommandCase::MeshLevel(_) => todo!(),
            RunMachineCommandRequestCommandCase::Goto(cmd) => {
                let serial_controller = self.acquire_machine_control(request.machine_id()).await?;

                // Absolute positioning
                serial_controller
                    .send_command("G90\n", DEFAULT_COMMAND_TIMEOUT)
                    .await?;

                serial_controller
                    .send_command(
                        format!("G0 X{:.2} Y{:.2} F{}\n", cmd.x(), cmd.y(), cmd.feed_rate()),
                        DEFAULT_COMMAND_TIMEOUT,
                    )
                    .await?;
            }
            RunMachineCommandRequestCommandCase::Jog(cmd) => {
                let serial_controller = self.acquire_machine_control(request.machine_id()).await?;

                // Relative positioning
                serial_controller
                    .send_command("G91\n", DEFAULT_COMMAND_TIMEOUT)
                    .await?;

                let mut command = format!("G0 F{}", cmd.feed_rate());
                for increment in cmd.increment() {
                    // TODO: Validate the axis ids.
                    command.push_str(&format!(" {}{:.2}", increment.axis_id(), increment.value()));
                }

                command.push('\n');

                serial_controller
                    .send_command(command, DEFAULT_COMMAND_TIMEOUT)
                    .await?;
            }
        }

        Ok(())
    }

    async fn acquire_machine_control(&self, machine_id: u64) -> Result<Arc<SerialController>> {
        lock!(state <= self.shared.state.lock().await?, {
            let entry = state
                .machines
                .get(&machine_id)
                .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

            // TODO: Error out if a player is currently controlling the machine.

            let serial = entry.serial.controller.clone().ok_or_else(|| {
                rpc::Status::failed_precondition("Machine not currently connected")
            })?;

            Result::<_, Error>::Ok(serial)
        })
    }

    async fn play_impl(&self, machine_id: u64) -> Result<()> {
        executor::spawn(Self::play_impl_inner(self.shared.clone(), machine_id))
            .join()
            .await
    }

    /// NOT CANCEL SAFE
    async fn play_impl_inner(shared: Arc<Shared>, machine_id: u64) -> Result<()> {
        // TODO: Before we allow something like this to run, we should have some overall
        // status check (serial port opened, all camera controllers setup, etc.).

        lock_async!(state <= shared.state.lock().await?, {
            let entry = state
                .machines
                .get_mut(&machine_id)
                .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

            if let Some(player_entry) = &entry.player {
                if player_entry.player.terminated() {
                    // Allowed to re-start playing the same file once the player has terminated.
                    entry.player.take();
                } else {
                    return player_entry.player.play().await;
                }
            }

            let file_ref = entry.loaded_file.as_ref().ok_or_else(|| {
                rpc::Status::failed_precondition("No file loaded on the machine to play")
            })?;

            let serial_controller = entry
                .serial
                .controller
                .as_ref()
                .ok_or_else(|| rpc::Status::failed_precondition("Machine is not connected"))?;

            if !serial_controller.connected().await? {
                return Err(rpc::Status::failed_precondition(
                    "Machine connection is not ready yet",
                )
                .into());
            }

            let player = Arc::new(
                Player::create(
                    machine_id,
                    entry.config.clone(),
                    file_ref.clone(),
                    serial_controller.clone(),
                    shared.changes.publisher(),
                )
                .await?,
            );

            entry.player = Some(PlayerEntry {
                player: player.clone(),
            });

            // TODO: Don't lock the entire state while this is running.
            // TODO: Parallelize if there are multiple cameras.
            for camera in entry.cameras.values_mut() {
                if let Some(camera_controller) = &mut camera.controller {
                    camera_controller
                        .set_current_player(Some(player.clone()))
                        .await?;
                    camera_controller.pre_play().await?;
                }
            }

            player.play().await?;

            Ok::<_, Error>(())
        })?;

        shared.changes.publisher().publish(ChangeEvent::new(
            EntityType::MACHINE,
            Some(machine_id),
            false,
        ));

        Ok(())
    }

    async fn pause_impl(&self, machine_id: u64) -> Result<()> {
        let shared = self.shared.clone();
        executor::spawn(async move {
            lock_async!(state <= shared.state.lock().await?, {
                let entry = state
                    .machines
                    .get_mut(&machine_id)
                    .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                let player = entry.player.as_ref().ok_or_else(|| {
                    rpc::Status::failed_precondition("Machine not playing anything")
                })?;

                player.player.pause().await
            })
        })
        .join()
        .await
    }

    async fn stop_impl(&self, machine_id: u64) -> Result<()> {
        let shared = self.shared.clone();
        executor::spawn(async move {
            lock_async!(state <= shared.state.lock().await?, {
                let entry = state
                    .machines
                    .get_mut(&machine_id)
                    .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                let player = entry.player.as_ref().ok_or_else(|| {
                    rpc::Status::failed_precondition("Machine not playing anything")
                })?;

                player.player.stop().await
            })
        })
        .join()
        .await
    }

    /// TODO: If the camera attached to the machine at this id changes while
    /// this is running, we should cancel the request.
    pub async fn get_camera_feed(&self, machine_id: u64, camera_id: u64) -> Result<http::Response> {
        let device_entry = lock!(state <= self.shared.state.lock().await?, {
            let machine = state
                .machines
                .get(&machine_id)
                .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

            let camera = machine
                .cameras
                .get(&camera_id)
                .ok_or_else(|| rpc::Status::not_found("Camera not found"))?;

            let device = camera
                .device
                .as_ref()
                .ok_or_else(|| rpc::Status::not_found("Camera not connected"))?;

            Ok::<_, Error>(device.clone())
        })?;

        let subscriber = device_entry
            .open_as_camera(&self.shared.camera_manager)
            .await?;

        media_web::camera_stream::respond_with_camera_stream(subscriber).await
    }
}

#[async_trait]
impl MonitorService for MonitorImpl {
    async fn QueryEntities(
        &self,
        request: rpc::ServerRequest<QueryEntitiesRequest>,
        response: &mut rpc::ServerStreamResponse<QueryEntitiesResponse>,
    ) -> Result<()> {
        self.query_entities_impl(request, response).await
    }

    async fn RunMachineCommand(
        &self,
        request: rpc::ServerRequest<RunMachineCommandRequest>,
        response: &mut rpc::ServerResponse<RunMachineCommandResponse>,
    ) -> Result<()> {
        self.run_machine_command_impl(&request.value).await?;
        Ok(())
    }

    async fn ReadSerialLog(
        &self,
        request: rpc::ServerRequest<ReadSerialLogRequest>,
        response: &mut rpc::ServerStreamResponse<ReadSerialLogResponse>,
    ) -> Result<()> {
        self.read_serial_log_impl(&request.value, response).await?;
        Ok(())
    }

    async fn StartFileUpload(
        &self,
        request: rpc::ServerRequest<StartFileUploadRequest>,
        response: &mut rpc::ServerResponse<StartFileUploadResponse>,
    ) -> Result<()> {
        response.value = self.start_file_upload_impl(&request.value).await?;
        Ok(())
    }

    async fn DeleteFile(
        &self,
        request: rpc::ServerRequest<DeleteFileRequest>,
        response: &mut rpc::ServerResponse<DeleteFileResponse>,
    ) -> Result<()> {
        self.delete_file_impl(&request.value).await?;
        Ok(())
    }
}
