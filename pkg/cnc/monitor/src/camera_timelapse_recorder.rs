use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Instant};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::io::Writeable;
use executor::lock;
use executor::channel;
use executor::sync::{AsyncMutex, AsyncRwLock, AsyncVariable};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use file::{LocalPath, LocalPathBuf};
use media_camera::camera_manager::{CameraManager, CameraSubscriber};
use media_camera::frame::{ImageFormat, ImageFrame};
use video::mp4::{self, MP4Builder, MP4BuilderOptions};
use db_table::ProtobufDB;

use crate::tables::MediaFragmentTable;
use crate::{config::MachineConfigContainer, player::Player};



pub struct CameraTimelapseRecorder {
    task: TaskResource,
}

impl_resource_passthrough!(CameraTimelapseRecorder, task);

impl CameraTimelapseRecorder {
    pub fn create(
        camera_id: u64,
        mut camera_subscriber: CameraSubscriber,
        capture_event_receiver: channel::Receiver<()>,
    ) -> Result<Self> {
        let task = TaskResource::spawn_interruptable("CameraTimelapseRecorder", Self::run(
            camera_subscriber, capture_event_receiver));

        Ok(Self { task })
    }

    async fn run(
        mut camera_subscriber: CameraSubscriber,
        capture_event_receiver: channel::Receiver<()>,
    ) -> Result<()> {

        // TODO: Create the data/timelapse dir if it doesn't exist.

        let mut i = 0;
        while let Ok(()) = capture_event_receiver.recv().await {
            let frame = camera_subscriber.recv_new().await?;
            let data = frame.data.data().unwrap();
            file::write(file::project_dir().join("data/timelapse").join(format!("{:04}.jpg", i)), data).await?;
            println!("Captured timelapse frame: {}", i);
            i += 1;
        }

        // Player is done

        println!("Timelapse done!");

        Ok(())
    }
}