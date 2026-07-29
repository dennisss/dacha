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

// TODO: Ideally dedup this with the wanding code and allow this to run concurrently with the wanding stuff.

pub struct RecordingMode {
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
    result: Option<Result<(), ()>>
}

impl RecordingMode {

    // TODO: Ideally subscribe later after everything is ready for logging.
    pub fn create(
        data_dir: &LocalPath,
        initial_status: MocapManagerStatus,
        blobs_subscriber: BroadcastChannelSubscriber<Arc<ReadBlobsResponse>>,
        points_subscriber: BroadcastChannelSubscriber<Arc<ReadTrackedPointsResponse>>
    ) -> Result<Self> {

        let shared = Arc::<Shared>::default();

        let task = ChildTask::spawn(Self::background_thread(
            shared.clone(),
            data_dir.to_owned(),
            initial_status,
            blobs_subscriber,
            points_subscriber
        ));

        Ok(Self {
            shared,
            task
        })
    }

    pub fn to_proto(&self) -> RecordingState {
        let mut proto = RecordingState::default();

        self.shared.state.apply(|state| {
            // TODO: elapsed_time

            match &state.result {
                Some(Ok(solution)) => {
                    
                }
                Some(Err(())) => {
                    proto.set_error(true);
                }
                None => {}
            }

        }).unwrap();

        proto
    }

    /// Stops recording data.
    ///
    /// Blocks until all background threads are cleaned up.
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

    async fn background_thread(
        shared: Arc<Shared>,
        data_dir: LocalPathBuf,
        initial_status: MocapManagerStatus,
        blobs_subscriber: BroadcastChannelSubscriber<Arc<ReadBlobsResponse>>,
        points_subscriber: BroadcastChannelSubscriber<Arc<ReadTrackedPointsResponse>>
    ) {
        let r = Self::background_thread_inner(
            &shared,
            data_dir,
            initial_status,
            blobs_subscriber,
            points_subscriber
        ).await;

        if let Err(e) = r {
            eprintln!("Recording failed: {}", e);
            shared.state.apply(|state| {
                state.result = Some(Err(()));
            }).unwrap();
        }
    }

    // TODO: Read both subscribers.
    async fn background_thread_inner(
        shared: &Shared,
        data_dir: LocalPathBuf,
        initial_status: MocapManagerStatus,
        mut blobs_subscriber: BroadcastChannelSubscriber<Arc<ReadBlobsResponse>>,
        mut points_subscriber: BroadcastChannelSubscriber<Arc<ReadTrackedPointsResponse>>
    ) -> Result<()> {
        let run_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let log_dir = data_dir.join("recording");
        file::create_dir_all(&log_dir).await?;

        let mut log_writer = RecordWriter::create_new(
            log_dir.join(format!("{}.log", run_id))
        ).await?;

        {
            let mut entry = MocapLogEntry::default();
            entry.set_system_state(initial_status.clone());
            log_writer.append(&entry.serialize()?).await?;
        }

        loop {
            race!(
                async move { executor::sleep(Duration::from_millis(100)); },
                blobs_subscriber.wait(),
                points_subscriber.wait()
            );

            let stopping = shared.state.apply(|state| state.stopping)?;
            if stopping {
                break;
            }

            if let Some(res) = blobs_subscriber.try_recv() {
                let res = res?;

                let mut entry = MocapLogEntry::default();
                entry.set_blobs(res.as_ref().clone());
                log_writer.append(&entry.serialize()?).await?;
            }

            if let Some(res) = points_subscriber.try_recv() {
                let res = res?;

                let mut entry = MocapLogEntry::default();
                entry.set_points(res.as_ref().clone());
                log_writer.append(&entry.serialize()?).await?;
            }

        }

        log_writer.flush().await?;
        drop(log_writer);

        println!("Done writing recording data!");

        shared.state.apply(|state| {
            state.result = Some(Ok(()));
        })?;

        Ok(())
    }


}

