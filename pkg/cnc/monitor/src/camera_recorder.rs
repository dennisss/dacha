use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Instant};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::io::Writeable;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncRwLock, AsyncVariable};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use file::{LocalPath, LocalPathBuf};
use media_web::camera_manager::{CameraFrameData, CameraManager, CameraSubscriber};
use video::mp4::{self, MP4Builder, MP4BuilderOptions};

use crate::db::ProtobufDB;
use crate::tables::MediaFragmentTable;
use crate::{config::MachineConfigContainer, player::Player};

const MAX_FRAGMENT_FRAMES: usize = 30 * 10; // 10 seconds at 30 fps.

const MAX_SEGMENT_SIZE: u64 = 64 * 1024 * 1024; // 64 MiB (~80 seconds)

/// Records camera data to disk (and writes MediaFragment entries to a database
/// for future lookup).
///
/// For recording to progress, the user must continously call record_step() and
/// eventually finish() to flush the last frames to disk.
pub struct CameraRecorder {
    camera_id: u64,
    camera_subscriber: CameraSubscriber,
    camera_data_dir: LocalPathBuf,
    db: Arc<ProtobufDB>,

    first_frame_time: Option<FrameTimestamp>,

    /// Next expected frame sequence number.
    next_frame_sequence_number: Option<u32>,

    mp4_builder: Option<MP4Builder>,

    /// Position of the last init data segment for upcoming media data.
    init_data: Option<MediaSegmentData>,

    current_segment: Option<CurrentSegment>,
}

struct FrameTimestamp {
    realtime: SystemTime,
    monotonic: Duration,
}

struct CurrentSegment {
    /// Index of the segment starting at 0. Corresponds to the segment_index
    /// emitted by the MP4Builder.
    index: usize,

    /// Unique id for this segment. Derived from the creation timestamp.
    id: u64,

    /// TODO: Can optimize this instance for append only operations with only
    /// occasional flushes.
    file: file::LocalFile,
}

impl CameraRecorder {
    pub async fn create(
        camera_id: u64,
        camera_subscriber: CameraSubscriber,
        db: Arc<ProtobufDB>,
        data_dir: &LocalPath,
    ) -> Result<Self> {
        // TODO: Skip some initial frames.

        let camera_data_dir = data_dir.join(format!("{:08x}", camera_id));

        file::create_dir_all(&camera_data_dir).await?;

        Ok(Self {
            camera_id,
            camera_subscriber,
            camera_data_dir,
            db,
            first_frame_time: None,
            next_frame_sequence_number: None,
            mp4_builder: None,
            init_data: None,
            current_segment: None,
        })
    }

    /// Gets one frame from the camera and performs any necessary recording
    /// actions.
    pub async fn record_step(&mut self) -> Result<()> {
        // TODO: Need some timeouts on how long we are willing to wait for a frame.

        // TODO: Implement pipelining of the recv, file write and database insert calls.

        let frame = self.camera_subscriber.recv().await?;

        // TODO: Provide some resilience to this but still print out a warning about
        // number of skipped frames.
        if self.next_frame_sequence_number.unwrap_or(frame.sequence) != frame.sequence {
            // TODO: PRint out how many we missed
            return Err(err_msg("Missed some frames while recording"));
        }

        self.next_frame_sequence_number = Some(frame.sequence + 1);

        self.record_data(Some(frame)).await
    }

    async fn record_data(&mut self, frame: Option<CameraFrameData>) -> Result<()> {
        let mp4_builder = match &mut self.mp4_builder {
            Some(v) => v,
            None => {
                let frame = match &frame {
                    Some(v) => v,
                    None => return Ok(()),
                };

                let mut options = MP4BuilderOptions::default();
                options.fragment = Some(MAX_FRAGMENT_FRAMES);
                options.max_segment_size = Some(MAX_SEGMENT_SIZE);
                options.independent_segments = true;
                options.skip_to_key_frame = true;

                self.mp4_builder.insert(MP4Builder::new(
                    frame.format.width,
                    frame.format.height,
                    frame.format.framerate,
                    options,
                )?)
            }
        };

        let first_frame_time = match &self.first_frame_time {
            Some(time) => time,
            None => {
                let frame = match &frame {
                    Some(v) => v,
                    None => return Ok(()),
                };

                self.first_frame_time.insert(FrameTimestamp {
                    realtime: SystemTime::now(),
                    monotonic: frame.monotonic_timestamp,
                })
            }
        };

        if let Some(frame) = frame {
            mp4_builder.append(&frame.data, Some(frame.monotonic_timestamp), false)?;
        } else {
            mp4_builder.append(b"", None, true)?;
        }

        while let Some(event) = mp4_builder.consume() {
            let segment = match &mut self.current_segment {
                Some(segment) => {
                    if segment.index == event.segment_index {
                        segment
                    } else {
                        // Finalize the previous segment.
                        segment.file.flush().await?;

                        let next_segment =
                            Self::create_segment(&self.camera_data_dir, event.segment_index)?;
                        self.current_segment.insert(next_segment)
                    }
                }
                None => {
                    let next_segment =
                        Self::create_segment(&self.camera_data_dir, event.segment_index)?;
                    self.current_segment.insert(next_segment)
                }
            };

            let start_offset = segment.file.current_position();
            segment.file.write_all(&event.data).await?;
            let end_offset = segment.file.current_position();

            let mut data_proto = MediaSegmentData::default();
            data_proto.set_segment_id(segment.id);
            data_proto.byte_range_mut().set_start(start_offset);
            data_proto.byte_range_mut().set_end(end_offset);

            if event.is_init {
                self.init_data = Some(data_proto);
            } else {
                let init_data = self
                    .init_data
                    .clone()
                    .ok_or_else(|| err_msg("No init data before fragment"))?;

                let time_range = event
                    .time_range
                    .as_ref()
                    .ok_or_else(|| err_msg("Fragment missing time range"))?;

                let user_time_range = event
                    .user_time_range
                    .ok_or_else(|| err_msg("No user time range in fragment"))?;

                let mut fragment_proto = MediaFragment::default();
                fragment_proto.set_camera_id(self.camera_id);

                // TODO: Switch to using base_time for this so that time realtime corrections
                // are done more rigorously.
                let start_time = (first_frame_time.realtime
                    + (user_time_range.start - first_frame_time.monotonic))
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap();
                let end_time = start_time + (user_time_range.end - user_time_range.start);

                fragment_proto.set_start_time(start_time.as_micros() as u64);
                fragment_proto.set_end_time(end_time.as_micros() as u64);

                fragment_proto.set_relative_time(time_range.start.as_micros() as u64);

                fragment_proto.set_init_data(init_data);
                fragment_proto.set_data(data_proto);

                fragment_proto.set_mime_type(mp4_builder.mime_type()?);

                self.db
                    .insert::<MediaFragmentTable>(&fragment_proto)
                    .await?;
            }
        }

        Ok(())
    }

    fn create_segment(camera_data_dir: &LocalPath, segment_index: usize) -> Result<CurrentSegment> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        // TODO: Just hex encode the number.
        let path = camera_data_dir.join(format!("{}.mp4", timestamp));

        let file = file::LocalFile::open_with_options(
            path,
            file::LocalFileOpenOptions::new()
                .create_new(true)
                .write(true),
        )?;

        Ok(CurrentSegment {
            index: segment_index,
            id: timestamp,
            file,
        })
    }

    /// Stops recording and flushes any in-memory state to disk.
    pub async fn finish(mut self) -> Result<()> {
        // Flush any pending MP4 state.
        self.record_data(None).await?;

        if let Some(mut segment) = self.current_segment {
            segment.file.flush().await?;
        }

        Ok(())
    }
}
