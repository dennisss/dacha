use std::sync::Arc;
use std::time::{Instant, Duration, SystemTime};
use std::collections::{HashMap, HashSet, VecDeque};

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::sync::{AsyncVariable, AsyncRwLock, AsyncMutex};
use executor::lock;
use executor::child_task::ChildTask;
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup, BroadcastChannel};
use cluster_client::ClusterMetaClient;
use mocap_proto::mocap::*;
use executor::channel::oneshot;
use executor::bundle::TaskResultBundle;
use cluster_client::service::address::{ServiceAddress, ServiceEntity, ServiceName};
use cluster_client::id::{entity_id_to_string, entity_id_from_string};
use http::Resolver;
use ptp_proto::ptp::TimeSyncConfig;
use ptp_proto::ptp::TimeSyncStub;
use ptp_proto::ptp::TimeSyncIntoService;
use cluster_client::service::create_rpc_channel;
use file::{project_path, LocalPathBuf};
use protobuf::Message;
use vision::{CameraIntrinsicsModel, CameraExtrinsics};
use mocap_simulation::*;
use math::matrix::Vector2d;
use protobuf::StaticMessage;

use crate::checkerboard::*;
use crate::matching::*;
use crate::proto_utils::*;
use crate::mjpeg::*;
use crate::config::*;
use crate::wanding::*;
use crate::rigid_body::*;
use crate::origin::*;
use crate::recording::*;
use crate::skeleton::{SkeletonTracker, standard_skeleton};
use crate::networking::*;
use crate::aux_rpc_server::*;
use crate::side_channel::*;

const CONFIG_PATCH_FILE: &'static str = "config.pb";

// TODO: Move to a shared crate.
macro_rules! log_every_sec {
    ($($arg:tt)*) => {{
        use std::time::Instant;
        use std::sync::{Mutex, LazyLock};

        static LAST_LOG_TIME: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::default());

        let now = Instant::now();

        let mut state = LAST_LOG_TIME.lock().unwrap();

        let should_log = {
            if let Some(last_time) = *state {
                now > last_time && now - last_time >= Duration::from_secs(1)
            } else {
                true
            }
        };

        if should_log {
            *state = Some(now);
        }

        drop(state);

        if should_log {
            eprintln!($($arg)*);
        }

    }};
}

// TODO: Need camera list sorting everywhere (mainly in status output protos)

// TODO: Auto-exposure for things like checkerboard calibration.

pub struct MocapManager {
    resources: ServiceResourceGroup,
    inner: MocapManagerInner,
}

impl_resource_passthrough!(MocapManager, resources);
impl_deref!(MocapManager::inner as MocapManagerInner);

pub struct MocapManagerInner {
    shared: Arc<Shared>
}

struct Shared {
    // meta_client: Arc<ClusterMetaClient>,
    data_dir: LocalPathBuf,
    config_path: LocalPathBuf,
    config: AsyncRwLock<ManagerConfigContainer>,
    config_writer_lock: AsyncMutex<()>,

    state: AsyncVariable<State>,
    camera_config_state: AsyncVariable<CameraConfigState>,
    merged_blobs: BroadcastChannel<Arc<ReadBlobsResponse>>,
    tracked_points: BroadcastChannel<Arc<ReadTrackedPointsResponse>>,
    simulator: Option<MocapSimulator>,
    skeleton_search_requests: AsyncMutex<HashMap<u32, bool, FastHasherBuilder>>,
    
    // TODO: Need to refactor this since this will hold a cyclic reference to Arc<Shared>
    // through the service. Minimally we need to ensure that it automatically cleans itself
    // up when the MocapManager resources are cancelled. 
    aux_rpc_server: AuxRpcServer,
}

#[derive(Default)]
struct State {
    cameras: HashMap<u64, CameraEntry, FastHasherBuilder>,

    /// Sorted by 'timestamp' (first_received time will also end up being sorted)
    frames: VecDeque<FrameEntry>,

    /// Largest frame timestamp value received so far.
    frame_timestamp_waterline: u64,

    mode: Mode,

    networking_status: NetworkingStatus,

    time_server_addr: String,

    side_channel: Option<Arc<DataSideChannel>>,
}

struct CameraConfigState {
    camera_config: MocapCameraConfigureRequest,
    // TODO: this is insuficient for handling restart conditions.
    camera_config_epoch: u64,
    active_camera_id: Option<u64>,
    single_camera_override: Option<(u64, MocapCameraConfigureRequest)>,

    configured: bool,
}

struct CameraEntry {
    // Exactly what was given by the resolver to detect if we need to reconnect.
    endpoint: String,

    ptp_addr: String,
    rpc_addr: String,

    camera_stub: Arc<CameraStub>,

    ptp_stub: Arc<TimeSyncStub>,
    ptp_leader: bool,

    supervisor_stub: Option<Arc<SupervisorStub>>,

    task: Option<ChildTask<()>>,

    status: Option<CameraStatus>,

    pending_save_intrinsics: Option<oneshot::Sender<Result<()>>>,

    // TODO: Also last time a ReadBlobs was received
    // TODO: Record number of late frames (Dropped)
}

struct CameraStatus {
    status: MocapCameraStatus,
    ptp_status: ptp_proto::ptp::StatusResponse,
    check_time: Instant,
}

struct FrameEntry {
    timestamp: u64,

    /// Local time at which the first result for this frame was received.
    first_received: Instant,

    results: HashMap<u64, ReadBlobsResponse, FastHasherBuilder>
}

enum Mode {
    Running,
    CheckerboardCalibration(CheckerboardCalibrationMode),
    WandingCalibration(WandingCalibrationMode),
    Recording(RecordingMode),
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Running
    }
}

impl MocapManager {

    pub async fn create(
        config: MocapManagerConfig,
        data_dir: LocalPathBuf,
        // meta_client: Arc<ClusterMetaClient>
    ) -> Result<Self> {

        file::create_dir_all(&data_dir).await?;

        let mut config = ManagerConfigContainer::create(&config)?;
        
        let config_path = data_dir.join(CONFIG_PATCH_FILE);
        if file::exists(&config_path).await? {
            let data = file::read(&config_path).await?;
            let diff = MocapManagerConfig::parse(&data)?;
            config.merge_from(&diff)?;
        }

        let resources = ServiceResourceGroup::new("MocapManager");

        let mut simulator = None;

        if config.make_dummy_cameras() {
            assert!(config.camera_service().is_empty());

            // TODO: Add to resources.
            simulator = Some(MocapSimulator::create(&config).await?);
        }

        let camera_config = config.initial_camera_config().clone();

        let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64;

        let shared = Arc::new(Shared {
            // meta_client,
            config: AsyncRwLock::new(config),
            config_writer_lock: Default::default(),
            data_dir,
            config_path,
            state: AsyncVariable::default(),
            camera_config_state: AsyncVariable::new(CameraConfigState {
                camera_config,
                camera_config_epoch: time,
                active_camera_id: None,
                single_camera_override: None,
                configured: false,
            }),
            merged_blobs: BroadcastChannel::default(),
            tracked_points: BroadcastChannel::default(),
            simulator,
            skeleton_search_requests: Default::default(),
            aux_rpc_server: Default::default(),
        });

        lock!(state <= shared.state.lock().await?, {
            state.networking_status.set_error("Initializing...");
        });

        resources.spawn_interruptable("resolver", MocapManagerInner::service_resolver_thread(shared.clone())).await;
        resources.spawn_interruptable("merger", MocapManagerInner::frame_merger_thread(shared.clone())).await;
        resources.spawn_interruptable("intrinsics_loader", MocapManagerInner::intrinsics_loader_thread(shared.clone())).await;

        let config = shared.config.read().await?;

        for per_cam in config.per_camera() {
            let camera_id = per_cam.camera_id();

            if config.make_dummy_cameras() {
                let camera_stub = Arc::new(CameraStub ::new(
                    Arc::new(rpc::LocalChannel::new(
                        shared.simulator.as_ref().unwrap().create_camera_service(camera_id)?
                        // Arc::new(mocap_camera::DummyMocapCamera::create(2).await?)
                        // .into_service()
                    ))
                ));

                let ptp_stub = Arc::new(TimeSyncStub::new(
                    Arc::new(rpc::LocalChannel::new(
                        Arc::new(ptp_core::DummyTimeSyncNode::create()).into_service()
                    ))
                ));

                lock!(state <= shared.state.lock().await?, {
                    let task = ChildTask::spawn(MocapManagerInner::camera_thread(
                        shared.clone(),
                        camera_id,
                        ptp_stub.clone(),
                        camera_stub.clone(),
                        None,
                    ));

                    state.cameras.insert(camera_id, CameraEntry {
                        endpoint: String::new(),
                        ptp_addr: String::new(),
                        rpc_addr: String::new(),
                        camera_stub,
                        ptp_stub,
                        ptp_leader: false,
                        supervisor_stub: None,
                        task: Some(task),
                        status: None,
                        pending_save_intrinsics: None,
                    });
                });
            }
        }

        // NOTE: Should be done after the extrinsics/intrinscs are setup.
        resources.spawn_interruptable("matcher", MocapManagerInner::matching_task(shared.clone())).await;

        drop(config);

        Ok(Self {
            resources,
            inner: MocapManagerInner {
                shared
            }
        })
    }

    pub async fn set_side_channel(&self, side_channel: Arc<DataSideChannel>) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            state.side_channel = Some(side_channel);
        });
        Ok(())
    }
}

impl MocapManagerInner {

    pub fn to_service(&self) -> Arc<dyn rpc::Service> {
        Self { shared: self.shared.clone() }.into_service()
    }

    pub async fn status(&self) -> Result<MocapManagerStatus> {
        Self::status_inner(&self.shared).await
    }

    async fn status_inner(shared: &Shared) -> Result<MocapManagerStatus> {
        let mut proto = MocapManagerStatus::default();

        proto.set_aux_rpc_server(shared.aux_rpc_server.status().await?);

        // TODO: Make sure we are consistent about the ordering of locking these.
        let config = shared.config.read().await?;
        let state = shared.state.lock().await?.read_exclusive();
        let camera_config_state = shared.camera_config_state.lock().await?.read_exclusive();

        // TODO: If the config contains more cameras than are currently in use, consider
        // pruning them.
        proto.set_config(config.value().clone());

        proto.set_networking(state.networking_status.clone());

        let now = Instant::now();

        for (camera_id, entry) in &state.cameras {
            {
                let proto = proto.new_cameras();
                proto.set_id(*camera_id);
                if let Some(s) = &entry.status {
                    proto.set_camera_status(s.status.clone());
                    proto.camera_status_mut().clear_config();
                    proto.camera_status_mut().clear_sensor();
                    proto.camera_status_mut().clear_camera_controls();

                    proto.set_ptp_status(s.ptp_status.clone());
                    proto.ptp_status_mut().config_mut().clear_follower();
                    proto.ptp_status_mut().config_mut().clear_leader();

                    proto.set_synced(true);
                    proto.set_last_sync_age((now - s.check_time).as_secs_f64());
                }

                proto.set_active(camera_config_state.active_camera_id == Some(*camera_id));
            }

            if proto.groups().is_empty() {
                if let Some(camera_status) = &entry.status {
                    let proto = proto.new_groups();

                    // Need at least one camera to become fully configured to get a valid config set.
                    // (mainly since we need to grab control values and such that are merged on the first config.)
                    if camera_config_state.configured {
                        proto.set_config(camera_config_state.camera_config.clone());
                        proto.set_camera_controls(camera_status.status.camera_controls().clone());
                    }

                    proto.set_sensor(camera_status.status.sensor().clone());
                }
            }
        }

        // Adding cameras that are enabled but not connected (since they also are required for
        // merging frames unless disabled).
        for camera in config.per_camera() {
            let camera_id = camera.camera_id();
            if !camera.enabled() || state.cameras.contains_key(&camera_id) {
                continue;
            }

            let proto = proto.new_cameras();
            proto.set_id(camera_id);
            proto.set_active(camera_config_state.active_camera_id == Some(camera_id));
        }

        if let Some((id, config)) = &camera_config_state.single_camera_override {
            let proto = proto.single_camera_override_mut();
            proto.set_camera_id(*id);
            proto.set_config(config.clone());
        }
 
        proto.cameras_mut().sort_by_key(|c| c.id());

        match &state.mode {
            Mode::Running => {
                proto.mode_mut().set_running(true);
            }
            Mode::CheckerboardCalibration(mode) => {
                proto.mode_mut().set_checkerboard_calibration(mode.to_proto());
            }
            Mode::WandingCalibration(mode) => {
                proto.mode_mut().set_wanding_calibration(mode.to_proto());
            }
            Mode::Recording(mode) => {
                proto.mode_mut().set_recording(mode.to_proto());
            }
        }

        Ok(proto)
    }

    pub async fn execute(&self, req: &ExecuteRequest) -> Result<ExecuteResponse> {

        match req.command_case() {
            ExecuteRequestCommandCase::ConfigureCameras(config) => {

                lock!(camera_config_state <= self.shared.camera_config_state.lock().await?, {
                    if config.camera_id() != 0 {
                        camera_config_state.single_camera_override = Some((config.camera_id(), config.config().clone()));
                    } else {
                        camera_config_state.camera_config = config.config().clone();
                        camera_config_state.single_camera_override = None;
                    }

                    camera_config_state.camera_config_epoch += 1;
                    camera_config_state.notify_all();
                });
            }
            
            ExecuteRequestCommandCase::SelectCamera(camera_id) => {
                lock!(camera_config_state <= self.shared.camera_config_state.lock().await?, {
                    camera_config_state.active_camera_id = Some(*camera_id);
                    camera_config_state.camera_config_epoch += 1;
                    camera_config_state.notify_all();
                });
            }

            ExecuteRequestCommandCase::ConfigureSimulation(c) => {
                if let Some(sim) = &self.shared.simulator {
                    sim.configure_animation(c)?;
                } else {
                    return Err(err_msg("Simulation not currently active"));
                }
            }

            ExecuteRequestCommandCase::StartCheckerboardCalibration(cmd) => {

                let config = self.shared.config.read().await?;

                lock!(state <= self.shared.state.lock().await?, {

                    match &state.mode {
                        Mode::Running => {}
                        _ => {
                            return Err(err_msg("Must be idle to start wanding"));
                        }
                    }

                    let camera_id = cmd.camera_id();

                    if !state.cameras.contains_key(&camera_id) {
                        return Err(err_msg("Unknown camera id"));
                    }

                    state.mode = Mode::CheckerboardCalibration(CheckerboardCalibrationMode::create(
                        config.checkerboard(),
                        camera_id,
                        &self.shared.data_dir
                    ));

                    Result::<_, Error>::Ok(())
                })?;
            }

            ExecuteRequestCommandCase::CaptureCheckerboardFrame(_) => {
                let res = lock!(state <= self.shared.state.lock().await?, {

                    let mode = match &state.mode {
                        Mode::CheckerboardCalibration(v) => v,
                        _ => {
                            return Err(err_msg("Must be idle to start wanding"));
                        }
                    };

                    let camera = state.cameras.get(&mode.camera_id())
                        .ok_or_else(|| err_msg("Camera missing"))?;

                    let camera_stub = camera.camera_stub.clone();

                    Ok(mode.capture_frame(camera_stub))
                })?;

                res.await?;
            }

            ExecuteRequestCommandCase::CancelCheckerboardCalibration(_) => {
                lock!(state <= self.shared.state.lock().await?, {
                    if let Mode::CheckerboardCalibration(_) = &state.mode {
                        state.mode = Mode::Running;
                    }
                });
            }

            ExecuteRequestCommandCase::ProcessCheckerboardCalibration(_) => {
                let res = lock!(state <= self.shared.state.lock().await?, {
                    let mode = match &state.mode {
                        Mode::CheckerboardCalibration(v) => v,
                        _ => {
                            return Err(err_msg("Must be idle to start wanding"));
                        }
                    };

                    Ok(mode.process_data())
                })?;

                res.await?;

            }

            ExecuteRequestCommandCase::ApplyCheckerboardCalibration(_) => {
                // TODO: this can't be interrupted.

                let writer_lock = self.shared.config_writer_lock.lock().await?;

                let (camera_id, result) = lock!(state <= self.shared.state.lock().await?, {
                    let mode = match &state.mode {
                        Mode::CheckerboardCalibration(v) => v,
                        _ => {
                            return Err(err_msg("Must be idle to start wanding"));
                        }
                    };

                    Ok((mode.camera_id(), mode.result()))
                })?;

                let result = match result {
                    Some(v) => v,
                    None => return Err(err_msg("Not processed yet"))
                };

                // TODO: Move this into the mode code.
                let mut patch = MocapManagerConfig::default();
                let cam = patch.new_per_camera();
                cam.set_camera_id(camera_id);
                cam.set_intrinsics(result.intrinsics.to_proto());

                self.apply_config_patch(&patch).await?;

                // Exit calibration mode.
                lock!(state <= self.shared.state.lock().await?, {
                    if let Mode::CheckerboardCalibration(_) = &state.mode {
                        state.mode = Mode::Running;
                    }
                });

                drop(writer_lock);
            }

            ExecuteRequestCommandCase::StartWandingCalibration(_) => {

                let initial_status = self.status().await?;
                let subscriber = self.shared.merged_blobs.subscribe(1024);

                let config = self.shared.config.read().await?;

                lock!(state <= self.shared.state.lock().await?, {

                    match &state.mode {
                        Mode::Running => {}
                        _ => {
                            return Err(err_msg("Must be idle to start wanding"));
                        }
                    }

                    let mode = WandingCalibrationMode::create(
                        config.value().clone(),
                        config.camera_intrinsics().clone(),
                        &self.shared.data_dir,
                        initial_status,
                        subscriber
                    )?;

                    state.mode = Mode::WandingCalibration(mode);

                    Result::<_, Error>::Ok(())
                })?;
            }

            ExecuteRequestCommandCase::CancelWandingCalibration(_) => {
                lock!(state <= self.shared.state.lock().await?, {
                    match &mut state.mode {
                        Mode::WandingCalibration(_) => {
                            state.mode = Mode::Running;
                        }
                        _ => {}
                    }

                    Result::<_, Error>::Ok(())
                })?;
            }

            ExecuteRequestCommandCase::ProcessWandingCalibration(_) => {
                let waiter = lock!(state <= self.shared.state.lock().await?, {
                    match &state.mode {
                        Mode::WandingCalibration(mode) => {
                            Some(mode.finish())
                        }
                        _ => None
                    }
                });

                if let Some(fut) = waiter {
                    fut.await;
                }
            }

            ExecuteRequestCommandCase::ApplyWandingCalibration(_) => {
                // TODO: this can't be interrupted.

                let writer_lock = self.shared.config_writer_lock.lock().await?;

                let patch = lock!(state <= self.shared.state.lock().await?, {
                    let mode = match &state.mode {
                        Mode::WandingCalibration(v) => v,
                        _ => {
                            return Err(err_msg("Must be idle to start wanding"));
                        }
                    };

                    Ok(mode.result())
                })?;

                let patch = match patch {
                    Some(v) => v,
                    None => return Err(err_msg("Not processed yet"))
                };

                self.apply_config_patch(&patch).await?;

                // Exit calibration mode.
                lock!(state <= self.shared.state.lock().await?, {
                    if let Mode::WandingCalibration(_) = &state.mode {
                        state.mode = Mode::Running;
                    }
                });

                drop(writer_lock);
            }

            ExecuteRequestCommandCase::ConfigureRigidBody(cmd) => {
                // TODO: We can assume that an empty entry (with just an id) can be deleted.
                // we should also generally verify we don't match empty bodies.

                let writer_lock = self.shared.config_writer_lock.lock().await?;
                
                let mut patch = {
                    let config = self.shared.config.read().await?;

                    let mut out = MocapManagerConfig::default();
                    *out.rigid_body_tracker_mut().bodies_mut() = config.rigid_body_tracker().bodies().to_vec();

                    out
                };

                let mut body = None;
                for b in patch.rigid_body_tracker_mut().bodies_mut() {
                    if b.id() == cmd.id() {
                        body = Some(b);
                        break;
                    }
                }

                let body = match body {
                    Some(v) => v,
                    None => patch.rigid_body_tracker_mut().new_bodies()
                };

                body.merge_from(cmd);

                self.apply_config_patch(&patch).await?;

                drop(writer_lock);
            }

            ExecuteRequestCommandCase::DeleteRigidBody(id) => {
                let writer_lock = self.shared.config_writer_lock.lock().await?;
                
                let mut patch = {
                    let config = self.shared.config.read().await?;

                    let mut out = MocapManagerConfig::default();
                    *out.rigid_body_tracker_mut().bodies_mut() = config.rigid_body_tracker().bodies().to_vec();

                    out
                };

                let mut found = false;
                for i in 0..patch.rigid_body_tracker().bodies().len() {
                    if patch.rigid_body_tracker().bodies()[i].id() == *id {
                        patch.rigid_body_tracker_mut().bodies_mut().remove(i);
                        found = true;
                        break;
                    }
                }

                if !found {
                    return Err(rpc::Status::not_found("No such rigid body").into());
                }

                self.apply_config_patch(&patch).await?;

                drop(writer_lock);
            }

            ExecuteRequestCommandCase::SetOrigin(cmd) => {
                let writer_lock = self.shared.config_writer_lock.lock().await?;

                let mut sub = self.shared.tracked_points.subscribe(1);
                let res = sub.recv().await?;

                let mut points = vec![];
                for p in res.points() {
                    points.push(TrackedPoint::from_proto(p)?);
                }

                let patch = {
                    let config = self.shared.config.read().await?;
                    set_origin_with_wand(&config, &points)?
                };

                self.apply_config_patch(&patch).await?;

                drop(writer_lock);
            }
            ExecuteRequestCommandCase::SetCameraEnabled(cmd) => {
                let writer_lock = self.shared.config_writer_lock.lock().await?;

                let mut patch = MocapManagerConfig::default();

                if cmd.all_cameras() {
                    lock!(state <= self.shared.state.lock().await?, {
                        for id in state.cameras.keys() {
                            let c = patch.new_per_camera();
                            c.set_camera_id(*id);
                            c.set_enabled(cmd.enabled());
                        }
                    });
                } else {
                    let c = patch.new_per_camera();
                    c.set_camera_id(cmd.camera_id());
                    c.set_enabled(cmd.enabled());
                }

                self.apply_config_patch(&patch).await?;

                drop(writer_lock);
            }
            ExecuteRequestCommandCase::StartRecording(cmd) => {
                let initial_status = self.status().await?;
                let blobs_subscriber = self.shared.merged_blobs.subscribe(1024);
                let points_subscriber = self.shared.tracked_points.subscribe(1024);

                lock!(state <= self.shared.state.lock().await?, {

                    match &state.mode {
                        Mode::Running => {}
                        _ => {
                            return Err(err_msg("Must be idle to start recording"));
                        }
                    }

                    let mode = RecordingMode::create(
                        &self.shared.data_dir,
                        initial_status,
                        blobs_subscriber,
                        points_subscriber
                    )?;

                    state.mode = Mode::Recording(mode);

                    Result::<_, Error>::Ok(())
                })?;

            }
            ExecuteRequestCommandCase::StopRecording(cmd) => {

                let f = lock!(state <= self.shared.state.lock().await?, {
                    match &state.mode {
                        Mode::Recording(m) => Ok(m.finish()),
                        _ => {
                            Err(err_msg("Must be recording to stop recording"))
                        }
                    }
                })?;

                f.await;

                lock!(state <= self.shared.state.lock().await?, {
                    state.mode = Mode::Running;
                });
            }

            ExecuteRequestCommandCase::CreateSkeleton(cmd) => {
                let writer_lock = self.shared.config_writer_lock.lock().await?;

                let mut patch = {
                    let mut p = MocapManagerConfig::default();
                    let config = self.shared.config.read().await?;
                    p.skeleton_tracker_mut().skeletons_mut().extend(config.skeleton_tracker().skeletons().iter().cloned());
                    p
                };

                let mut next_id = 1;

                for s in patch.skeleton_tracker().skeletons() {
                    next_id = next_id.max(s.id() + 1);
                }

                let mut skel = standard_skeleton();
                skel.id = next_id;
                patch.skeleton_tracker_mut().add_skeletons(skel.to_proto());

                self.apply_config_patch(&patch).await?;

                drop(writer_lock);
            }

            ExecuteRequestCommandCase::DeleteSkeleton(cmd) => {
                let writer_lock = self.shared.config_writer_lock.lock().await?;

                let mut patch = {
                    let mut p = MocapManagerConfig::default();
                    let config = self.shared.config.read().await?;
                    p.skeleton_tracker_mut().skeletons_mut().extend(config.skeleton_tracker().skeletons().iter().cloned());
                    p
                };

                for i in 0..patch.skeleton_tracker().skeletons().len() {
                    let s = &patch.skeleton_tracker().skeletons()[i];
                    if s.id() == cmd.id() {
                        patch.skeleton_tracker_mut().skeletons_mut().remove(i);
                        break;
                    }
                }

                self.apply_config_patch(&patch).await?;

                drop(writer_lock);
            }

            ExecuteRequestCommandCase::SetSkeletonSearching(cmd) => {                
                lock!(reqs <= self.shared.skeleton_search_requests.lock().await?, {
                    reqs.insert(cmd.id(), cmd.searching());
                });
            }

            ExecuteRequestCommandCase::StartAuxRpcServer(_) => {
                let config = self.shared.config.read().await?;
                self.shared.aux_rpc_server.start(self.to_service(), config.aux_rpc_server()).await?;
            }

            ExecuteRequestCommandCase::StopAuxRpcServer(_) => {
                self.shared.aux_rpc_server.stop().await?;
            }

            ExecuteRequestCommandCase::SaveFactoryIntrinsics(cmd) => {

                let (sender, receiver) = oneshot::channel();

                lock!(state <= self.shared.state.lock().await?, {
                    let entry = state.cameras.get_mut(&cmd.camera_id())
                        .ok_or_else(|| err_msg("Unknown camera id"))?;

                    if entry.pending_save_intrinsics.is_some() {
                        return Err(err_msg("Already saving intrinsics..."));
                    }

                    entry.pending_save_intrinsics = Some(sender);

                    Result::<_, Error>::Ok(())
                })?;

                receiver.recv().await
                    .map_err(|_| err_msg("Receiver failed"))??;
            }
            ExecuteRequestCommandCase::NOT_SET => {
                return Err(err_msg("Unknown command"));
            }
        }

        Ok(ExecuteResponse::default())
    }

    async fn apply_config_patch(&self, patch: &MocapManagerConfig) -> Result<()> {
        Self::apply_config_patch_inner(&self.shared, patch).await
    }

    async fn apply_config_patch_inner(shared: &Shared, patch: &MocapManagerConfig) -> Result<()> {
        let diff = lock!(config <= shared.config.write().await?, {
            config.merge_from(patch)?;
            Result::<_, Error>::Ok(config.diff().clone())
        })?;

        let data = diff.serialize()?;

        // TODO: Need atomic file operations here.
        file::write(&shared.config_path, data).await?;

        Ok(())
    }

    pub async fn live_stream(&self, camera_id: u64) -> http::Response {

        let stub = lock!(state <= self.shared.state.lock().await.unwrap(), {
            let camera = match state.cameras.get(&camera_id) {
                Some(v) => v,
                None => return None
            };

            Some(camera.camera_stub.clone())
        });

        let stub = match stub {
            Some(v) => v,
            None => return http_util::not_found()
        };

        create_camera_live_stream(stub).await
    }

    /// Global (per-manager) thread that monitors the set of workers in the camera
    /// service. The jobs of this thread are to:
    /// - Maintain the State::cameras set (add new cameras / remove dead ones).
    /// - Maintain a PTP leader.
    async fn service_resolver_thread(shared: Arc<Shared>) -> Result<()> {
        loop {
            if let Err(e) = Self::service_resolver_thread_inner(shared.clone()).await {
                eprintln!("[Resolver Error] {}", e);

                lock!(state <= shared.state.lock().await?, {
                    let mut s = NetworkingStatus::default();
                    s.set_error(e.to_string());
                    state.networking_status = s;
                });
            }

            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    async fn service_resolver_thread_inner(shared: Arc<Shared>) -> Result<()> {

        let mut resolver = CameraResolver::create().await?;

        // TODO: Monitor for failures in this resource.
        let time_server = resolver.create_time_server().await?;

        let time_server_addr = time_server.local_addr()?.to_string();

        println!("Time Server running on: {}", time_server_addr);
        lock!(state <= shared.state.lock().await?, {
            state.time_server_addr = time_server_addr;
        });

        loop {
            // TODO: Explicitly rate limit the resolving rate.
            let current_cameras = resolver.resolve().await?;

            let mut enabled_cameras = HashSet::<u64, FastHasherBuilder>::default();
            {
                let config = shared.config.read().await?;
                for c in config.per_camera() {
                    if c.enabled() {
                        enabled_cameras.insert(c.camera_id());
                    }
                }
            }

            // Make sure all cameras have connections setup.

            // TODO: Make sure that leader selection is deterministic assuming all cameras are present.

            let mut have_ptp_leader = false;
            let mut new_cameras = vec![];
            lock!(state <= shared.state.lock().await?, {

                // Remove old cameras.
                // TODO: Eventually also remove cameras if we haven't been able to connect to them for some period of time.
                if resolver.disconnect_missing_cameras() {
                    state.cameras.retain(|old_camera_id, old_entry| {
                        if !current_cameras.contains_key(old_camera_id) {
                            return false;
                        }

                        true
                    });
                }

                for (camera_id, camera_endpoint) in current_cameras {
                    if let Some(old_entry) = state.cameras.get(&camera_id) {
                        if old_entry.endpoint == camera_endpoint {
                            continue;
                        }
                    }

                    new_cameras.push((camera_id, camera_endpoint));
                }
            });


            let mut new_camera_entries = vec![];
            for (camera_id, endpoint) in new_cameras {

                let conn = resolver.connect(&endpoint).await?;
                let supervisor_stub = resolver.connect_to_supervisor(&endpoint).await?;

                new_camera_entries.push((camera_id, CameraEntry {
                    endpoint,
                    rpc_addr: conn.rpc_addr,
                    ptp_addr: conn.ptp_addr,
                    ptp_stub: conn.ptp_stub,
                    camera_stub: conn.camera_stub,
                    ptp_leader: false,
                    supervisor_stub: Some(supervisor_stub),
                    status: None,
                    pending_save_intrinsics: None,

                    // NOTE: We only create this after the entry is added to the state
                    // to avoid race conditions.
                    task: None
                }));
            }

            lock!(state <= shared.state.lock().await?, {
                for (camera_id, mut entry) in new_camera_entries {

                    entry.task = Some(ChildTask::spawn(Self::camera_thread(
                        shared.clone(),
                        camera_id,
                        entry.ptp_stub.clone(),
                        entry.camera_stub.clone(),
                        entry.supervisor_stub.clone(),
                    )));

                    state.cameras.insert(camera_id, entry);
                }

                let mut have_ptp_leader = false;
                let mut first_enabled = None;
                for (id, entry) in &mut state.cameras {
                    let is_enabled = enabled_cameras.contains(id);
                    entry.ptp_leader &= is_enabled;
                    have_ptp_leader |= entry.ptp_leader;

                    if is_enabled {
                        first_enabled = Some(*id);
                    }
                }

                if !have_ptp_leader {
                    if let Some(id) = first_enabled {
                        state.cameras.get_mut(&id).unwrap().ptp_leader = true;
                    }
                }


                let mut s = NetworkingStatus::default();
                s.set_iface_name(resolver.iface_name());
                s.set_iface_description(resolver.iface_description());
                s.set_healthy(true);

                state.networking_status = s;

            });


            // TODO: Mark as good in the state.

            // TODO: Instead rely on the resolver notifications
            // executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    /// Global (per-manager) thread that is responsible for publishing finalized groups of blobs for a
    /// frame from all cameras once all data is received or a timeout has occurred.
    async fn frame_merger_thread(shared: Arc<Shared>) -> Result<()> {

        let config = shared.config.read().await?;

        let timeout = Duration::from_secs_f32(config.frame_aggregation_timeout());

        drop(config);

        loop {
            // TODO: For this to be valid, we should maintain some tombstone entries
            // whenever we are missing a camera that is enabled (and indicate that in the UI).
            let num_enabled = shared.config.read().await?.num_enabled_cameras();

            let mut state = shared.state.lock().await?.enter();
            if state.frames.is_empty() {
                state.wait().await;
                continue;
            }

            let now = Instant::now();

            let time_elapsed = now - state.frames[0].first_received;

            // NOTE: If a camera was just disabled, this '>= num_enabled' check might be stale for one
            // frame but thats usually not a big deal.
            let complete = (
                state.frames[0].results.len() >= num_enabled ||
                time_elapsed >= timeout
            );

            if !complete {
                let _ = executor::timeout(timeout - time_elapsed, state.wait()).await;
                continue;
            }

            if time_elapsed > Duration::from_millis(5) {
                log_every_sec!("Took {:?} to gather all camera results", time_elapsed);
            }

            let frame = state.frames.pop_front().unwrap();

            let mut out = ReadBlobsResponse::default();
            out.set_frame_timestamp(frame.timestamp);

            for (cam_id, res) in frame.results {
                let mut cam = res.cameras()[0].as_ref().clone();
                cam.set_camera_id(cam_id);
                out.add_cameras(cam);
            }

            // println!("AGGREGATED FRAME: {} : {}", out.frame_timestamp(), out.cameras().len());

            shared.merged_blobs.send(Arc::new(out));

            state.exit();
        }
    }


    // TODO: Need all the per-camera threads to use weak pointers.

    //// Per-camera thread
    async fn camera_thread(
        shared: Arc<Shared>,
        camera_id: u64,
        ptp_stub: Arc<TimeSyncStub>,
        camera_stub: Arc<CameraStub>,
        supervisor_stub: Option<Arc<SupervisorStub>>
    ) {

        loop {
            let mut bundle = TaskResultBundle::new();
            bundle.add("Status", Self::camera_status_monitor(shared.clone(), camera_id, ptp_stub.clone(), camera_stub.clone(), supervisor_stub.clone()));
            bundle.add("ReadBlobs", Self::camera_read_blobs_thread(shared.clone(), camera_id, camera_stub.clone()));

            let res = bundle.join().await;

            eprintln!("Camera {} Thread Terminated: {:?}", entity_id_to_string(camera_id).unwrap(), res);

            // TODO: Use exponential backoff (though ideally retry sooner if we get a sign of life like a heartbeat).
            executor::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Per-camera thread which has two jobs:
    /// - Checks the status of the PTP and camera code.
    /// - Configures the camera once PTP is syncronized.
    ///
    /// TODO: This will need RPC timeouts.
    async fn camera_status_monitor(
        shared: Arc<Shared>,
        camera_id: u64,
        ptp_stub: Arc<TimeSyncStub>,
        camera_stub: Arc<CameraStub>,
        supervisor_stub: Option<Arc<SupervisorStub>>
    ) -> Result<()> {

        // TODO: Seem to be possible for the status page to tell us there are multiple leaders

        let mut request_context = rpc::ClientRequestContext::default();
        request_context.http.wait_for_ready = true;

        loop {
            let now = Instant::now();

            let ptp_status = {
                let req = ptp_proto::ptp::StatusRequest::default();
                ptp_stub.Status(&request_context, &req).await.result?
            };

            let intended_ptp_config = Self::get_ptp_config(&shared, camera_id).await?;
            if *ptp_status.config() != intended_ptp_config {
                // eprintln!("Configuring PTP for camera...");

                if intended_ptp_config.has_leader() {
                    // Ideally the leader is configured slightly after the followers to ensure the followers don't
                    // complain about not being configured yet when a leader sync occurs.
                    executor::sleep(Duration::from_millis(10)).await?;
                }

                let mut req = ptp_proto::ptp::ConfigureRequest::default();
                req.set_config(intended_ptp_config);
                ptp_stub.Configure(&request_context, &req).await.result?;
            }

            let mut status = {
                let req = StatusRequest::default();
                camera_stub.Status(&request_context, &req).await.result?
            };

            // TODO: Can't do this until the PPS divider is alive.
            // TODO: Only configure once PTP is well synced.
            // TODO: Make this check smarter.

            let (intended_camera_config, intended_per_camera) = lock!(config_state <= shared.camera_config_state.lock().await?, {
                let (config, is_per_camera) = Self::get_camera_config(camera_id, &config_state);
                (config, is_per_camera)
            });

            if status.config().epoch() != intended_camera_config.epoch() {
                camera_stub.Configure(&request_context, &intended_camera_config).await.result?;

                status = {
                    let req = StatusRequest::default();
                    camera_stub.Status(&request_context, &req).await.result?
                };

                // Pull any merge conflicts that were adjusted by the camera into our local copy of the config.
                // (the hope is that all cameras behave the same way)
                lock!(config_state <= shared.camera_config_state.lock().await?, {
                    if status.config().epoch() == config_state.camera_config_epoch {
                        if intended_per_camera {
                            config_state.single_camera_override = Some((camera_id, status.config().clone()));
                        } else {
                            config_state.camera_config = status.config().clone();
                        }

                        config_state.configured = true;
                    } else {
                        eprintln!("Status returned different config epoch")
                    }
                });
            }


            let mut pending_save_intrinsics = None;

            lock!(state <= shared.state.lock().await?, {
                let entry = match state.cameras.get_mut(&camera_id) {
                    Some(v) => v,
                    None => return
                };

                // TODO: Ideally we keep it in place so that we don't allow new updates to restart until we are fully done the old one
                pending_save_intrinsics = entry.pending_save_intrinsics.take();

                // TODO: Setting this is now generally sufficient for knowing if it is in sync since
                // future changes to the config also need to be accounted for.
                // 
                entry.status = Some(CameraStatus {
                    status: status.clone(),
                    ptp_status,
                    check_time: now
                });
            });

            if let Some(sender) = pending_save_intrinsics {
                sender.send(Self::store_factory_intrinsics(&shared, camera_id, &status, supervisor_stub.clone()).await);
            }

            let res = executor::timeout(
                Duration::from_secs(1),
                Self::wait_for_new_config_epoch(&shared, intended_camera_config.epoch())
            ).await;
        }
    }

    async fn store_factory_intrinsics(
        shared: &Shared,
        camera_id: u64,
        status: &MocapCameraStatus,
        supervisor_stub: Option<Arc<SupervisorStub>>
    ) -> Result<()> {
        let mut hardware_config = status.hardware_config().clone();

        let config = shared.config.read().await?;

        let camera_config = config.per_camera().iter()
            .find(|c| c.camera_id() == camera_id)
            .ok_or_else(|| err_msg("Camera has no existing config"))?;

        if !camera_config.has_intrinsics() {
            return Err(err_msg("Camera has no intriniscs yet"));
        }

        if hardware_config.factory_intrinsics() == camera_config.intrinsics() {
            return Err(err_msg("Factory intrinsics already match camera's active config"));
        }

        hardware_config.set_factory_intrinsics(camera_config.intrinsics().clone());

        drop(config);

        let hardware_config_payload = hardware_config.serialize()?;

        let supervisor_stub = supervisor_stub.clone()
            .ok_or_else(|| err_msg("No supervisor connection for camera"))?;

        let mut updater = UpdateClient::create(&supervisor_stub).await?;

        updater.start_update().await?;

        updater.send_payload(&hardware_config_payload).await?;

        updater.write_file("/boot/firmware/camera_hardware.pb").await?;

        updater.commit_update().await?;

        // We need to restart for the camera to notice the new hardware config.
        // NOTE: this will probably return an error since we are killing the software.
        let _ = restart_camera(&supervisor_stub).await;

        Ok(())
    }

    async fn intrinsics_loader_thread(shared: Arc<Shared>) -> Result<()> {
        loop {
            {
                let lock = shared.config_writer_lock.lock().await?;

                let mut cameras_with_intrinsics = HashSet::<u64, FastHasherBuilder>::default();
                {
                    let config = shared.config.read().await?;
                    for c in config.per_camera() {
                        if c.has_intrinsics() {
                            cameras_with_intrinsics.insert(c.camera_id());
                        }
                    }
                }

                let mut patch = MocapManagerConfig::default();

                lock!(state <= shared.state.lock().await?, {

                    for (camera_id, entry) in &state.cameras {
                        if cameras_with_intrinsics.contains(&camera_id) {
                            continue;
                        }

                        let status = match &entry.status {
                            Some(v) => v,
                            None => continue
                        };

                        if !status.status.hardware_config().has_factory_intrinsics() {
                            continue;
                        }

                        println!("Loading intrinics for camera: {}", entity_id_to_string(*camera_id).unwrap());

                        let cam = patch.new_per_camera();
                        cam.set_camera_id(*camera_id);
                        cam.set_intrinsics(status.status.hardware_config().factory_intrinsics().clone());
                    }
                });


                if !patch.per_camera().is_empty() {
                    Self::apply_config_patch_inner(&shared, &patch).await?;
                }

                drop(lock);
            }

            // TODO: Make it react faster to status updates from cameras. 
            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    async fn wait_for_new_config_epoch(shared: &Shared, last_epoch: u64) -> Result<()> {
        loop {
            let state = shared.camera_config_state.lock().await?.read_exclusive();
            if state.camera_config_epoch != last_epoch {
                return Ok(());
            }

            state.wait().await;
        }
    }


    async fn get_ptp_config(shared: &Shared, camera_id: u64) -> Result<TimeSyncConfig> {
        let config = shared.config.read().await?;

        lock!(state <= shared.state.lock().await?, {
            let entry = state.cameras.get(&camera_id)
                .ok_or_else(|| err_msg("Missing camera"))?;

            let mut ptp_config = TimeSyncConfig::default();

            let template = ptp_core::default_config_template();

            if entry.ptp_leader {
                ptp_config.set_leader(template.leader().clone());

                for (follower_camera_id, entry) in &state.cameras {
                    if *follower_camera_id == camera_id {
                        continue;
                    }

                    if !config.camera_enabled(*follower_camera_id) {
                        continue;
                    }

                    let proto = ptp_config.leader_mut().new_followers();
                    proto.set_rpc_addr(entry.rpc_addr.clone());
                    proto.set_ptp_addr(entry.ptp_addr.clone());
                }


                ptp_config.set_basic_client(template.basic_client().clone());
                ptp_config.basic_client_mut().set_server_addr(&state.time_server_addr);

            } else {
                ptp_config.set_follower(template.follower().clone());
            }

            Ok(ptp_config)
        })
    }

    fn get_camera_config(camera_id: u64, config_state: &CameraConfigState) -> (MocapCameraConfigureRequest, bool) {
        let (mut config, per_camera) = {
            if config_state.single_camera_override.is_some() && config_state.single_camera_override.as_ref().unwrap().0 == camera_id {
                (config_state.single_camera_override.as_ref().unwrap().1.clone(), true)
            } else {
                (config_state.camera_config.clone(), false)
            }
        };

        config.set_epoch(config_state.camera_config_epoch);

        let leds_on = config.leds_on();

        for v in config.rgb_led_colors_mut() {
            if !leds_on {
                *v = 0;
            } else if config_state.active_camera_id == Some(camera_id) {
                *v = 100; // blue at (100 / 255) intensity
            } else {
                // Weak green
                // TODO: Have this timeout on the camera if it hasn't been talked to in a while.
                *v = 20 << 8;
            }
        }

        (config, per_camera)
    }

    /// Per-camera thread which handles continously calling ReadBlobs.
    ///
    /// TODO: Retry with exponential backoff.
    async fn camera_read_blobs_thread(
        shared: Arc<Shared>, camera_id: u64, stub: Arc<CameraStub >
    ) -> Result<()> {
        let mut ctx = rpc::ClientRequestContext::default();
        ctx.http.wait_for_ready = true;

        let mut req = ReadBlobsRequest::default();

        let mut res_stream = stub.ReadBlobs(&ctx, &req).await;

        while let Some(mut res) = res_stream.recv().await {

            let rx_time = Instant::now();

            if res.cameras().len() != 1 {
                log_every_sec!("Bad blobs format");
                continue;
            }


            let num_enabled;
            let enabled;
            {
                let config = shared.config.read().await?;
                num_enabled = config.num_enabled_cameras();
                enabled = config.camera_enabled(camera_id);
            }

            if !enabled {
                continue;
            }

            lock!(state <= shared.state.lock().await?, {

                // TODO: There are risks that a camera that is just starting to act poorly sends up very far in the 
                // future timestamps and messed up tracking for all other cameras.

                // TODO: Ignore cameras which aren't synced (though non-healthy ones shouldn't block frame completion).

                for i in 0..state.frames.len() {
                    let frame = &mut state.frames[i];

                    if res.frame_timestamp() < frame.timestamp {
                        log_every_sec!("Rejecting stale timestamp for new frame");
                        return;
                    }
                    
                    if res.frame_timestamp() == frame.timestamp {
                        if frame.results.contains_key(&camera_id) {
                            log_every_sec!("Duplicate blob data for frame");
                            return;
                        }

                        res.cameras_mut()[0].set_latency((rx_time - frame.first_received).as_nanos() as u64);
                        frame.results.insert(camera_id, res);

                        // TODO: Base this on the number of 'healthy' cameras.
                        if i == 0 && frame.results.len() == num_enabled {
                            state.notify_all();
                        }
                        return;
                    }
                }

                if state.frame_timestamp_waterline >= res.frame_timestamp() {
                    log_every_sec!("Rejecting stale timestamp for new frame (2)");
                    return;
                }

                state.frame_timestamp_waterline = res.frame_timestamp();

                let mut entry = FrameEntry {
                    timestamp: res.frame_timestamp(),
                    first_received: rx_time,
                    results: HashMap::default()
                };

                res.cameras_mut()[0].set_latency((rx_time - entry.first_received).as_nanos() as u64);

                entry.results.reserve(state.cameras.len());
                entry.results.insert(camera_id, res);
                state.frames.push_back(entry);

                // When we get the first entry, trigger the timeout for the timestamp
                // to start.
                if state.frames.len() == 1 {
                    state.notify_all();
                }
            });
        }

        res_stream.finish().await?;

        Err(err_msg("ReadBlobs terminated from the camera"))
    }

    // TODO: This probably deserves its own OS thread.
    async fn matching_task(shared: Arc<Shared>) -> Result<()> {
        let config = shared.config.read().await?;
        let mut matcher = BlobMatcher::new(config.matching());
        drop(config);

        let mut rigid_body_tracker = RigidBodyTracker::default();

        let mut skeleton_tracker = SkeletonTracker::default();

        let mut last_config_revision = 0;

        let mut subscriber = shared.merged_blobs.subscribe(8);

        loop {
            let blobs_res = subscriber.recv().await?;

            // Update camera parameters if changed.
            {
                let config = shared.config.read().await?;

                if config.revision() != last_config_revision {
                    let mut params = vec![];
                    
                    // TODO: Limit to only currently discovered cameras?
                    for (cam_id, extrinsics) in config.camera_extrinsics() {
                        let intrinsics = match config.camera_intrinsics().get(cam_id) {
                            Some(v) => v,
                            None => continue
                        };

                        params.push(CameraParameters {
                            id: *cam_id,
                            extrinsics: extrinsics.clone(),
                            intrinsics: intrinsics.clone()
                        });
                    }

                    matcher.set_camera_parameters(&params);

                    rigid_body_tracker.set_config(config.rigid_body_tracker().clone())?;

                    skeleton_tracker.set_config(config.skeleton_tracker().clone())?;
                }

                last_config_revision = config.revision();
                drop(config);
            }

            lock!(reqs <= shared.skeleton_search_requests.lock().await?, {
                for (id, searching) in reqs.drain() {
                    skeleton_tracker.set_skeleton_searching(id, searching);
                }
            });

            let mut points = matcher.run(&blobs_res);

            rigid_body_tracker.run(&points);
            // TODO: Expose back propagation directly through the API.
            rigid_body_tracker.backpropagate_predicted_points(&mut matcher);
            points = matcher.points();

            // TODO: When doing skeleton tracking, exclude any points found for rigid bodies.

            skeleton_tracker.run(blobs_res.frame_timestamp(), &points);
            skeleton_tracker.backpropagate_predicted_points(&mut matcher);
            points = matcher.points();


            let mut res = ReadTrackedPointsResponse::default();
            res.set_frame_timestamp(blobs_res.frame_timestamp());
            for pt in points {
                res.add_points(pt.to_proto());
            }

            let rigid_bodies = rigid_body_tracker.bodies();
            for body in rigid_bodies {
                if body.transform.is_none() {
                    continue;
                }

                res.add_rigid_bodies(body.to_proto());
            }

            for skel in skeleton_tracker.to_state_protos() {
                res.add_skeletons(skel);
            }

            shared.tracked_points.send(Arc::new(res));
        }

        Ok(())
    }
}

#[async_trait]
impl ManagerService for MocapManagerInner {

    async fn Status(
        &self,
        request: rpc::ServerRequest<MocapManagerStatusRequest>,
        response: &mut rpc::ServerResponse<MocapManagerStatus>
    ) -> Result<()> {
        response.value = self.status().await?;
        Ok(())
    }

    async fn Execute(
        &self,
        request: rpc::ServerRequest<ExecuteRequest>,
        response: &mut rpc::ServerResponse<ExecuteResponse>
    ) -> Result<()> {
        self.execute(&request.value).await?;
        Ok(())
    }

    async fn ReadBlobs(
        &self,
        request: rpc::ServerRequest<ReadBlobsRequest>,
        response: &mut rpc::ServerStreamResponse<ReadBlobsResponse>
    ) -> Result<()> {        
        // TODO: Need some logging if we ever drop frames
        let mut subscriber = self.shared.merged_blobs.subscribe(1024);

        response.send_head().await?;

        let mut last_response = Instant::now();

        let mut min_interval = Duration::ZERO;
        if request.max_rate() != 0 {
            min_interval = Duration::from_secs_f32(1.0 / (request.max_rate() as f32));
        }

        loop {
            let res = subscriber.recv().await?;
            
            let now = Instant::now();
            if now - last_response >= min_interval {
                response.send(res.as_ref().clone()).await?;
                last_response = now;
            }
        }

        Ok(())
    }

    async fn ReadTrackedPoints(
        &self,
        request: rpc::ServerRequest<ReadTrackedPointsRequest>,
        response: &mut rpc::ServerStreamResponse<ReadTrackedPointsResponse>
    ) -> Result<()> {

        // TODO: Need some logging if we ever drop frames
        let mut subscriber = self.shared.tracked_points.subscribe(1024);

        response.send_head().await?;

        let mut last_response = Instant::now();

        let mut min_interval = Duration::ZERO;
        if request.max_rate() != 0 {
            min_interval = Duration::from_secs_f32(1.0 / (request.max_rate() as f32));
        }

        loop {
            let res = subscriber.recv().await?;
            
            let now = Instant::now();
            if now - last_response >= min_interval {
                response.send(res.as_ref().clone()).await?;
                last_response = now;
            }
        }

        Ok(())
    }

    async fn ReadFrames(
        &self,
        request: rpc::ServerRequest<ReadFramesRequest>,
        response: &mut rpc::ServerStreamResponse<ReadFramesResponse>
    ) -> Result<()> {
        // TODO: Need to ensure there is only one ReadFrames request to each camera per Manager (and ideally we hide errors from clients).
        
        let camera_id = request.camera_id();

        let stub = lock!(state <= self.shared.state.lock().await.unwrap(), {
            let camera = match state.cameras.get(&camera_id) {
                Some(v) => v,
                None => return None
            };

            Some(camera.camera_stub.clone())
        });

        let stub = match stub {
            Some(v) => v,
            None => return Err(err_msg("Unknown camera"))
        };

        let mut channel: Option<Arc<DataSideChannel>> = None;
        if request.side_channel_id() != 0 {
            let c = lock!(state <= self.shared.state.lock().await.unwrap(), {
                state.side_channel.clone()
            });
            channel = Some(c.ok_or_else(|| err_msg("Missing side channel"))?);
        }

        let req = ReadFramesRequest::default();
        let ctx = rpc::ClientRequestContext::default();

        let mut res_stream = stub.ReadFrames(&ctx, &req).await;

        while let Some(res) = res_stream.recv().await {
            if let Some(channel) = &channel {
                channel.push(request.side_channel_id(), res.mjpeg().into()).await?;
            } else {
                response.send(res).await?;
            }
        }

        res_stream.finish().await?;
        Err(err_msg("Unexpected end to frames stream"))
    }

}


