// Helpers for reading/writing an HTTP2 body from a stream.

use std::sync::Arc;

use common::errors::*;
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::io::Readable;

use crate::body::Body;
use crate::header::{Headers, TRANSFER_ENCODING, CONNECTION};
use crate::request::RequestHead;
use crate::response::ResponseHead;
use crate::method::Method;
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
            // TODO: Find the best way to make sure that we always log this error.
            println!("OUTGOING BODY FAILURE: {}", e);
            let _ = self.connection_event_sender.send(ConnectionEvent::StreamWriteFailure {
                stream_id: self.stream_id,
                internal_error: e
            }).await;
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

            // TODO: Must subtract from this.
            let max_to_read = stream.remote_window;
            
            // NOTE: We don't want to hold the stream for performing the reading from the body
            // as that may take a long time.
            // drop(stream);

            if max_to_read > 0 {
                let start = stream.sending_buffer.len();
                stream.sending_buffer.resize(start + (max_to_read as usize), 0);

                // TODO: If this errors, out, then we will be in an inconsistent state (the sending_buffer will be too big).
                let n = body.read(&mut stream.sending_buffer[start..]).await?;
                stream.sending_buffer.truncate(start + n);

                // Once we've read all the data, we're done.
                //
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
            stream_state.sending_end = true;
        }

        // Let the ConnectionWriter know that we completed generating the
        // outgoing body.
        //
        // NOTE: It doesn't matter if this fails as we are going to exit this
        // function immediately afterwards anyway.
        let _ = self.connection_event_sender.send(ConnectionEvent::StreamWrite {
            stream_id: self.stream_id
        }).await;

        Ok(())
    }

}

/// Reader of data received on a HTTP2 stream from a remote endpoint.
///
/// TODO: If an IncomingStreamBody gets dropped, any data that is still unread
/// should be freed and given back to the other endpoint via a WINDOW_UPDATE
/// TODO: Sometimes we may want read() to return an error (e.g. if there was a stream error.)
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

    /// Expected length of this body typically derived from the 'Content-Length' header.
    ///
    /// NOTE: Validation that we don't read less or more than this number is
    /// done in the connection code and not in this file.
    pub expected_length: Option<usize>
}

impl Drop for IncomingStreamBody {
    fn drop(&mut self) {
        // Notify the connection that we are done reading from the stream. Any remaining
        // data received on this stream can be discarded. 
        // TODO: If the stream was fully read successfully, then we don't need to send this.
        self.connection_event_sender.try_send(ConnectionEvent::StreamReaderClosed {
            stream_id: self.stream_id,
            stream_state: self.stream_state.clone()
        });
    }
}

#[async_trait]
impl Body for IncomingStreamBody {
    fn len(&self) -> Option<usize> { self.expected_length.clone() }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        let mut stream_state = self.stream_state.lock().await;
        if !stream_state.received_end {
            return Err(err_msg("Haven't read entire stream yet"));
        }

        // NOTE: Currently if this is called twice, it will return None the
        // second time.
        Ok(stream_state.received_trailers.take())
    }
}

// When this is dropped, there as a few things that can happen.
// 1. We become an infinite sink hole for new data.
// 2. 

#[async_trait]
impl Readable for IncomingStreamBody {
    async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let mut nread = 0;

        // TODO: Error out if this has to loop more than twice.
        while !buf.is_empty() {
            let mut stream_state = self.stream_state.lock().await;

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

                // Notify the connection that we can receive more data (as the
                // received buffer has shrunk).
                //
                // NOTE: It is ok for this to fail as we are allowed to continue reading from a
                // body even after the connection has been closed so long as all data has already
                // been received. This would typically happen if either side tries to gracefully
                // close the connection. See the later self.read_available_receiver.recv() which
                // will fail if we didn't get all the data.
                //
                // TODO: Optimize this so that we only need the channel to store a HashSet of stream ids
                let _ = self.connection_event_sender.send(ConnectionEvent::StreamRead {
                    stream_id: self.stream_id,
                    count: n
                }).await;

                nread += n;

                // Stop as soon as we read any data
                break;
            } else if stream_state.received_end {
                break;
            }

            // Unlock all resources.
            drop(stream_state);

            // Wait for a change in the reader buffer.
            // If this fails, then that means that we haven't received all data yet, but the
            // connection is closed so we will never get all the data.
            // TODO: If this fails, check one last time to see if these is a better error in the state?
            self.read_available_receiver.recv().await
                .map_err(|_| err_msg("Connection closed before receiving all data"))?;
        }

        Ok(nread)
    }
}


/// NOTE: We assume that the 'stream_state' matches the one in 'incoming_body'.
pub fn create_server_request_body(
    request_head: &RequestHead, mut incoming_body: IncomingStreamBody,
    stream_state: &mut StreamState,
) -> StreamResult<Box<dyn Body>> {
    // 8.1.2.2
    // Verify that there are no HTTP 1 connection level headers exist.
    if request_head.headers.find(CONNECTION).next().is_some() ||
       request_head.headers.find(TRANSFER_ENCODING).next().is_some() {
        // TODO: Make this a stream error
        return Err(StreamError::malformed_message(
            "Received HTTP 1 connection level headers"));
    }

    let content_length = crate::header_syntax::parse_content_length(&request_head.headers)
        .map_err(|_| StreamError::malformed_message("Request contains invalid Content-Length"))?;

    if let Some(len) = content_length {
        stream_state.received_expected_bytes = Some(len);
        incoming_body.expected_length = Some(len);
    }

    Ok(Box::new(incoming_body))
}

/// NOTE: We assume that the 'stream_state' matches the one in 'incoming_body'.
pub fn create_client_response_body(
    request_method: Method,
    response_head: &ResponseHead,
    mut incoming_body: IncomingStreamBody,
    stream_state: &mut StreamState
) -> StreamResult<Box<dyn Body>> {
    // 8.1.2.2
    // Verify that there are no HTTP 1 connection level headers exist.
    if response_head.headers.find(CONNECTION).next().is_some() ||
       response_head.headers.find(TRANSFER_ENCODING).next().is_some() {
        // TODO: Make this a stream error
        return Err(StreamError::malformed_message(
            "Received HTTP 1 connection level headers"));
    }

    let mut expected_length = None;

    let status_num = response_head.status_code.as_u16();
    // 1xx
    let info_status = status_num >= 100 && status_num < 200;
    // 2xx
    let success_status = status_num >= 200 && status_num < 300;


    if request_method == Method::HEAD || info_status || status_num == 204 ||
       status_num == 304 || (request_method == Method::CONNECT && success_status) {
        expected_length = Some(0);
    } else {
        expected_length = crate::header_syntax::parse_content_length(&response_head.headers)
            .map_err(|_| StreamError::malformed_message("Response contains invalid Content-Length"))?;
    }

    if let Some(len) = expected_length {
        stream_state.received_expected_bytes = Some(len);
        incoming_body.expected_length = Some(len);
    }

    Ok(Box::new(incoming_body))
}
