use std::time::Duration;
use std::{sync::Arc, time::Instant};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncRwLock, AsyncVariable};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use file::LocalPathBuf;
use media_web::camera_manager::CameraManager;

use crate::camera_recorder::CameraRecorder;
use crate::devices::AvailableDevice;
use crate::{config::MachineConfigContainer, player::Player, protobuf_table::ProtobufDB};

// TODO: This needs to emit state change events.

// TODO: Allow recording failures to optionally stop the player.

/// Component which manages the recording of camera frames for a single camera
/// to disk while a machine is running.
///
/// This component lives for the entire life of a single camera config entry in
/// the MachineConfig and will transition between idle/recording states as
/// needed.
///
/// When recording:
/// - Segments are written into the filesystem at paths of the form:
///   - './camera/[camera_id]/[timestamp].mp4'
///   - '[timestamp]' is mainly used a monotonically increasing number that is
///     greater than all previous files.
///   - The first 'init' segment in each contiguous recording span will contain
///     no video data.
///   - Later 'data' segments will be up to 64 MiB of video data (~80 seconds of
///     video).
/// - Individual 'data' segments will be composed of fragments of length 10
///   seconds.
///   - Each fragment will have its own MediaFragment row recorded in the
///     database.
///   - TODO: Only write to the database once we have fully flushed the data to
///     disk.
pub struct CameraController {
    shared: Arc<Shared>,
    task: TaskResource,
}

impl_resource_passthrough!(CameraController, task);

struct Shared {
    machine_config: Arc<AsyncRwLock<MachineConfigContainer>>,
    camera_manager: Arc<CameraManager>,
    camera_id: u64,
    camera_device: AvailableDevice,
    db: Arc<ProtobufDB>,
    data_dir: LocalPathBuf,
    state: AsyncVariable<State>,
}

#[derive(Default)]
struct State {
    /// The current player instance.
    player: Option<Arc<Player>>,

    // TODO: Implement me.
    recording: bool,

    /// If true, the controller has entered a terminal state and can't process
    /// any more data.
    ///
    /// TODO: Implement me.
    stopped: bool,

    /// If set, the user has requested that we record until at least this time
    /// (regardless of whether or not the player is active).
    record_until: Option<Instant>,
}

impl CameraController {
    pub fn create(
        machine_id: u64,
        camera_id: u64,
        camera_manager: Arc<CameraManager>,
        camera_device: AvailableDevice,
        machine_config: Arc<AsyncRwLock<MachineConfigContainer>>,
        data_dir: LocalPathBuf,
        db: Arc<ProtobufDB>,
    ) -> Self {
        let shared = Arc::new(Shared {
            machine_config,
            camera_device,
            camera_manager,
            camera_id,
            data_dir,
            db,
            state: AsyncVariable::default(),
        });

        // TODO: Should handle cancellation tokens.
        let task =
            TaskResource::spawn_interruptable("CameraController", Self::run_thread(shared.clone()));

        Self { shared, task }
    }

    /// Returns whether or not frames are currently being recorded to disk from
    /// the camera.
    pub async fn recording(&self) -> Result<bool> {
        lock!(state <= self.shared.state.lock().await?, {
            Ok(state.recording)
        })
    }

    /// Changes which player instance should be watched for play/pause/stop
    /// transitions to guide recording.
    pub async fn set_current_player(&self, player: Option<Arc<Player>>) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            state.player = player;
            state.notify_all();
            Ok(())
        })
    }

    /// Should be called by the user immediately before play() is run on the
    /// player. This pre-emptively starts up the recording and waits for it to
    /// be initialized so that it is ready before playing starts.
    pub async fn pre_play(&self) -> Result<()> {
        // Don't do anything if the camera isn't configured to record while playing.
        {
            let config = self.shared.machine_config.read().await?;

            let camera_config = match config
                .cameras()
                .iter()
                .find(|c| c.id() == self.shared.camera_id)
            {
                Some(v) => v,
                None => return Ok(()),
            };

            if !camera_config.record_while_playing() {
                return Ok(());
            }
        }

        self.start_recording(Instant::now() + Duration::from_secs(30))
            .await
    }

    /// If not already recording, request that the recorder start playing.
    ///
    /// We will continue recording at least until 'until' time.
    ///
    /// Blocks until we have started recorded or the recorder failed.
    pub async fn start_recording(&self, until: Instant) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            state.record_until = Some(until);
            state.notify_all();
        });

        loop {
            let state = self.shared.state.lock().await?.read_exclusive();

            // TODO: There is no guarantee that the 'recording' status is fresh. Ideally
            // check against some timestamp.
            if state.stopped || state.recording {
                return Ok(());
            }

            state.wait().await;
        }
    }

    async fn run_thread(shared: Arc<Shared>) -> Result<()> {
        let res = Self::run_thread_inner(&shared).await;

        println!("Camera Controller exited!");

        lock!(state <= shared.state.lock().await?, {
            state.stopped = true;
            state.notify_all();
        });

        res
    }

    async fn run_thread_inner(shared: &Shared) -> Result<()> {
        /*
        TODO: Conduct an initial test to verify that the camera is working (receiving valid non-black frames)
        */

        let mut recorder: Option<CameraRecorder> = None;

        loop {
            let now = Instant::now();

            let should_record = {
                let mut state = shared.state.lock().await?.enter();

                let should_record = Self::should_be_recording(shared, &state)
                    .await
                    .unwrap_or(false);

                if should_record == recorder.is_some() {
                    // TODO: Also record a time.
                    state.recording = should_record;
                    state.notify_all();

                    if !should_record {
                        // TODO: Must also wait for machine change events so that we can detector
                        // player state changes (when pre_play isn't used).
                        executor::timeout(Duration::from_secs(2), state.wait()).await;
                        continue;
                    }
                }

                state.exit();

                should_record
            };

            if should_record {
                let recorder = match &mut recorder {
                    Some(v) => v,
                    None => {
                        let camera_subscriber = shared
                            .camera_device
                            .open_as_camera(&shared.camera_manager)
                            .await?;

                        recorder.insert(
                            CameraRecorder::create(
                                shared.camera_id,
                                camera_subscriber,
                                shared.db.clone(),
                                &shared.data_dir,
                            )
                            .await?,
                        )
                    }
                };

                for i in 0..30 {
                    recorder.record_step().await?;
                }
            } else {
                if let Some(recorder) = recorder.take() {
                    recorder.finish().await?;
                }
            }
        }
    }

    async fn should_be_recording(shared: &Shared, state: &State) -> Result<bool> {
        let now = Instant::now();
        if let Some(t) = state.record_until {
            if t > now {
                return Ok(true);
            }
        }

        let config = shared.machine_config.read().await?;

        let camera_config = match config.cameras().iter().find(|c| c.id() == shared.camera_id) {
            Some(v) => v,
            None => return Ok(false),
        };

        if let Some(player) = &state.player {
            let player_state = player.state_proto().await?;

            if (player_state.status() == RunningProgramState_PlayerState::PLAYING
                || player_state.status() == RunningProgramState_PlayerState::PAUSING)
                && camera_config.record_while_playing()
            {
                return Ok(true);
            }

            if (player_state.status() == RunningProgramState_PlayerState::PAUSED)
                && camera_config.record_while_paused()
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
