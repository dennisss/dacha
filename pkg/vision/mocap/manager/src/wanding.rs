use std::sync::Arc;
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::future::Future;

use common::errors::*;
use common::hash::*;
use executor::sync::SyncMutex;
use executor::child_task::ChildTask;
use mocap_proto::mocap::*;
use file::{LocalPathBuf, LocalPath};
use image::Image;
use vision::*;
use math_proto_util::VectorProtoExt;
use executor_multitask::BroadcastChannelSubscriber;
use sstable::record_log::*;
use protobuf::Message;

use crate::calibration::*;


/*

Bascially have a thread to do the following:

- Wait for next data entry.
    - Write to log file
    - Detect wand
    - Dedup wand
    - Write to output debug image.
    - Encode the output debug image.

My display size is 302 x 189

I will render 4x downsampled (480 x 300)

*/


pub struct WandingCalibrationMode {
    shared: Arc<Shared>,
    task: ChildTask
}

#[derive(Default)]
struct Shared {
    state: SyncMutex<State>,
}

#[derive(Default)]
struct State {
    stopping: bool,
    stats: WandingCalibrationStats,
    result: Option<Result<WandingCalibrationSolution, ()>>,

    refine_requested: bool,
    refinement: Option<Result<WandRefinement, ()>>,
}

impl WandingCalibrationMode {

    // TODO: Ideally subscribe later after everything is ready for logging.
    pub fn create(
        config: MocapManagerConfig,
        camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
        data_dir: &LocalPath,
        initial_status: MocapManagerStatus,
        subscriber: BroadcastChannelSubscriber<Arc<ReadBlobsResponse>>
    ) -> Result<Self> {

        let shared = Arc::<Shared>::default();

        let task = ChildTask::spawn(Self::background_thread(
            shared.clone(),
            config,
            camera_intrinsics,
            data_dir.to_owned(),
            initial_status,
            subscriber
        ));

        Ok(Self {
            shared,
            task
        })
    }

    pub fn to_proto(&self) -> WandingCalibrationState {
        let mut proto = WandingCalibrationState::default();
        self.shared.state.apply(|state| {
            proto.set_stats(state.stats.clone());

            match &state.result {
                Some(Ok(solution)) => {
                    let result = proto.result_mut();
                    result.set_error(solution.error);
                    result.set_refining(state.refine_requested && state.refinement.is_none());

                    match &state.refinement {
                        Some(Ok(refinement)) => {
                            result.set_refinement(refinement.to_proto());
                        }
                        Some(Err(())) => {
                            result.refinement_mut().set_failed(true);
                        }
                        None => {}
                    }
                }
                Some(Err(())) => {
                    proto.result_mut().set_failed(true);
                }
                None => {}
            }

        }).unwrap();
        proto
    }

    /// Stops recording data and proceeds with finishing the calibration.
    ///
    /// Blocks until the process is complete.
    pub fn finish(&self) -> impl Future<Output=()> + 'static {
        self.shared.state.apply(|state| {
            state.stopping = true;
        }).unwrap();

        let shared = self.shared.clone();

        async move {
            loop {
                let done = shared.state.apply(|state| state.result.is_some()).unwrap();
                if done {
                    break;
                }

                let _ = executor::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    /// After processing, starts measuring the wand marker positions.
    ///
    /// The returned future blocks until that is complete.
    pub fn refine(&self) -> Result<impl Future<Output=()> + 'static> {
        self.shared.state.apply(|state| {
            match &state.result {
                Some(Ok(_)) => {}
                _ => return Err(err_msg("Wanding calibration not processed yet")),
            }

            state.refine_requested = true;
            Ok(())
        })??;

        let shared = self.shared.clone();

        Ok(async move {
            loop {
                let done = shared.state.apply(|state| state.refinement.is_some()).unwrap();
                if done {
                    break;
                }

                let _ = executor::sleep(Duration::from_millis(100)).await;
            }
        })
    }

    /// Gets the config patch to apply. If the wand was refined, this also
    /// includes the measured wand marker positions.
    pub fn result(&self) -> Option<MocapManagerConfig> {
        let (result, refinement) = self.shared.state.apply(|state| {
            (state.result.clone(), state.refinement.clone())
        }).unwrap();

        let (params, wand_points) = match (result, refinement) {
            (Some(Ok(_)), Some(Ok(refinement))) => {
                (refinement.solution.params, Some(refinement.refined_points))
            }
            (Some(Ok(solution)), _) => (solution.params, None),
            _ => return None
        };

        // TODO: Ideally delete any data on cameras not involved in the wanding to keep the config from expanding forever.

        let mut patch = MocapManagerConfig::default();

        for params in params {
            let cam = patch.new_per_camera();
            cam.set_camera_id(params.id);
            cam.set_intrinsics(params.intrinsics.to_proto());
            cam.set_extrinsics(params.extrinsics.to_proto());
        }

        if let Some(points) = wand_points {
            for p in points {
                patch.wand_mut().add_points(p.to_proto());
            }
        }

        Some(patch)
    }

    async fn background_thread(
        shared: Arc<Shared>,
        config: MocapManagerConfig,
        camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
        data_dir: LocalPathBuf,
        initial_status: MocapManagerStatus,
        subscriber: BroadcastChannelSubscriber<Arc<ReadBlobsResponse>>
    ) {
        let r = Self::background_thread_inner(
            &shared,
            config,
            camera_intrinsics,
            data_dir,
            initial_status,
            subscriber
        ).await;

        if let Err(e) = r {
            eprintln!("Wanding calibration failed: {}", e);
            shared.state.apply(|state| {
                state.result = Some(Err(()));
            }).unwrap();
        }
    }

    async fn background_thread_inner(
        shared: &Shared,
        config: MocapManagerConfig,
        camera_intrinsics: HashMap<u64, CameraIntrinsicsModel, FastHasherBuilder>,
        data_dir: LocalPathBuf,
        initial_status: MocapManagerStatus,
        mut subscriber: BroadcastChannelSubscriber<Arc<ReadBlobsResponse>>
    ) -> Result<()> {
        let run_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let mut calibrator = WandingCalibrationSolver::new(
            config,
            camera_intrinsics
        );
        calibrator.set_initial_status(initial_status.clone());

        let log_dir = data_dir.join("recording");
        file::create_dir_all(&log_dir).await?;

        let mut log_writer = RecordWriter::create_new(
            log_dir.join(format!("{}-wanding.log", run_id))
        ).await?;

        {
            let mut entry = MocapLogEntry::default();
            entry.set_system_state(initial_status.clone());
            log_writer.append(&entry.serialize()?).await?;
        }

        loop {
            let r = executor::timeout(
                Duration::from_millis(100),
                subscriber.wait()
            ).await;

            let stopping = shared.state.apply(|state| state.stopping)?;
            if stopping {
                break;
            }

            // Wait more if no data was available yet.
            if !r.is_ok() {
                continue;
            }

            // This should return immediately since we used subscriber.wait() above. 
            let res = subscriber.recv().await?;

            let mut entry = MocapLogEntry::default();
            entry.set_blobs(res.as_ref().clone());

            calibrator.add_frame(&entry)?;

            shared.state.apply(|state| { state.stats = calibrator.stats(); })?;

            log_writer.append(&entry.serialize()?).await?;
        }

        log_writer.flush().await?;
        drop(log_writer);

        println!("Done writing calibration data!");

        // TODO: Make this interruptable.
        let solution = calibrator.solve()?;

        shared.state.apply(|state| {
            state.result = Some(Ok(solution));
        })?;

        // Wait for a refine request (this task is dropped once the calibration
        // is applied or cancelled).
        loop {
            if shared.state.apply(|state| state.refine_requested)? {
                break;
            }

            let _ = executor::sleep(Duration::from_millis(100)).await;
        }

        let refinement = calibrator.refine();
        if let Err(e) = &refinement {
            eprintln!("Wand refinement failed: {}", e);
        }

        shared.state.apply(|state| {
            state.refinement = Some(refinement.map_err(|_| ()));
        })?;

        Ok(())
    }


}

