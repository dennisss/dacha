use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use common::io::{IoError, IoErrorKind, Readable};
use executor::channel::error::RecvError;
use executor::channel::spsc;
use executor::lock_async;
use executor::sync::AsyncMutex;
use http::Headers;

use crate::buffer_queue::*;
use crate::message::MessageSerializer;

/// Buffer for retaining the serialized Messages sent(/to be sent) for a client
/// request.
/// - This is used to keep data in memory in case we want to retry or hedge the
///   request.
/// - There will be a single MessageRequestBuffer instance per channel
///   invocation (shared across all RPC attempts).
/// - At least min(last sent message, N bytes) are stored in the buffer.
pub struct MessageRequestBuffer {
    /// Maximum length we will try to enforce for the request buffer.
    max_length: usize,

    /// Channel for receiving more data from the client
    /// (ClientStreamingRequest). This receives data when when
    ///
    /// If this needs to be locked, then it MUST be locked before 'received' is
    /// locked.
    request_receiver: AsyncMutex<spsc::Receiver<Result<Option<Bytes>>>>,

    /// Data contained in this buffer.
    ///
    /// MUST NOT be locked while blocking for an indefinite amount of time.
    received: AsyncMutex<MessageRequestBufferReceived>,
}

struct MessageRequestBufferReceived {
    /// Current state of our buffering.
    state: MessageRequestBufferState,

    /// Data to send for this request. This is a chain of memory buffers to be
    /// treated as contiguous segments.
    buffer: BufferQueue,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MessageRequestBufferState {
    /// Still actively polling request_receiver for more data to send.
    Receiving,

    /// The request_receiver returned an error (this is a terminal state caused
    /// by the client so retrying won't help with this.)
    Error,

    /// The request_receiver's opposite sending end was dropped so likely this
    /// request is being cancelled.
    Cancelled,

    /// request_receiver has been exhaustively polled and we have attempted to
    /// insert all bytes of the request into the buffer (some may have been
    /// evicted though).
    Done,
}

impl MessageRequestBuffer {
    pub fn new(max_length: usize, request_receiver: spsc::Receiver<Result<Option<Bytes>>>) -> Self {
        Self {
            max_length,
            request_receiver: AsyncMutex::new(request_receiver),
            received: AsyncMutex::new(MessageRequestBufferReceived {
                state: MessageRequestBufferState::Receiving,
                buffer: BufferQueue::new(),
            }),
        }
    }

    /// Checks whether or not this buffer can currently be used to retry the
    /// request.
    pub async fn is_retryable(&self) -> bool {
        let received = match self.received.lock().await {
            Ok(v) => v.read_exclusive(),
            Err(_) => return false,
        };

        // Can't be retried if we truncated some data.
        if received.buffer.start_byte_offset() != 0 {
            return false;
        }

        match received.state {
            MessageRequestBufferState::Receiving => true,
            MessageRequestBufferState::Error => false,
            MessageRequestBufferState::Cancelled => false,
            MessageRequestBufferState::Done => true,
        }
    }

    /// Reads from the buffer into 'buf'. The next position in the buffer to be
    /// read is defined by 'cursor'.
    ///
    /// This function should be completely safe to cancel/retry in the future.
    ///
    /// CANCEL SAFE
    async fn read(&self, cursor: &mut BufferQueueCursor, mut buf: &mut [u8]) -> Result<usize> {
        let mut request_receiver = self.request_receiver.lock().await?.enter();

        // This is safe because:
        // - We only use wait() and try_recv().
        // - The return value of try_recv() is never transferred across sync/async code
        //   boundaries so will always be atomically processed.
        unsafe { request_receiver.unpoison() };

        self.read_with_receiver(&mut request_receiver, cursor, buf)
            .await
    }

    async fn read_with_receiver(
        &self,
        request_receiver: &mut spsc::Receiver<Result<Option<Bytes>>>,
        cursor: &mut BufferQueueCursor,
        mut buf: &mut [u8],
    ) -> Result<usize> {
        let mut nread = 0;
        loop {
            let mut received = self.received.lock().await?.read_exclusive();
            let current_state = received.state;
            match current_state {
                MessageRequestBufferState::Receiving | MessageRequestBufferState::Done => {
                    match received.buffer.read(cursor, buf) {
                        Ok(n) => {
                            nread += n;
                            buf = &mut buf[n..];
                        }
                        Err(()) => {
                            // This should only happen if we are using hedging with different
                            // requests competting for buffer space.
                            return Err(IoError::new(
                                IoErrorKind::Aborted,
                                "Fell behind while reading the RPC request buffer.",
                            )
                            .into());
                        }
                    }

                    // Drop before doing any receiving to ensure that any concurrent attempts can
                    // also use the buffer.
                    drop(received);

                    if buf.is_empty() {
                        break;
                    }

                    if current_state == MessageRequestBufferState::Receiving {
                        // If we couldn't ready anything from the buffer, block until we get more
                        // data. Otherwise just optimistically process any messages already in the
                        // reciever channel.

                        if nread == 0 {
                            request_receiver.wait().await;
                        }

                        let mut received = self.received.lock().await?.enter();

                        // NOTE: All code after this point must be synchronous.
                        // Once we receive data, we must apply it to the state regardless of whether
                        // or not the task was cancelled. This is because we may have other
                        // concurrent attempts or future retries that may need that data.

                        let data = match request_receiver.try_recv() {
                            Some(v) => v,
                            None => {
                                assert!(nread != 0);
                                received.exit();
                                break;
                            }
                        };

                        match data {
                            Ok(Ok(Some(data))) => {
                                let header = MessageSerializer::serialize_header(&data, false);

                                received.buffer.advance_until_under_limit(
                                    self.max_length
                                        .checked_sub(header.len() + data.len())
                                        .unwrap_or(0),
                                );

                                received.buffer.push(header);
                                received.buffer.push(data);
                                received.exit();
                                continue;
                            }
                            Ok(Ok(None)) => {
                                received.state = MessageRequestBufferState::Done;
                                received.exit();
                                break;
                            }
                            Ok(Err(e)) => {
                                // Custom failure reason (non-cancellation).
                                received.state = MessageRequestBufferState::Error;
                                received.exit();
                                return Err(e);
                            }
                            Err(RecvError::SenderDropped) => {
                                // The sender was dropped before the None (end of stream indicator)
                                // was sent.
                                received.state = MessageRequestBufferState::Cancelled;
                                received.exit();
                                continue;
                            }
                        }
                    }

                    break;
                }
                MessageRequestBufferState::Error => {
                    return Err(IoError::new(
                        IoErrorKind::Aborted,
                        "Previously received error while reading the request buffer.",
                    )
                    .into());
                }
                MessageRequestBufferState::Cancelled => {
                    // The sender was dropped before the None (end of stream indicator) was sent.
                    return Err(IoError::new(
                        IoErrorKind::Cancelled,
                        "RPC request ended before complete body was written.",
                    )
                    .into());
                }
            }
        }

        Ok(nread)
    }
}

/// http::Body for serializing client requests as separate message frames.
///
/// There will be one instance of this per RPC attempt.
pub struct MessageRequestBody {
    /// Current position relative to the start of the request at which we will
    /// next return data.
    cursor: BufferQueueCursor,

    /// Buffer of request data to be sent.
    buffer: Arc<MessageRequestBuffer>,

    /// Channel used to track if this attempt is still alive. The request is
    /// determined to be alive if this channel isn't closed.
    attempt_alive: spsc::Receiver<()>,
}

impl MessageRequestBody {
    pub fn new(buffer: Arc<MessageRequestBuffer>, attempt_alive: spsc::Receiver<()>) -> Self {
        // TODO: Take the initial length of the buffer as a reference here if it is
        // done.
        // ^ We will need to process all currently outstanding requests (but we must
        // make sure that we don't exceed the buffer side)
        // ^ Also making sure we cork for unary responses?
        Self {
            cursor: BufferQueueCursor::default(),
            buffer,
            attempt_alive,
        }
    }
}

#[async_trait]
impl http::Body for MessageRequestBody {
    fn len(&self) -> Option<usize> {
        //
        None
    }
    async fn trailers(&mut self) -> Result<Option<Headers>> {
        Ok(None)
    }
}

#[async_trait]
impl Readable for MessageRequestBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // Wait for either the attempt to end or until we get a successful read.
        //
        // We must wait for the attempt to finish in order as the HTTP2 connection will
        // only by dropped after this function returns a cancellation.
        //
        // NOTE: If the read future is dropped we will be in an undefined state so this
        // body can't be polled again.

        enum Event {
            AttemptCancelled,
            ReadDone(Result<usize>),
        }

        let buffer_read_future =
            executor::future::map(Box::pin(self.buffer.read(&mut self.cursor, buf)), |r| {
                Event::ReadDone(r)
            });

        let attempt_cancelled =
            executor::future::map(self.attempt_alive.recv(), |_| Event::AttemptCancelled);

        let event = executor::future::race(buffer_read_future, attempt_cancelled).await;

        match event {
            Event::AttemptCancelled => {
                // NOTE: This is different than the other cancellation error as this is when the
                // request might still be alive but this specific attempt is dead.
                Err(IoError::new(
                    IoErrorKind::Cancelled,
                    "RPC attempt ended before complete body was written.",
                )
                .into())
            }
            Event::ReadDone(r) => r,
        }
    }
}
