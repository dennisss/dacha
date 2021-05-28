// Helpers for reading/writing an HTTP2 body from a stream.

use std::sync::Arc;

use common::errors::*;
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::io::Readable;

use crate::body::Body;
use crate::header::Headers;
use crate::v2::stream_state::StreamState;
use crate::v2::types::*;
use crate::v2::connection_state::ConnectionEvent;
use crate::proto::v2::{ErrorCode};

/// Wrapper around a Body that is used to read it and feed it to a stream.
/// This is intended to be run as a separate task.
///
/// This will buffer data into the stream's 'sending_buffer' until the remote
/// endpoint's stream level flow control limit is hit (up to some reasonable local limit).
///
/// NOTE: We don't limit ourselves to buffering only up to the connection level flow control
/// limit as they may cause priority inversion issues where low priority streams can send more
/// data if they can buffer data faster than higher priority streams.
///
/// TODO: Eventually we may want to consider sharding the connection level limit to all streams
/// whenever there is a new stream or a priotity change.
pub struct OutgoingStreamBody {
    pub stream_id: StreamId,

    pub stream_state: Arc<Mutex<StreamState>>,

    /// Used to send ConnectionEvent::StreamWrite events to let the writer thread know that
    /// more data is available for writing to 
    pub connection_event_sender: channel::Sender<ConnectionEvent>,

    /// Receives notifications whenever the size of the 'sending_buffer' for this stream has
    /// decreased (or the flow control limit has changed).
    ///
    /// These events typically mean that we can continue buffering more data.
    pub write_available_receiver: channel::Receiver<()>,
}

impl OutgoingStreamBody {

    pub async fn run(mut self, body: Box<dyn Body>) {
        if let Err(e) = self.run_internal(body).await {
            // TODO: Re-use guard from run_internal
            let mut stream_state = self.stream_state.lock().await;
            // stream.

            println!("OUTGOING BODY FAILURE: {}", e);

            // TODO: Consider using some standard way of emitting an error right here?
            // One possible challenge is deal with propagating errors (e.g. remote endpoint
            // hangs up and that causes our outgoing computation to fail.)

            if stream_state.error.is_some() {
                return;
            }

            stream_state.error = Some(ProtocolErrorV2 {
                code: ErrorCode::INTERNAL_ERROR,
                message: "Internal error occured while processing this stream",
                local: true
            });

            drop(stream_state);

            // TODO: This means that the writer thread needs to be responsible for removing streams
            // from the list.
            self.connection_event_sender.send(ConnectionEvent::ResetStream {
                stream_id: self.stream_id,
                error: ProtocolErrorV2 {
                    code: ErrorCode::INTERNAL_ERROR,
                    message: "Internal error occured while processing this stream",
                    local: true
                }
            }).await;

            // Need to mark an error on the stream and trigger a total reset of the stream
            // (this likely requirs a lot of clean up).
        }
    }

    async fn run_internal(&mut self, mut body: Box<dyn Body>) -> Result<()> {
        loop {
            // TODO: Don't keep the stream locked for the body.read() operation as that
            // may take a long time.
            let mut stream = self.stream_state.lock().await;
            
            // Stop if the stream was reset.
            if stream.error.is_some() {
                return Ok(());
            }

            let max_to_read = stream.remote_window;
            
            // NOTE: We don't want to hold the stream for performing the reading from the body
            // as that may take a long time.
            // drop(stream);

            if max_to_read > 0 {
                let start = stream.sending_buffer.len();
                stream.sending_buffer.resize(start + (max_to_read as usize), 0);

                let n = body.read(&mut stream.sending_buffer[start..]).await?;
                stream.sending_buffer.truncate(start + n);
                
                // TODO: Must also check the trailers.
                // if n == 0 {
                //     break;
                //     stream.sending_at_end = true;
                // }

                // NOTE: We break before we send the StreamWrite event as we'd rather get the
                // trailers first (which will usually avoid multiple transfers).
                if n == 0 {
                    break;
                }

                // TODO: Prefer not to send this until we figure out if we have trailers.
                let r = self.connection_event_sender.send(ConnectionEvent::StreamWrite {
                    stream_id: self.stream_id
                }).await;
                if r.is_err() {
                    // Writer thread hung up. No point in continuing to run.
                    return Ok(());
                }

            } else {
                drop(stream);

                let r = self.write_available_receiver.recv().await;
                if r.is_err() {
                    return Ok(());
                }
            }
        }

        // Checking trailers
        {
            let trailers = body.trailers().await?;
            let mut stream_state = self.stream_state.lock().await;
            stream_state.sending_trailers = trailers;
            stream_state.sending_at_end = true;
        }

        self.connection_event_sender.send(ConnectionEvent::StreamWrite {
            stream_id: self.stream_id
        }).await;

        Ok(())
    }

}

/// Reader of data received on a HTTP2 stream from a remote endpoint.
///
/// TODO: Sometimes we may want read() to return an error (e.g. if there was a stream error.)
/// TODO: Dropping this object should imply what?
/// TODO: This is unsufficient as it doesn't do things like read the Content-Type or other things like Transfer-Encoding (requests a layer on top of this.)
pub struct IncomingStreamBody {
    pub stream_id: StreamId,

    pub stream_state: Arc<Mutex<StreamState>>,

    /// Used by the body to notify the connection that data has been read.
    /// This means that the connection can let the other side know that more
    /// data can be sent. 
    ///
    /// NOTE: This will only be used to send ConnectionEvent::StreamRead events.
    /// NOTE: This is created by cloning the 'connection_event_channel' Sender in the 'ConnectionShared' instance.
    pub connection_event_sender: channel::Sender<ConnectionEvent>,

    /// Used by the body to wait for more data to become available to read from the stream (or for an error to occur).
    pub read_available_receiver: channel::Receiver<()>,

    /// Expected length of this body derived from the 'Content-Length' header.
    /// NOTE: Validation that we don't read less or more than this number is
    /// done in the connection code and not in this file.
    pub expected_length: Option<usize>
}

#[async_trait]
impl Body for IncomingStreamBody {
    fn len(&self) -> Option<usize> { self.expected_length.clone() }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        let mut stream_state = self.stream_state.lock().await;
        if !stream_state.received_end_of_stream {
            return Err(err_msg("Haven't read entire stream yet"));
        }

        // NOTE: Currently if this is called twice, it will return None the
        // second time.
        Ok(stream_state.received_trailers.take())
    }
}

#[async_trait]
impl Readable for IncomingStreamBody {
    async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let mut nread = 0;

        // TODO: Error out if this has to loop more than twice.
        while !buf.is_empty() {
            let mut stream_state = self.stream_state.lock().await;

            // TODO: Ideally stream errors should take precedance as some of them could be retryable?

            if let Some(e) = &stream_state.error {
                return Err(e.clone().into());
            }

            if !stream_state.received_buffer.is_empty() {
                let n = std::cmp::min(buf.len(), stream_state.received_buffer.len());
                (&mut buf[0..n]).copy_from_slice(&stream_state.received_buffer[0..n]);
                buf = &mut buf[n..];

                // TODO: Not verify efficient
                stream_state.received_buffer = stream_state.received_buffer.split_off(n);

                // Allow the remote endpoint to send more data now that some has been read.
                // NOTE: The connection level flow control will be updated in the connection writer thread
                // to avoid acquiring a connection wide lock in this function.
                stream_state.local_window += n as WindowSize;

                // TODO: Optimize this so that we only need the channel to store a HashSet of stream ids
                self.connection_event_sender.send(ConnectionEvent::StreamRead {
                    stream_id: self.stream_id,
                    count: n
                }).await
                .map_err(|_| err_msg("Connection hung up.")) ?;

                nread += n;

                // Stop as soon as we read any data
                break;
            } else if stream_state.received_end_of_stream {
                break;
            }

            // Unlock all resources.
            drop(stream_state);

            // Wait for a change in the reader buffer.
            self.read_available_receiver.recv().await?;
        }

        Ok(nread)
    }
}