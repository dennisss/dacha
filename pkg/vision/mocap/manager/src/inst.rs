use std::sync::Arc;
use std::time::{Instant, Duration};
use std::collections::{HashMap, VecDeque};

use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::sync::AsyncVariable;
use executor::lock;
use executor::child_task::ChildTask;
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup, BroadcastChannel};
use cluster_client::ClusterMetaClient;
use mocap_proto::mocap::*;
use executor::bundle::TaskResultBundle;
use cluster_client::service::address::{ServiceAddress, ServiceEntity, ServiceName};
use cluster_client::id::{entity_id_to_string, entity_id_from_string};
use http::Resolver;
use ptp::TimeSyncNode;
use ptp_proto::ptp::{TimeSyncRole, TimeSyncConfig};
use ptp_proto::ptp::TimeSyncStub;
use ptp_proto::ptp::TimeSyncIntoService;
use cluster_client::service::create_rpc_channel;
use sstable::record_log::*;
use file::project_path;
use protobuf::Message;
use vision::{CameraIntrinsicsModel, CameraExtrinsics};

use crate::matching::*;
use crate::proto_utils::*;

// TODO: Need camera list sorting everywhere (mainly in status output protos)

pub struct MocapManager {
    resources: ServiceResourceGroup,
    shared: Arc<Shared>
}

impl_resource_passthrough!(MocapManager, resources);

struct Shared {
    meta_client: Arc<ClusterMetaClient>,
    config: MocapManagerConfig,
    state: AsyncVariable<State>,
    camera_config_state: AsyncVariable<CameraConfigState>,
    merged_blobs: BroadcastChannel<Arc<ReadBlobsResponse>>,
    tracked_points: BroadcastChannel<Arc<ReadTrackedPointsResponse>>,
}

#[derive(Default)]
struct State {
    cameras: HashMap<u64, CameraEntry, FastHasherBuilder>,

    camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
    camera_extrinsics: HashMap<u64, CameraExtrinsics, FastHasherBuilder>,

    /// Sorted by 'timestamp' (first_received time will also end up being sorted)
    frames: VecDeque<FrameEntry>,

    /// Largest frame timestamp value received so far.
    frame_timestamp_waterline: u64,

    calibration: Option<CalibrationState>,
}

struct CameraConfigState {
    camera_config: MocapCameraConfigureRequest,
    camera_config_epoch: u64,
    active_camera_id: Option<u64>,
}

struct CameraEntry {
    worker_address: String,

    camera_stub: Arc<MocapCameraStub>,

    ptp_stub: Arc<TimeSyncStub>,
    ptp_leader: bool,

    /// Version of the camera config that was last successfully
    /// pushed to this camera. 
    camera_config_epoch: u64,

    task: Option<ChildTask<()>>,

    status: Option<CameraStatus>,

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

struct CalibrationState {
    stopping: bool,
    task: ChildTask,
}

impl MocapManager {

    pub async fn create(
        config: MocapManagerConfig,
        meta_client: Arc<ClusterMetaClient>
    ) -> Result<Self> {

        let resources = ServiceResourceGroup::new("MocapManager");

        let shared = Arc::new(Shared {
            meta_client,
            config: config.clone(),
            state: AsyncVariable::default(),
            camera_config_state: AsyncVariable::new(CameraConfigState {
                camera_config: config.initial_camera_config().clone(),
                camera_config_epoch: 1,
                active_camera_id: None,
            }),
            merged_blobs: BroadcastChannel::default(),
            tracked_points: BroadcastChannel::default(),
        });

        resources.spawn_interruptable("resolver", Self::service_resolver_thread(shared.clone())).await;
        resources.spawn_interruptable("merger", Self::frame_merger_thread(shared.clone())).await;

        if shared.config.make_dummy_cameras() {
            assert!(shared.config.camera_service().is_empty());
        }

        for per_cam in config.per_camera() {
            let camera_id = entity_id_from_string(per_cam.camera_id_str()).unwrap();

            lock!(state <= shared.state.lock().await?, {
                if per_cam.has_intrinsics() {
                    state.camera_intrinsics.insert(camera_id, intrinsics_from_proto(per_cam.intrinsics()));
                }

                if per_cam.has_extrinsics() {
                    state.camera_extrinsics.insert(camera_id, extrinsics_from_proto(per_cam.extrinsics()));
                }
            });

            if shared.config.make_dummy_cameras() {
                let camera_stub = Arc::new(MocapCameraStub::new(
                    Arc::new(rpc::LocalChannel::new(
                        Arc::new(mocap_camera::DummyMocapCamera::create(2).await?).into_service()
                    ))
                ));

                let ptp_stub = Arc::new(TimeSyncStub::new(
                    Arc::new(rpc::LocalChannel::new(
                        Arc::new(ptp::DummyTimeSyncNode::create()).into_service()
                    ))
                ));

                lock!(state <= shared.state.lock().await?, {
                    let task = ChildTask::spawn(Self::camera_thread(
                        shared.clone(),
                        camera_id,
                        ptp_stub.clone(),
                        camera_stub.clone()
                    ));

                    state.cameras.insert(camera_id, CameraEntry {
                        worker_address: String::new(),
                        camera_stub,
                        ptp_stub,
                        ptp_leader: false,
                        task: Some(task),
                        status: None,
                        camera_config_epoch: 0,
                    });
                });
            }
        }

        // NOTE: Should be done after the extrinsics/intrinscs are setup.
        resources.spawn_interruptable("matcher", Self::matching_task(shared.clone())).await;

        Ok(Self {
            resources,
            shared
        })
    }

    pub async fn status(&self) -> Result<MocapManagerStatus> {
        Self::status_inner(&self.shared).await
    }

    async fn status_inner(shared: &Shared) -> Result<MocapManagerStatus> {
        let mut proto = MocapManagerStatus::default();

        let state = shared.state.lock().await?.read_exclusive();
        let camera_config_state = shared.camera_config_state.lock().await?.read_exclusive();

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

                if let Some(extrinsics) = &state.camera_extrinsics.get(camera_id) {
                    proto.set_extrinsics(extrinsics_to_proto(extrinsics));
                }

                proto.set_active(camera_config_state.active_camera_id == Some(*camera_id));
            }

            if proto.groups().is_empty() {
                if let Some(camera_status) = &entry.status {
                    let proto = proto.new_groups();

                    // Need at least one camera to become fully configured to get a valid config set.
                    // (mainly since we need to grab control values and such that are merged on the first config.)
                    if entry.camera_config_epoch != 0 {
                        proto.set_config(camera_config_state.camera_config.clone());
                        proto.set_camera_controls(camera_status.status.camera_controls().clone());                            
                    }

                    proto.set_sensor(camera_status.status.sensor().clone());
                }
            }
        }

        proto.cameras_mut().sort_by_key(|c| c.id());

        if state.calibration.is_some() {
            proto.calibration_mut();
        }

        Ok(proto)
    }

    pub async fn execute(&self, req: &ExecuteRequest) -> Result<ExecuteResponse> {
        lock!(state <= self.shared.state.lock().await?, {
            if req.start_calibration() {
                if state.calibration.is_some() {
                    return Err(err_msg("Calibration already started"));
                }

                state.calibration = Some(CalibrationState {
                    stopping: false,
                    task: ChildTask::spawn(Self::calibration_task(self.shared.clone()))
                });
            }

            if req.stop_calibration() {
                if let Some(c) = &mut state.calibration {
                    println!("Stopping calibration!");
                    c.stopping = true;
                }
            }


            Result::<_, Error>::Ok(())
        })?;

        if req.has_configure_cameras() {
            lock!(camera_config_state <= self.shared.camera_config_state.lock().await?, {
                camera_config_state.camera_config = req.configure_cameras().clone();
                camera_config_state.camera_config_epoch += 1;
                camera_config_state.notify_all();
            });
        }

        if req.has_select_camera() {
            lock!(camera_config_state <= self.shared.camera_config_state.lock().await?, {
                camera_config_state.active_camera_id = Some(req.select_camera());
                camera_config_state.camera_config_epoch += 1;
                camera_config_state.notify_all();
            });
        }

        Ok(ExecuteResponse::default())
    }

    /// Global (per-manager) thread that monitors the set of workers in the camera
    /// service. The jobs of this thread are to:
    /// - Maintain the State::cameras set (add new cameras / remove dead ones).
    /// - Maintain a PTP leader.
    async fn service_resolver_thread(shared: Arc<Shared>) -> Result<()> {
        if shared.config.camera_service().is_empty() {
            return Ok(());
        }

        let resolver = cluster_client::ServiceResolver::create(
            shared.config.camera_service(), shared.meta_client.clone()
        )?;

        loop {
            let endpoints = resolver.resolve().await?;
            if endpoints.is_empty() {
                // TODO: Instead rely on the resolver notifications
                executor::sleep(Duration::from_secs(1)).await?;
                continue;
            }

            let mut current_cameras = HashMap::new();
            for endpoint in endpoints {

                let host_name = match &endpoint.authority.host {
                    http::uri::Host::Name(v) => v.to_string(),
                    _ => return Err(err_msg("Unexpected host name format"))
                };

                let camera_id = match ServiceName::parse(&host_name)?.entity() {
                    ServiceEntity::Worker { worker_id, .. } => entity_id_from_string(&worker_id)
                        .ok_or_else(|| err_msg("Failed to parse worker_id format"))?,
                    _ => return Err(err_msg("Expected all service endpoints to be workers"))
                };

                current_cameras.insert(camera_id, host_name);
            }

            // Make sure all cameras have connections setup.

            // TODO: Make sure that leader selection is deterministic assuming all cameras are present.

            let mut have_ptp_leader = false;
            let mut new_cameras = vec![];
            lock!(state <= shared.state.lock().await?, {

                // Remove old cameras.
                // (the assumption is that they are dead so will give up their ptp leadership if needed)
                state.cameras.retain(|old_camera_id, old_entry| {
                    if !current_cameras.contains_key(old_camera_id) {
                        return false;
                    }

                    have_ptp_leader |= old_entry.ptp_leader;
                    true
                });

                for (camera_id, camera_endpoint) in current_cameras {
                    if !state.cameras.contains_key(&camera_id) {
                        new_cameras.push((camera_id, camera_endpoint));
                    }
                }

                // If there is no leader, always prioritize electing an existing camera
                // which likely has the most disciplined clock.
                if !have_ptp_leader && !state.cameras.is_empty() {
                    state.cameras.values_mut().next().unwrap().ptp_leader = true;
                    have_ptp_leader = true;
                }
            });


            let mut new_camera_entries = vec![];
            for (camera_id, endpoint) in new_cameras {

                // TODO: Ideally we'd just re-use the entire resolved endpoint data.
                // TODO: This also needs to factor in any custom port names.
                let channel = create_rpc_channel(
                    &endpoint,
                    shared.meta_client.clone()
                ).await?;

                let ptp_stub = Arc::new(TimeSyncStub::new(channel.clone()));
                let camera_stub = Arc::new(MocapCameraStub::new(channel.clone()));

                let ptp_leader = !have_ptp_leader;
                have_ptp_leader = true;

                new_camera_entries.push((camera_id, CameraEntry {
                    worker_address: endpoint,
                    ptp_stub,
                    camera_stub,
                    ptp_leader,
                    status: None,
                    camera_config_epoch: 0,

                    // NOTE: We only create this after the entry is added to the state
                    // to avoid race conditions.
                    task: None
                }));
            }

            if !new_camera_entries.is_empty() {
                lock!(state <= shared.state.lock().await?, {
                    for (camera_id, mut entry) in new_camera_entries {

                        entry.task = Some(ChildTask::spawn(Self::camera_thread(
                            shared.clone(),
                            camera_id,
                            entry.ptp_stub.clone(),
                            entry.camera_stub.clone()
                        )));

                        state.cameras.insert(camera_id, entry);
                    }
                });
            }

            // TODO: Instead rely on the resolver notifications
            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    /// Global (per-manager) thread that is responsible for publishing finalized groups of blobs for a
    /// frame from all cameras once all data is received or a timeout has occurred.
    async fn frame_merger_thread(shared: Arc<Shared>) -> Result<()> {

        let timeout = Duration::from_secs_f32(shared.config.frame_aggregation_timeout());

        loop {
            let mut state = shared.state.lock().await?.enter();
            if state.frames.is_empty() {
                state.wait().await;
                continue;
            }

            let now = Instant::now();

            let time_elapsed = now - state.frames[0].first_received;

            let complete = (
                state.frames[0].results.len() == state.cameras.len() ||
                time_elapsed >= timeout
            );

            if !complete {
                let _ = executor::timeout(timeout - time_elapsed, state.wait()).await;
                continue;
            }

            if time_elapsed > Duration::from_millis(5) {
                eprintln!("Took {:?} to gather all camera results", time_elapsed);
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
        camera_stub: Arc<MocapCameraStub>
    ) {

        loop {
            let mut bundle = TaskResultBundle::new();
            bundle.add("Status", Self::camera_status_monitor(shared.clone(), camera_id, ptp_stub.clone(), camera_stub.clone()));
            bundle.add("ReadBlobs", Self::camera_read_blobs_thread(shared.clone(), camera_id, camera_stub.clone()));

            let res = bundle.join().await;

            eprintln!("Camera {} Thread Terminated: {:?}", entity_id_to_string(camera_id).unwrap(), res);

            // TODO: Use exponential backoff.
            executor::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Per-camera thread which has two jobs:
    /// - Checks the status of the PTP and camera code.
    /// - Configures the camera once PTP is syncronized.
    async fn camera_status_monitor(
        shared: Arc<Shared>,
        camera_id: u64,
        ptp_stub: Arc<TimeSyncStub>,
        camera_stub: Arc<MocapCameraStub>
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
                eprintln!("Configuring PTP for camera...");

                if intended_ptp_config.role() == TimeSyncRole::LEADER {
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

            let mut last_configured_epoch = lock!(state <= shared.state.lock().await?, {
                let entry = match state.cameras.get_mut(&camera_id) {
                    Some(v) => v,
                    None => return 0
                };

                entry.camera_config_epoch
            });

            let next_config = lock!(config_state <= shared.camera_config_state.lock().await?, {

                if config_state.camera_config_epoch == last_configured_epoch {
                    return None;
                }

                let config = Self::get_camera_config(camera_id, &config_state);
                Some((config, config_state.camera_config_epoch))
            });

            
            if let Some((config, epoch)) = next_config {
                camera_stub.Configure(&request_context, &config).await.result?;
                last_configured_epoch = epoch;

                status = {
                    let req = StatusRequest::default();
                    camera_stub.Status(&request_context, &req).await.result?
                };

                // Pull any merge conflicts that were adjusted by the camera into our local copy of the config.
                // (the hope is that all cameras behave the same way)
                lock!(config_state <= shared.camera_config_state.lock().await?, {
                    config_state.camera_config = status.config().clone();
                });
            }

            lock!(state <= shared.state.lock().await?, {
                let entry = match state.cameras.get_mut(&camera_id) {
                    Some(v) => v,
                    None => return
                };

                entry.camera_config_epoch = last_configured_epoch;

                // TODO: Setting this is now generally sufficient for knowing if it is in sync since
                // future changes to the config also need to be accounted for.
                // 
                entry.status = Some(CameraStatus {
                    status,
                    ptp_status,
                    check_time: now
                });
            });


            let res = executor::timeout(
                Duration::from_secs(1),
                Self::wait_for_new_config_epoch(&shared, last_configured_epoch)
            ).await;
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
        lock!(state <= shared.state.lock().await?, {
            let entry = state.cameras.get(&camera_id)
                .ok_or_else(|| err_msg("Missing camera"))?;

            let mut ptp_config = TimeSyncNode::default_config()?;

            if !entry.ptp_leader {
                ptp_config.set_role(TimeSyncRole::FOLLOWER);
                return Ok(ptp_config);
            }

            ptp_config.set_role(TimeSyncRole::LEADER);
                
            for (follower_camera_id, entry) in &state.cameras {
                if *follower_camera_id == camera_id {
                    continue;
                }

                let proto = ptp_config.leader_mut().new_followers();
                proto.set_rpc_addr(format!("_rpc.{}", entry.worker_address));
                proto.set_ptp_addr(format!("_ptp.{}", entry.worker_address));
            }

            Ok(ptp_config)
        })
    }

    fn get_camera_config(camera_id: u64, config_state: &CameraConfigState) -> MocapCameraConfigureRequest {
        let mut config = config_state.camera_config.clone();
        for v in config.rgb_led_colors_mut() {
            if config_state.active_camera_id == Some(camera_id) {
                *v = 100; // blue at (100 / 255) intensity
            } else {
                // Weak green
                // TODO: Have this timeout on the camera if it hasn't been talked to in a while.
                *v = 20 << 8;
            }
        }

        config
    }

    /// Per-camera thread which handles continously calling ReadBlobs.
    ///
    /// TODO: Retry with exponential backoff.
    async fn camera_read_blobs_thread(
        shared: Arc<Shared>, camera_id: u64, stub: Arc<MocapCameraStub>
    ) -> Result<()> {
        let mut ctx = rpc::ClientRequestContext::default();
        ctx.http.wait_for_ready = true;

        let mut req = ReadBlobsRequest::default();

        let mut res_stream = stub.ReadBlobs(&ctx, &req).await;

        while let Some(mut res) = res_stream.recv().await {

            let rx_time = Instant::now();

            if res.cameras().len() != 1 {
                eprintln!("Bad blobs format");
                continue;
            }

            lock!(state <= shared.state.lock().await?, {

                // TODO: There are risks that a camera that is just starting to act poorly sends up very far in the 
                // future timestamps and messed up tracking for all other cameras.

                // TODO: Ignore cameras which aren't synced (though non-healthy ones shouldn't block frame completion).

                for i in 0..state.frames.len() {
                    let frame = &mut state.frames[i];

                    if res.frame_timestamp() < frame.timestamp {
                        eprintln!("Rejecting stale timestamp for new frame");
                        return;
                    }
                    
                    if res.frame_timestamp() == frame.timestamp {
                        if frame.results.contains_key(&camera_id) {
                            eprintln!("Duplicate blob data for frame");
                            return;
                        }

                        res.cameras_mut()[0].set_latency((rx_time - frame.first_received).as_nanos() as u64);
                        frame.results.insert(camera_id, res);

                        // TODO: Base this on the number of 'healthy' cameras.
                        if i == 0 && frame.results.len() == state.cameras.len() {
                            state.notify_all();
                        }
                        return;
                    }
                }

                if state.frame_timestamp_waterline >= res.frame_timestamp() {
                    eprintln!("Rejecting stale timestamp for new frame (2)");
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

    async fn calibration_task(shared: Arc<Shared>) {

        if let Err(e) = Self::calibration_task_inner(&shared).await {
            eprintln!("Calibration failed: {}", e);
        }

        lock!(state <= shared.state.lock().await.unwrap(), {
            state.calibration = None;
        });
    }

    async fn calibration_task_inner(shared: &Shared) -> Result<()> {

        let mut writer = RecordWriter::create_new(project_path!("data/mocap/calibration.log")).await?;

        {
            let mut entry = MocapLogEntry::default();
            entry.set_system_state(Self::status_inner(shared).await?);
            writer.append(&entry.serialize()?).await?;
        }

        let mut subscriber = shared.merged_blobs.subscribe(1024);

        loop {
            // TODO: Allow this to interrupt the recv()
            let stopping = lock!(state <= shared.state.lock().await?, {
                if let Some(c) = &state.calibration {
                    c.stopping
                } else {
                    true
                }
            });

            if stopping {
                break;
            }

            let res = subscriber.recv().await?;

            let mut entry = MocapLogEntry::default();
            entry.set_blobs(res.as_ref().clone());
            writer.append(&entry.serialize()?).await?;
        }

        writer.flush().await?;
        drop(writer);

        println!("Done writing calibration data!");

        Ok(())
    }

    async fn matching_task(shared: Arc<Shared>) -> Result<()> {

        let mut params = vec![];

        lock!(state <= shared.state.lock().await?, {
            for (cam_id, extrinsics) in &state.camera_extrinsics {
                let intrinsics = match state.camera_intrinsics.get(cam_id) {
                    Some(v) => v,
                    None => continue
                };

                params.push(CameraParameters {
                    id: *cam_id,
                    extrinsics: extrinsics.clone(),
                    intrinsics: intrinsics.clone()
                });
            }
        });

        let mut matcher = BlobMatcher::new(shared.config.matching(), &params);

        let mut subscriber = shared.merged_blobs.subscribe(8);

        loop {
            let blobs_res = subscriber.recv().await?;

            let points = matcher.run(&blobs_res);

            let mut res = ReadTrackedPointsResponse::default();
            res.set_frame_timestamp(blobs_res.frame_timestamp());
            for pt in points {
                res.add_points(pt.to_proto());
            }

            shared.tracked_points.send(Arc::new(res));
        }



        Ok(())
    }
}

#[async_trait]
impl MocapManagerService for MocapManager {

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
}


