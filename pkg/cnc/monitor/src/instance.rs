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
use protobuf::Message;

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

const RECONNECT_BACKOFF: Duration = Duration::from_secs(10);

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
    changes: ChangeDistributer,
    db: Arc<ProtobufDB>,
    files: FileManager,
    state: AsyncMutex<State>,
    force_reconcile: channel::Sender<()>,
}

#[derive(Default)]
struct State {
    // Machines indexed by id.
    machines: HashMap<MachineId, MachineEntry>,

    files: HashMap<FileId, FileProto>,
}

struct MachineEntry {
    id: u64,

    config: Arc<AsyncRwLock<MachineConfigContainer>>,

    /// If set, then this machine (as represented by its serial interface) has
    /// been detected as attached to the current machine so can be connected
    /// to.
    present: Option<DeviceSelector>,

    // TODO: Dynamically add these to the resource group.
    /// If not None,
    serial: Option<OpenedSerialInterface>,

    // TODO: Need better propagation of this to the UI. There may be multiple errors if there is a
    // camera and serial device on one machine.
    last_error: Option<String>,

    /*
    - Loaded file.
    - Mesh leveling grid (when external to the machine)
    */
    /// If set, connecting to the machine errored out so
    ///
    /// TODO: Need a gneeral backoff that limits max connect attempt rate (e.g.
    /// if machines fail very fast).
    start_after: Option<Instant>,

    loaded_file: Option<FileReference>,

    player: Option<PlayerEntry>,

    /// The user has explicitly requested we connect to this machine
    connect_requested: bool,

    disconnect_requested: bool,
}

impl MachineEntry {
    fn new(id: u64, config: MachineConfigContainer) -> Self {
        Self {
            id,
            config: Arc::new(AsyncRwLock::new(config)),
            present: None,
            serial: None,
            last_error: None,
            start_after: None,
            loaded_file: None,
            player: None,
            connect_requested: false,
            disconnect_requested: false,
        }
    }

    fn set_last_error(&mut self, error: String) {
        eprintln!("Machine Error: {}", error);
        self.last_error = Some(error);

        // TODO: Do some backoff.
    }
}

struct OpenedSerialInterface {
    controller: Arc<SerialController>,

    /// sysfs path to the USB device used for the serial_interface.
    device_path: LocalPathBuf,

    device_info: DeviceSelector,

    watcher_task: ChildTask,

    /// If true, we have issues a cancellation on the 'controller' resource.
    /// The disconnect will be complete once the 'watcher_task'
    disconnect_requested: bool,
}

struct PlayerEntry {
    player: Arc<Player>,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
enum DeviceRole {
    SerialInterface,
    Camera,
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
            state: AsyncMutex::new(state),
            db,
            files,
            force_reconcile: reconcile_sender,
        });

        let task_resource = TaskResource::spawn_interruptable(
            "MonitorImpl::run",
            Self::run(shared.clone(), reconcile_receiver, config_presets),
        );

        Ok(Self {
            shared,
            task_resource,
        })
    }

    pub fn files(&self) -> &FileManager {
        &self.shared.files
    }

    // TODO: If this thread fails, it shouldn't take down all existing machines.
    async fn run(
        shared: Arc<Shared>,
        reconcile_receiver: channel::Receiver<()>,
        config_presets: Vec<MachineConfig>,
    ) -> Result<()> {
        // The main loop has the job of periodically ensuring that we assign

        let usb_context = usb::Context::create()?;

        // TODO: Also need a concept of top level error messages that we can broadcast
        // to the user in the web UI.

        // TODO: Pass in a cancellation token for this part.
        loop {
            let devices = AvailableDevice::list_all(&usb_context).await?;

            let mut state = shared.state.lock().await?.enter();

            /*
            Two important invariants:
            - For all instantiated machines, no one device can match multiple of them.
                - Also multiple devices can't satisfy

            - For all presets, no one device can match to multiple of them.

            Two maps:
            (device_path -> Vec<(MachineId, Role)>)

            (MachineId, Role) -> Vec<device_path>

            */

            // Handle all disconnect requests.
            for machine in state.machines.values_mut() {
                if let Some(serial) = &mut machine.serial {
                    if machine.disconnect_requested && !serial.disconnect_requested {
                        serial
                            .controller
                            .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                            .await;
                        serial.disconnect_requested = true;
                    }
                    machine.disconnect_requested = false;
                }
            }

            // Try to assign all available devices to existing machine instances.
            let mut role_to_device = HashMap::<(MachineId, DeviceRole), Vec<usize>>::new();
            let mut device_to_role = HashMap::<usize, Vec<(MachineId, DeviceRole)>>::new();
            for (machine_id, machine) in &state.machines {
                let config = machine.config.read().await?;

                for (i, dev) in devices.iter().enumerate() {
                    if !config.has_device() {
                        continue;
                    }

                    if dev.matches(config.device()) {
                        let role = (*machine_id, DeviceRole::SerialInterface);
                        role_to_device.entry(role).or_default().push(i);
                        device_to_role.entry(i).or_default().push(role);
                    }

                    // TODO: Also implement config.serial_path()

                    // TODO: Also add cameras.
                }
            }

            // Reset presence (will be set in the next loop)
            for machine in state.machines.values_mut() {
                machine.present = None;
            }

            // Apply the device changes.
            for ((machine_id, role), device_index) in &role_to_device {
                let machine = state.machines.get_mut(machine_id).unwrap();

                // TODO: Throttle based on retry backoff. (but still want to preserve any
                // presence information, assuming we didn't fail because we couldn't match the
                // right device).

                // Verify we made an unambiguous device assignment (part 1)
                if device_index.len() > 1 {
                    machine.set_last_error(format!(
                        "Multiple devices satisfy the role of {:?} for machine {}",
                        *role, *machine_id
                    ));
                    continue;
                }

                let device_index = device_index[0];
                let device = &devices[device_index];

                // Verify we made an unambiguous device assignment (part 2)
                {
                    let roles = device_to_role.get(&device_index).unwrap();
                    if roles.len() > 1 {
                        // TODO: There may be multiple errors for one machine if we count both
                        // camera and connection roles.
                        machine.set_last_error(format!(
                            "{} satifies roles for multiple machines.",
                            device.label()
                        ));
                        continue;
                    }
                }

                // Apply the effects.
                match *role {
                    DeviceRole::SerialInterface => {
                        if let Err(e) = Self::open_serial_interface(&shared, device, machine).await
                        {
                            machine.set_last_error(e.to_string());
                        }
                    }
                    DeviceRole::Camera => {
                        //
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

                for preset in &config_presets {
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

                        // TODO:
                        // device_to_role.insert(k, v)

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

            // TODO: Improve this.
            state.exit();

            // TODO: Any unassigned USB devices may be useable as cameras or serial ports
            // for generic presets.

            // TODO: Report events.

            // TODO: Need a self test for cameras so that we know that they are behaving
            // prior to us hitting play.

            /*
            Also some concept of intent:
            - If serial device changes, we need a new machine
            - If the camera device changes, we need to make a new device.
            */

            // Try to open any closed

            // Go through all existing machines and mark device claims.
            // - Need some warnings if there are multiple possibilites for one machine or
            //   multiple match one.

            // Go through all existing devices and maybe restart them
            // - Also update 'present'
            // - Maybe kill 'machine' if not present for a while.

            // Go through the presets.
            // - Try to instantiate new machines for them.

            // Things to do

            // All configs matching

            // Check if any

            // Note that for existing machines,

            // What we should do all the time is record event logs to a database.
            // - Ideally have a full traceable play/pause/connect/etc. history.

            // TODO: Adjust this based on the backoff time and also respond faster if we
            // detect hot plugging of devices.
            // If a new machine is created, we can immediately allocate devices to it.
            if !made_new_devices {
                executor::timeout(Duration::from_secs(5), reconcile_receiver.recv()).await;
            }
        }
    }

    // TODO: Avoid holding a state lock while this is running.
    async fn open_serial_interface(
        shared: &Arc<Shared>,
        device: &AvailableDevice,
        machine: &mut MachineEntry,
    ) -> Result<()> {
        let info = device.verbose_proto().await?;

        machine.present = Some(info.clone());

        if let Some(serial) = &mut machine.serial {
            if serial.device_path.as_path() == &device.path() {
                return Ok(());
            }

            if !serial.disconnect_requested {
                serial
                    .controller
                    .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                    .await;
                serial.disconnect_requested = true;
            }
            machine.disconnect_requested = false;

            // Wait for disconnect to finish.
            return Ok(());
        }

        let auto_connect = machine.config.read().await?.auto_connect();

        if !auto_connect && !machine.connect_requested {
            return Ok(());
        }

        machine.connect_requested = false;

        // TODO: Check if we want to auto-connect.

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

        let watcher_task = ChildTask::spawn(Self::watch_serial_port(
            Arc::downgrade(shared),
            machine.id,
            controller.clone(),
        ));

        machine.serial = Some(OpenedSerialInterface {
            controller,
            device_path: device.path().to_owned(),
            device_info: info.clone(),
            watcher_task,
            disconnect_requested: false,
        });

        Ok(())
    }

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

            entry.disconnect_requested = false;

            entry.serial.take();

            if let Some(error) = error {
                entry.set_last_error(error);
            } else {
                entry.last_error = None;
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
            EntityType::CAMERA => {
                // TODO
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

            let proto = out.new_machines();
            proto.set_id(*machine_id);
            proto.set_config(machine.config.read().await?.value().clone());

            let state_proto = proto.state_mut();

            if let Some(iface) = &machine.serial {
                // In this case, we are in a CONNECTING | CONNECTED state.
                iface.controller.state_proto(state_proto).await?;

                state_proto.set_connection_device(iface.device_info.clone());
            } else {
                if machine.start_after.is_some() {
                    // TODO: Is this correct if auto_connect is disabled?
                    state_proto.set_connection_state(MachineStateProto_ConnectionState::ERROR);
                } else if machine.present.is_some() {
                    state_proto
                        .set_connection_state(MachineStateProto_ConnectionState::DISCONNECTED);
                } else {
                    state_proto.set_connection_state(MachineStateProto_ConnectionState::MISSING);
                }

                if let Some(v) = machine.present.clone() {
                    state_proto.set_connection_device(v);
                }
            }

            if let Some(e) = machine.last_error.clone() {
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

                    if entry.serial.is_some() {
                        return Err(
                            rpc::Status::failed_precondition("Machine already connected.").into(),
                        );
                    }

                    if entry.present.is_none() {
                        return Err(rpc::Status::failed_precondition(
                            "Machine has no device attached for connecting.",
                        )
                        .into());
                    }

                    entry.connect_requested = true;
                    entry.disconnect_requested = false;

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

                    if entry.serial.is_none() {
                        return Err(
                            rpc::Status::failed_precondition("Machine is not connected.").into(),
                        );
                    }

                    if entry.present.is_none() {
                        return Err(rpc::Status::failed_precondition(
                            "Machine has no device attached for connecting.",
                        )
                        .into());
                    }

                    entry.connect_requested = false;
                    entry.disconnect_requested = true;

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
                let state = self.shared.state.lock().await?.read_exclusive();
                let entry = state
                    .machines
                    .get(&request.machine_id())
                    .ok_or_else(|| rpc::Status::not_found("Machine not found."))?;

                // TODO: Don't update if the merge fails?
                lock!(config <= entry.config.write().await?, {
                    config.merge_from(new_config)
                })?;

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

            let serial = entry.serial.as_ref().clone().ok_or_else(|| {
                rpc::Status::failed_precondition("Machine not currently connected")
            })?;

            Result::<_, Error>::Ok(serial.controller.clone())
        })
    }

    async fn play_impl(&self, machine_id: u64) -> Result<()> {
        executor::spawn(Self::play_impl_inner(self.shared.clone(), machine_id))
            .join()
            .await
    }

    /// NOT CANCEL SAFE
    async fn play_impl_inner(shared: Arc<Shared>, machine_id: u64) -> Result<()> {
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

            let serial_entry = entry
                .serial
                .as_ref()
                .ok_or_else(|| rpc::Status::failed_precondition("Machine is not connected"))?;

            if !serial_entry.controller.connected().await? {
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
                    serial_entry.controller.clone(),
                    shared.changes.publisher(),
                )
                .await?,
            );

            entry.player = Some(PlayerEntry {
                player: player.clone(),
            });

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
