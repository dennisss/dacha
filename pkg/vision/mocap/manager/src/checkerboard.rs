use std::sync::Arc;
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use std::collections::{HashMap, VecDeque};
use std::future::Future;

use common::errors::*;
use executor::sync::SyncMutex;
use executor::child_task::ChildTask;
use executor::channel::oneshot;
use mocap_proto::mocap::*;
use file::{LocalPathBuf, LocalPath};
use math::matrix::Vector2d;
use cluster_client::id::entity_id_to_string;
use image::Image;
use vision::*;
use math_proto_util::VectorProtoExt;


/// This manages all the state associated with the checkerboard calibration
/// workflow for calibrating the intrinsics of a single camera.
pub struct CheckerboardCalibrationMode {
    shared: Arc<Shared>
}

struct Shared {
    config: CheckerboardCalibrationConfig,

    camera_id: u64,

    run_id: u64,

    data_dir: LocalPathBuf,

    output_dir: LocalPathBuf,

    state: SyncMutex<State>
}

struct State {
    frames: Vec<CheckerboardFrame>,

    result: Option<CameraIntrinsicsSolution>,

    /// 
    task: Option<ChildTask>,
}

#[derive(Clone)]
struct CheckerboardFrame {
    points_2d: Option<Vec<Vector2d>>,
    image_path: LocalPathBuf, 
}

impl CheckerboardCalibrationMode {

    pub fn create(
        config: &CheckerboardCalibrationConfig,
        camera_id: u64,
        data_dir: &LocalPath
    ) -> Self {
        let run_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let output_dir = data_dir
            .join("checkerboard")
            .join(entity_id_to_string(camera_id).unwrap()).join(run_id.to_string());

        Self {
            shared: Arc::new(Shared {
                config: config.clone(),
                camera_id,
                run_id,
                data_dir: data_dir.to_owned(),
                output_dir,
                state: SyncMutex::new(State {
                    frames: vec![],
                    result: None,
                    task: None
                })
            })
        }
    }

    pub fn camera_id(&self) -> u64 {
        self.shared.camera_id
    }

    pub fn result(&self) -> Option<CameraIntrinsicsSolution> {
        self.shared.state.apply(|state| {
            state.result.clone()
        }).unwrap()
    }

    pub fn to_proto(&self) -> CheckerboardCalibrationState {
        let mut proto = CheckerboardCalibrationState::default();
        proto.set_camera_id(self.shared.camera_id);
        proto.set_run_id(self.shared.run_id);

        self.shared.state.apply(|state| {

            for frame in &state.frames {
                let proto = proto.new_frames();
                proto.set_image_path(format!("/data/{}", frame.image_path.strip_prefix(&self.shared.data_dir).unwrap().as_str()));
                if let Some(pts) = &frame.points_2d {
                    for pt in pts {
                        proto.add_points_2d(pt.to_proto());
                    }
                }
            }

            if let Some(res) = &state.result {
                proto.result_mut().set_error(res.error);
                proto.result_mut().set_intrinsics(res.intrinsics.to_proto());
            }
        }).unwrap();

        proto
    }

    fn start_task<F: FnOnce() -> Fut + Send + 'static, Fut: Future<Output=Result<()>> + Send + 'static>(
        &self, f: F
    ) -> impl Future<Output=Result<()>> + 'static {
        let (sender, receiver) = oneshot::channel();

        self.shared.state.apply(move |state| {
            if state.task.is_some() {
                sender.send(Err(err_msg("Calibration currently busy")));
                return;
            }

            let shared = self.shared.clone();
            state.task = Some(ChildTask::spawn(async move {
                let res = f().await;

                shared.state.apply(|state| {
                    sender.send(res);
                    state.task = None;
                }).unwrap();
            }))
        }).unwrap();;

        async move {
            receiver.recv().await.map_err(|_| err_msg("Task failure"))?
        }
    }

    pub fn capture_frame(&self, camera_stub: Arc<MocapCameraStub>) -> impl Future<Output=Result<()>> + 'static {    
        let shared = self.shared.clone();
        self.start_task(move || Self::capture_frame_task(shared, camera_stub))
    }

    async fn capture_frame_task(
        shared: Arc<Shared>,
        camera_stub: Arc<MocapCameraStub>,
    ) -> Result<()> {

        let grid_width = 8;
        let grid_height = 13;

        let req = ReadFramesRequest::default();
        let ctx = rpc::ClientRequestContext::default();

        let mut res_stream = camera_stub.ReadFrames(&ctx, &req).await;

        let res = match res_stream.recv().await {
            Some(v) => v,
            None => {
                res_stream.finish().await?;
                return Err(err_msg("Unexpected end to frames stream"))
            }
        };

        let img = Image::<u8>::parse_from(res.mjpeg())?;

        let points_2d = detect_checkboard(&img, grid_width, grid_height).await.points;

        file::create_dir_all(&shared.output_dir).await?;

        let idx = shared.state.apply(|state| {
            state.frames.len()
        }).unwrap();

        let image_path = shared.output_dir.join(&format!("{:04}.jpg", idx));
        file::write(&image_path, res.mjpeg()).await?;

        shared.state.apply(|state| {
            state.frames.push(CheckerboardFrame {
                points_2d,
                image_path
            });
        }).unwrap();

        Ok(())
    }

    pub fn process_data(&self) -> impl Future<Output=Result<()>> + 'static {
        let shared = self.shared.clone();
        self.start_task(move || Self::process_data_task(shared))
    }

    async fn process_data_task(shared: Arc<Shared>) -> Result<()> {
        let config = &shared.config;

        let points_3d = generate_checkerboard_grid_3d(
            config.grid_width() as usize, config.grid_height() as usize, config.square_size()
        );

        let mut initial_intrinsics = CameraIntrinsicsModel::from_nominal_params(
            1920,
            1200,
            millis(3.6),
            micros(3.),
        );

        let mut solver = CameraInstrinsicsSolver::new(&initial_intrinsics);

        let frames = shared.state.apply(|state| state.frames.clone())?;

        // TODO: Verify that we have at least a few frames with points.
        for frame in frames {
            let points_2d = match frame.points_2d.as_ref() {
                Some(v) => v,
                None => continue
            };

            solver.add_object(
                &points_3d,
                points_2d,
            );
        }

        // TODO: Ideally run this on a separate thread or at least make it more easily interruptable.
        let solution = solver.solve();

        shared.state.apply(|state| {
            state.result = Some(solution);
        })?;

        Ok(())
    }
}