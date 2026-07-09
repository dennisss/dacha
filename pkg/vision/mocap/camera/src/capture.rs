use std::sync::Arc;
use std::collections::VecDeque;
use std::time::{Instant, Duration};

use common::bytes::Bytes;
use common::errors::*;
use executor::sync::AsyncMutex;
use executor::{lock, lock_async};
use executor::channel;
use executor::channel::oneshot;
use executor_multitask::{impl_resource_passthrough, ServiceResource, ServiceResourceGroup, BroadcastChannel};
use mocap_proto::mocap::*;
use media_camera::v4l2::capture_buffer::CaptureDMABuffer;
use ptp::MonotonicClockTimeSyncer;
use mocap_camera_core::*;

use crate::inst::CameraSensorData;
use crate::timestamp::*;

/// This number must be at least equal to the maximum number fo processing threads + 2
/// to ensure that we can always feed v4l2 at max frame rate even if processing is slow
/// and is blocking access to most of the buffers.
const NUM_FRAME_BUFFERS: usize = 4;

const NUM_BLOB_PROCESSING_THREADS: usize = 2;

const NUM_MJPEG_THREADS: usize = 1;

// For an AR0234 at 119 FPS
// Will actually be 8.4ms, but we want to be a little conservative to be safe.
const FRAME_TRANSFER_TIME: Duration = Duration::from_micros(8600);

const NUM_FRAME_CHUNKS: usize = 8;

/// For a BCM2712. the number of bytes fetched into one line of the cache.
/// (the smallest cacheable and cache invalidatable segment of memory)
const CACHE_LINE_SIZE: usize = 64;


/// Handles all the frame buffer queueing and processing for the camera.
///
/// Internally:
/// - Processing is done directly on DMA buffers that are shared with the camera v4l2 driver.
/// - The typical flow of a frame is as follows:
///   - Enqueue DMA buffer to v4l2
///   - Get a FRAME_SYNC event indicating that the buffer is starting to be filled.
///   - Run pipelined blob processing while the buffer is being filled.
///   - Dequeue the buffer from v4l2 and finish blob processing.
///   - Do MJPEG processing if needed.
///   - Re-enqueue buffer and start over.
pub struct MocapCameraCaptureProcessor {
    resources: ServiceResourceGroup,
    shared: Arc<Shared>,
}    

impl_resource_passthrough!(MocapCameraCaptureProcessor, resources);

struct Shared {
    config: Arc<AsyncMutex<Arc<MocapCameraConfigureRequest>>>,
    camera_sensor: CameraSensorData,
    mono_time_sync: Arc<MonotonicClockTimeSyncer>,
    blob_subscribers: BroadcastChannel<ReadBlobsResponse>,
    mjpeg_subscribers: BroadcastChannel<Bytes>,
    enqueue_state: AsyncMutex<EnqueueState>,
    state: AsyncMutex<State>
}

// NOTE: THis must be locked while enqueueing to ensure that we aren't giving things to v4l2 out of order.
struct EnqueueState {
    next_frame_sequence: u32,
    capture_stream: Arc<v4l2::Stream<v4l2::DMABuffer<CaptureDMABuffer>>>,
}

#[derive(Default)]
struct State {
    pending_dequeue: VecDeque<PendingDequeueEntry>,

    /// All pairs of a v4l2+DMA buffers that are currently in the v4l2 queue
    /// but haven't received a FRAME_SYNC event yet.
    ///
    /// - Items are pushed to the back by the camera_capture_thread right before
    ///   a buffer is given to v4l2
    /// - Items are popped from the front when a FRAME_SYNC is received and
    ///   pushed to the back of 
    enqueued_buffers: VecDeque<EnqueuedBuffer>,

    /// Ordered list of frame sequences which are currently being processed by the blob processing threads.
    ///
    /// This is used to track how much stuff is currently processing and blocks out of order
    /// processing responses.
    ///
    /// Entries are added when we push work to the blob processing threads and removed by the blob processing
    /// threas once they are done working on that item.
    blob_processing_queue: VecDeque<u32>,

    /// Similar to blob_processing_queue but for tracking completion of MJPEG encoding.
    mjpeg_processing_queue: VecDeque<u32>,
}

struct PendingDequeueEntry {
    frame_sequence: u32,
    dma_buffer: CaptureDMABuffer,
    dequeue_done: oneshot::Sender<DequeuedBuffer>,
}

struct EnqueuedBuffer {
    frame_sequence: u32,
    dma_buffer: CaptureDMABuffer,
    dequeue_done: oneshot::Receiver<DequeuedBuffer>
}

struct DequeuedBuffer {
    shared: Arc<Shared>,
    buf: Option<v4l2::DMABuffer<CaptureDMABuffer>>,
    dequeued_time: Duration,
}

impl Drop for DequeuedBuffer {
    fn drop(&mut self) {
        let buf = self.buf.take().unwrap();
        let shared = self.shared.clone();

        // TODO: Re-enqueue it.
        executor::spawn(async move {
            if let Err(e) = MocapCameraCaptureProcessor::enqueue_buffer(&shared, buf).await {
                eprintln!("Failure while returning buffer: {}", e);
            }
        });
    }
}


#[derive(Default)]
struct WallTimeTracker {
    total: Duration
}

impl WallTimeTracker {
    #[inline(always)]
    pub fn span<T, F: FnMut() -> T>(&mut self, mut f: F) -> T {
        let a = Instant::now();
        let ret = f();
        let b = Instant::now();
        self.total += b - a;
        ret
    }

}


impl MocapCameraCaptureProcessor {
    pub async fn create(
        config: Arc<AsyncMutex<Arc<MocapCameraConfigureRequest>>>,
        camera_sensor: CameraSensorData,
        capture_device: v4l2::Device,
        capture_stream: v4l2::UnconfiguredStream,
        mono_time_sync: Arc<MonotonicClockTimeSyncer>,
    ) -> Result<Self> {

        let (mut capture_stream, capture_buffers) = capture_stream.configure_dma(NUM_FRAME_BUFFERS).await?;
        assert_eq!(capture_buffers.len(), NUM_FRAME_BUFFERS);

        let capture_stream = Arc::new(capture_stream);


        let resources = ServiceResourceGroup::new("MocapCameraCaptureProcessor");
        
        let shared = Arc::new(Shared {
            config,
            camera_sensor,
            mono_time_sync,
            blob_subscribers: BroadcastChannel::default(),
            mjpeg_subscribers: BroadcastChannel::default(),
            enqueue_state: AsyncMutex::new(EnqueueState {
                capture_stream: capture_stream.clone(),
                next_frame_sequence: 0,
            }),
            state: AsyncMutex::default()
        });

        let (processing_sender, processing_receiver) = channel::unbounded(); 
        let (mjpeg_sender, mjpeg_receiver) = channel::unbounded();

        resources.spawn_interruptable(
            "MocapCamera::camera_capture_thread",
            Self::camera_capture_thread(shared.clone(), capture_stream, capture_buffers)
        ).await;

        resources.spawn_interruptable(
            "MocapCamera::event_dequeue_thread",
            Self::event_dequeue_thread(shared.clone(), capture_device, processing_sender)
        ).await;

        for i in 0..NUM_BLOB_PROCESSING_THREADS {
            resources.spawn_interruptable(
                &format!("MocapCamera::blob_processing_thread[{}]", i),
                Self::blob_processing_thread(shared.clone(), processing_receiver.clone(), mjpeg_sender.clone())
            ).await;
        }

        for i in 0..NUM_MJPEG_THREADS {
            resources.spawn_interruptable(
                &format!("MocapCamera::mjpeg_processing_thread[{}]", i),
                Self::mjpeg_processing_thread(shared.clone(), mjpeg_receiver.clone())
            ).await;
        }

        Ok(Self {
            resources,
            shared,
        })
    }

    pub fn blob_subscribers(&self) -> &BroadcastChannel<ReadBlobsResponse> {
        &self.shared.blob_subscribers
    }

    pub fn mjpeg_subscribers(&self) -> &BroadcastChannel<Bytes> {
        &self.shared.mjpeg_subscribers
    }

    async fn enqueue_buffer(shared: &Shared, buffer: v4l2::DMABuffer<CaptureDMABuffer>) -> Result<()> {
        // TODO: If anything here fails, then we need to poison the enqueue_state.
        lock_async!(enqueue_state <= shared.enqueue_state.lock().await?, {

            let frame_sequence = enqueue_state.next_frame_sequence;
            enqueue_state.next_frame_sequence = frame_sequence + 1;

            let (sender, receiver) = oneshot::channel();

            let pending_dequeue = PendingDequeueEntry {
                frame_sequence,
                dma_buffer: unsafe { buffer.data().unwrap().clone() },
                dequeue_done: sender,
            };

            let enqueued_buffer = EnqueuedBuffer {
                frame_sequence,
                dma_buffer: unsafe { buffer.data().unwrap().clone() },
                dequeue_done: receiver,
            };

            lock!(state <= shared.state.lock().await?, {
                state.pending_dequeue.push_back(pending_dequeue);
                state.enqueued_buffers.push_back(enqueued_buffer);
            });

            enqueue_state.capture_stream.enqueue_buffer(buffer).await?;

            Ok(())
        })
    }

    // NOTE: In here and in the event dequeuing loop, we can't be skipping any frames or eventa otherwise
    // we won't be able to track which events correspond to which buffers.
    async fn camera_capture_thread(
        shared: Arc<Shared>,
        capture_stream: Arc<v4l2::Stream<v4l2::DMABuffer<CaptureDMABuffer>>>,
        capture_buffers: Vec<v4l2::DMABuffer<CaptureDMABuffer>>,
    ) -> Result<()> {
        let dma_buffers = CaptureDMABuffer::allocate(
            (shared.camera_sensor.width as usize) * (shared.camera_sensor.height as usize),
            capture_buffers.len()
        )?;

        // TODO: Just zip them.
        for (mut capture_buf, dma_buffer) in capture_buffers.into_iter().zip(dma_buffers.into_iter()) {;
            capture_buf.set_data(dma_buffer);
            Self::enqueue_buffer(&shared, capture_buf).await?;
        }

        println!("On");
        capture_stream.turn_on().await?;
        println!("Done!");

        // TODO: Graceful cancellation.

        loop {
            let buf = capture_stream.dequeue_buffer().await?;
            let dequeued_time: Duration = sys::ClockId::MONOTONIC.get_time()?.into();

            let dequeue_entry = lock!(state <= shared.state.lock().await?, {
                state.pending_dequeue.pop_front()
            }).ok_or_else(|| err_msg("Dequeued buffer but no entry associated with it"))?;

            if buf.sequence() != dequeue_entry.frame_sequence {
                return Err(format_err!("Skipped frame sequences: {} vs {}", buf.sequence(), dequeue_entry.frame_sequence));
            }

            if !dequeue_entry.dma_buffer.equals(buf.data().unwrap()) {
                return Err(err_msg("Next dequeued buffer did not have the right DMA buffer attached"));
            }

            dequeue_entry.dequeue_done.send(DequeuedBuffer {
                shared: shared.clone(),
                buf: Some(buf),
                dequeued_time
            });
        }

        capture_stream.turn_off().await?;
    }

    async fn event_dequeue_thread(
        shared: Arc<Shared>,
        capture_device: v4l2::Device,
        processing_sender: channel::Sender<(v4l2::Event, EnqueuedBuffer)>
    ) -> Result<()> {
        /*
        V4L2_EVENT_FRAME_SYNC
        drivers/media/platform/raspberrypi/rp1_cfe/cfe.c

        ktime_get_ns
        ktime_get
        */
        capture_device.subscribe_to_event(v4l2::EventType::FRAME_SYNC).await?;

        let mut next_event_seq = 0;
        loop {
            let event = capture_device.dequeue_event().await?;

            let now: Duration = sys::ClockId::MONOTONIC.get_time()?.into();
            let staleness = now - event.monotonic_timestamp();

            // TODO: This is likely to fail initially since on program restarts, we may still be running with the other
            // program's frame sync settings. 
            if staleness > Duration::from_millis(8) {
                return Err(format_err!("Recieved stale event: {:?} : {:?}", staleness, event))
            }

            if event.sequence() != next_event_seq {
                return Err(format_err!(
                    "Based on event sequence, some events were skipped. Seq: {}; Expected: {}",
                    event.sequence(),
                    next_event_seq
                ));
            }
            next_event_seq = event.sequence() + 1;

            let frame_sync = match event.data() {
                v4l2::EventData::FrameSync(v) => v,
                _ => return Err(err_msg("Event is not a frame sync message"))
            };

            let enqueued_buffer = lock!(state <= shared.state.lock().await?, {
                state.enqueued_buffers.pop_front()
            }).ok_or_else(|| err_msg("Got event but no buffers enqueued"))?;

            if enqueued_buffer.frame_sequence != frame_sync.frame_sequence {
                return Err(err_msg("Incorrect alignment of enqueues and events"));
            }

            lock!(state <= shared.state.lock().await?, {
                if state.blob_processing_queue.len() < NUM_BLOB_PROCESSING_THREADS {
                    state.blob_processing_queue.push_back(frame_sync.frame_sequence);
                    processing_sender.try_send((event, enqueued_buffer)); 
                } else {
                    eprintln!("Hit blob processing concurrency limit");
                }
            });
        }
    }

    async fn blob_processing_thread(
        shared: Arc<Shared>,
        receiver: channel::Receiver<(v4l2::Event, EnqueuedBuffer)>,
        mjpeg_sender: channel::Sender<DequeuedBuffer>,
    ) -> Result<()> {
        let mut frame_processor = FrameProcessor::new(
            shared.camera_sensor.width as usize,
            shared.camera_sensor.height as usize,
        );

        assert!((shared.camera_sensor.height as usize) % NUM_FRAME_CHUNKS == 0);

        let buffer_size = (
            (shared.camera_sensor.height as usize) *
            (shared.camera_sensor.width as usize)
        );
        let chunk_size = buffer_size / NUM_FRAME_CHUNKS;
        assert!(chunk_size % CACHE_LINE_SIZE == 0);

        let chunk_time = FRAME_TRANSFER_TIME / (NUM_FRAME_CHUNKS as u32);

        let mut last_log_time: Duration = sys::ClockId::MONOTONIC.get_time()?.into();

        loop {
            let (event, enqueued_buffer) = receiver.recv().await?;

            let config: Arc<MocapCameraConfigureRequest> = lock!(config <= shared.config.lock().await?, {
                config.clone()
            });

            // Local monotonic clock time at which we get the start of frmae.
            //
            // This timestamp is recorded in the interrupt handler for the 'start of frame'
            // MIPI packet so it should be at the trigger time + exposure time.
            //
            // NOTE: This can use either the v4l2::Buffer or v4l2::Event timestamp since they
            // both mean the same thing.
            let sof_time = event.monotonic_timestamp();

            let mut processing_time = WallTimeTracker::default();

            // TODO: Need to make this more robust to any times when we change the frame rate.
            let (frame_timestamp, exposure_time) = {
                let sof_time_ptp = shared.mono_time_sync.monotonic_to_ptp_time(sof_time);

                // NOTE: If the exposure time recently changed, then this may glitch and have a high error
                // though that is pretty unlikely since we use exposure times that are much smaller than
                // '1 / fps'.
                let trigger_time_ptp = sof_time_ptp - Duration::from_micros(config.exposure_micros() as u64);

                // If the job restarted and the PPS divider still has an old config, we may still get frames even if frame_rate is 0
                // TODO: Fix this.
                let frame_rate = config.frame_rate().max(5);

                let frame_timestamp = rounded_frame_timestamp(trigger_time_ptp, frame_rate);
                let exposure_time = sof_time_ptp.checked_sub(frame_timestamp);

                (frame_timestamp.as_nanos() as u64, exposure_time)
            };

            let blob_processing_active = shared.blob_subscribers.active();

            if blob_processing_active {
                frame_processor.reset();
            }

            // NOTE: Regardless of whether or not we do the blob processing, we always do the cache invalidation
            // logic so that it is always done (so the the MJPEG logic also gets this for free)
            for chunk_i in 0..(NUM_FRAME_CHUNKS - 1) {
                let now: Duration = sys::ClockId::MONOTONIC.get_time()?.into();
                let completion_time = sof_time + chunk_time * ((chunk_i + 1) as u32);
                if now < completion_time {
                    // TODO: Use a standard sleep once blob processing has a dedicated OS thread.
                    executor::sleep(completion_time - now).await?;
                }

                processing_time.span(|| {
                    // NOTE: The CPU's memory pre-fetcher may cache beyond the current chunk so this
                    // needs to clear the chunk cache every time.
                    let chunk_data = enqueued_buffer.dma_buffer.read_partial(
                        chunk_size * chunk_i,
                        chunk_size
                    )?;

                    if blob_processing_active {
                        frame_processor.process_lines(chunk_data, config.pixel_threshold() as u8);
                    }

                    Result::<_, Error>::Ok(())
                })?;
            }

            // For the final chunk, we will precisely time the processing by waiting for the 
            // full buffer to be dequeued.

            // TODO: Need to check for buffer errors after dequeue.
            // (we should ideally be resilient to occasional errors in the buffer)
            let dequeued_buffer = enqueued_buffer.dequeue_done.recv().await
                .map_err(|_| err_msg("Failed to get dequeued buffer"))?;


            processing_time.span(|| {
                let chunk_data = enqueued_buffer.dma_buffer.read_partial(
                    chunk_size * (NUM_FRAME_CHUNKS - 1),
                    chunk_size
                )?;

                if blob_processing_active {
                    frame_processor.process_lines(chunk_data, config.pixel_threshold() as u8);
                }

                Result::<_, Error>::Ok(())
            })?;

            // TODO: Figure out which this is always proportional to frame rate rather than going at
            // the max speed of the MIPI bus.
            //
            // It seems like the RP1 is only ever releasing it when 
            let actual_frame_transfer_time = dequeued_buffer.dequeued_time - sof_time;

            let mut res = None;
            if blob_processing_active {
                let results = processing_time.span(|| {
                    frame_processor.finish(config.blob_filter())
                });

                let mut res = res.insert(ReadBlobsResponse::default());
                res.set_frame_timestamp(frame_timestamp);
                res.new_cameras().set_results(results);

                let now: Duration = sys::ClockId::MONOTONIC.get_time()?.into();
                
                let overhead = now - dequeued_buffer.dequeued_time;
                if now - last_log_time >= Duration::from_secs(2) || overhead > Duration::from_millis(6) {
                    last_log_time = now;

                    println!(
                        "Blobs: [Overhead: {:?}] [Compute: {:?}] [Frame Transfer: {:?}] [Exposure: {:?}]",
                        overhead,
                        processing_time.total,
                        actual_frame_transfer_time,
                        exposure_time
                    );
                }

                // TODO: Also report the latency (from trigger to end of calculation)
                // (we should be able to assume that exposure started exactly at the rounded timestamp to eliminate kernel latency)
            }

            /*
            // TODO: wipe all the data from the frame to validate that the partial caching is actually working.
            // (since if the frame is wiped, we would see nothing in the run run if we are still caching stuff.)
            {
                let data = unsafe { dequeued_buffer.buf.as_ref().unwrap().data().unwrap().write_unsafe() };
                data.fill(0x00);
            }
            */


            lock!(state <= shared.state.lock().await?, {
                if state.blob_processing_queue[0] == enqueued_buffer.frame_sequence {
                    if let Some(res) = res {
                        shared.blob_subscribers.send(res);
                    }
                } else {
                    eprintln!("Blob processing threads completed out of order. Dropping data...");
                }

                state.blob_processing_queue.retain(|v| *v != enqueued_buffer.frame_sequence);

                if state.mjpeg_processing_queue.len() < NUM_MJPEG_THREADS {
                    state.mjpeg_processing_queue.push_back(enqueued_buffer.frame_sequence);
                    mjpeg_sender.try_send(dequeued_buffer);
                }
            });
        }
    }

    // Note that mjpeg processing always happens after blob processing for a given frame so that we can avoid having to deal with
    // both trying to invalidate the cached memory while the other is running.
    async fn mjpeg_processing_thread(
        shared: Arc<Shared>,
        receiver: channel::Receiver<DequeuedBuffer>
    ) -> Result<()> {

        use image::format::jpeg::encoder::JPEGEncoder;

        // Note that 90% quality has been observed to produce stripping artifacts for things like checkerboard
        // patterns.
        // - 100% compresses to is 0.2x the original size
        // - 90% compressed to 0.1x the original size
        let mut encoder = JPEGEncoder::new(100);
        encoder.use_default_tables();

        let mut last_mjpeg_frame = Instant::now();

        let mut last_log_time = Instant::now();

        let mut i = 0; 
        loop {
            let buffer = receiver.recv().await?;
            let buf = buffer.buf.as_ref().unwrap();
            let frame_sequence = buf.sequence();

            let mut output_data = None;

            if shared.mjpeg_subscribers.active() {
                // Note that cache invalidation is handled by the blob processing thread.
                let raw_data = unsafe { buf.data().unwrap().read_unsafe() };

                let now = Instant::now();

                if now - last_mjpeg_frame > Duration::from_secs_f32(1.0 / 5.0) {
                    last_mjpeg_frame = now;

                    let s = Instant::now();

                    let image = image::ImageRef {
                        width: shared.camera_sensor.width as usize,
                        height: shared.camera_sensor.height as usize,
                        channels: 1,
                        data: raw_data,
                    };

                    let mut data = vec![];
                    data.reserve_exact(raw_data.len() / 8);

                    encoder.encode_raw(&image, &mut data)?;
                    let e = Instant::now();

                    let compression_ratio = (data.len() as f32) / (raw_data.len() as f32);

                    output_data = Some(data.into());

                    if now - last_log_time >= Duration::from_secs(2) {
                        last_log_time = now;
                        println!("Encode {} in {:?} : Compression {:.2}", frame_sequence, e - s, compression_ratio);
                    }

                    i += 1;
                }
            }

            lock!(state <= shared.state.lock().await?, {
                // TODO: Check that we are the first entry in the queue.
                if let Some(data) = output_data {
                    let _ = shared.mjpeg_subscribers.send(data);   
                }

                state.mjpeg_processing_queue.retain(|v| *v != frame_sequence);
            });
        }
    }
}