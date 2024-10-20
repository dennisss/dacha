use std::sync::Arc;
use std::time::{Duration, Instant};

use common::bytes::Bytes;
use common::errors::*;
use executor::channel::error::SendError;
use executor::lock;
use executor::{channel::spsc, sync::AsyncMutex};
use executor_graph::*;

use crate::frame::ImageFrame;

/// Maximum number of unprocessed frames that one subscriber can have enqueued
/// before we start dropping frames.
///
/// Default value is 2 seconds worth of buffering at 30 fps.
const CAMERA_SUBSCRIBER_QUEUE_LENGTH: usize = 2 * 30;

/// If no subscriber is pulling frames from the camera for this amount of time,
/// we will
const CAMERA_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

/// Set of subscriber clients that all want to read from the same stream of
/// image frames.
#[derive(Default)]
pub struct ImageFrameSubscribers {
    subscribers: AsyncMutex<Vec<spsc::Sender<ImageFrame>>>,
}

impl ImageFrameSubscribers {
    pub async fn subscribe(&self) -> Result<spsc::Receiver<ImageFrame>> {
        let (sender, receiver) = spsc::bounded(CAMERA_SUBSCRIBER_QUEUE_LENGTH);

        lock!(subs <= self.subscribers.lock().await?, {
            subs.push(sender);
        });

        Ok(receiver)
    }
}

/// Broadcasts image frames received via a graph input to zero or more external
/// subscribers (not running as part of the graph).
///
/// Note that since subscribers may be slow to process frames (e.g. if sending
/// over the network), this op won't block processing newer frames on any
/// individual subscribers.
pub struct ImageFrameBufferOp {
    subscribers: Arc<ImageFrameSubscribers>,
}

impl ImageFrameBufferOp {
    pub fn new() -> (Self, Arc<ImageFrameSubscribers>) {
        let subscribers = Arc::new(ImageFrameSubscribers::default());
        let inst = Self {
            subscribers: subscribers.clone(),
        };

        (inst, subscribers)
    }
}

#[async_trait]
impl Operation for ImageFrameBufferOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "ImageFrameBufferOp".to_string(),
            num_inputs: 1,
            num_outputs: 0,
        }
    }

    async fn execute(
        &self,
        mut inputs: Vec<InputStream>,
        outputs: Vec<OutputStream>,
    ) -> Result<()> {
        let mut input = inputs.pop().unwrap();

        let mut last_sent_frame = Instant::now();
        loop {
            let frame_any = match input.read().await? {
                Some(v) => v,
                None => break,
            };

            let frame = frame_any
                .downcast_ref::<ImageFrame>()
                .ok_or_else(|| err_msg("Input isn't an image frame"))?;

            // Materialize to a Bytes array so that upstream ops can re-use the original
            // buffer immediately (after drop(frame_any)).
            let mut frame = frame.clone();
            frame.data = Arc::new(Bytes::from(frame.data.data().unwrap()));

            drop(frame_any);

            let now = Instant::now();

            let have_subscribers =
                lock!(subscribers <= self.subscribers.subscribers.lock().await?, {
                    let mut i = 0;
                    while i < subscribers.len() {
                        if let Err(e) = subscribers[i].try_send(frame.clone()) {
                            if e.error == SendError::ReceiverDropped {
                                subscribers.swap_remove(i);
                                continue;
                            }
                        } else {
                            last_sent_frame = now;
                        }

                        i += 1;
                    }

                    !subscribers.is_empty()
                });

            // TODO: Exit immediately if !have_subscribers and we get a
            // cancellation/shutdown signal
            if now - last_sent_frame > CAMERA_IDLE_TIMEOUT {
                break;
            }
        }

        Ok(())
    }
}
