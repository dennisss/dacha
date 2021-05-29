use std::{convert::TryFrom, sync::Arc};
use std::collections::HashMap;

use common::{chrono::Duration, errors::*};
use common::io::{Writeable, Readable};
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::chrono::prelude::*;
use common::task::ChildTask;

use crate::v2::types::*;
use crate::v2::body::*;
use crate::v2::stream::*;
use crate::v2::stream_state::*;
use crate::v2::headers::*;
use crate::v2::connection_state::*;
use crate::{headers::connection, method::Method, v2::settings::*};
use crate::hpack::HeaderFieldRef;
use crate::hpack;
use crate::request::Request;
use crate::response::{Response, ResponseHead};
use crate::server::RequestHandler;
use crate::proto::v2::*;
use crate::v2::frame_utils;


pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub const CONNECTION_PREFACE_BODY: &[u8] = b"SM\r\n\r\n";

/// Amount of time after which we'll close the connection if we don't receive an acknowledment to our
/// 
pub const SETTINGS_ACK_TIMEOUT_SECS: u64 = 10;

pub struct ConnectionShared {
    pub is_server: bool,

    pub state: Mutex<ConnectionState>,

    // TODO: We may want to keep around a timer for the last time we closed a stream so that if we 

    /// Handler for producing responses to incoming requests.
    ///
    /// NOTE: This will only be used in HTTP servers.
    pub request_handler: Option<Box<dyn RequestHandler>>,

    /// Used to notify the connection of events that have occured.
    /// The writer thread listens to these events performs actions such as sending more data, starting
    /// requests, etc. in response to each event.
    ///
    /// TODO: Make this a bounded channel?
    pub connection_event_channel: (channel::Sender<ConnectionEvent>, channel::Receiver<ConnectionEvent>),

    // Stream ids can't be re-used.
    // Also, stream ids can't be 
}

impl ConnectionShared {

    // TODO: Verify that this is never called when we already have a lock?
    pub async fn recv_reset_stream(&self, stream_id: StreamId, error: ProtocolErrorV2) {
        let mut connection_state = self.state.lock().await;

        let mut stream = {
            if let Some(stream) = connection_state.streams.remove(&stream_id) {
                stream
            } else {
                // Ignore requests for already closed streams.
                return;
            }
        };

        if self.is_local_stream_id(stream_id) {
            connection_state.local_stream_count -= 1;
            // After removing a local stream, try to send any remaining queued requests. 
            let _ = self.connection_event_channel.0.try_send(ConnectionEvent::SendRequest);
        } else {
            connection_state.remote_stream_count -= 1;
        }

        let mut stream_state = stream.state.lock().await;
        stream_state.error = Some(error.clone());

        // TODO: Whenever we are resetting a stream, we can allow the other stream to 

        // If there is unread data when resetting a stream we can clear it and count it as 'read' by
        // increasing the connection level flow control limit. 
        if stream_state.received_buffer.len() > 0 {
            self.connection_event_channel.0.send(ConnectionEvent::StreamRead {
                stream_id: 0,
                count: stream_state.received_buffer.len()
            }).await;
        }

        // Clear no longer needed memory.
        // TODO: Make sure that this doesn't happen if we are just gracefully closing a stream.
        stream_state.received_buffer.clear();
        stream_state.sending_buffer.clear();

        // while let Some(handle) = stream.processing_tasks.pop() {
        //     // TODO: Do I need to task::spawn() this?
        //     handle.cancel();
        // }

        // If the error happened before response headers will received by a client, response with an error.
        // TODO: Also need to notify the requester of whether or not the request is trivially retryable
        // (based on the stream id in the latest GOAWAY message).
        if let Some((request_method, response_handler, body)) = stream.incoming_response_handler.take() {
            response_handler.handle_response(Err(error.into()));
        }

        if let Some(outgoing_body) = stream.outgoing_response_handler.take() {
            // I don't think I need to do anything here?
        }

        // Notify all reader/writer threads that the stream is dead.
        // TODO: Handle errors on all these things.
        let _ = stream.read_available_notifier.try_send(());
        let _ = stream.write_available_notifier.try_send(());
    }

    pub async fn send_reset_stream(&self, stream_id: StreamId, error: ProtocolErrorV2) -> Result<()> {
        self.recv_reset_stream(stream_id, error.clone()).await;
        
        // Notify the writer thread that to let the other endpoint know that the stream should be killed.
        // TODO: Do we care if this fails?
        self.connection_event_channel.0.send(ConnectionEvent::ResetStream { stream_id, error }).await;

        Ok(())
    }

    pub fn is_local_stream_id(&self, id: StreamId) -> bool {
        // Clients have ODD numbered ids. Servers have EVEN numbered ids.
        self.is_server == (id % 2 == 0)
    }

    pub fn is_remote_stream_id(&self, id: StreamId) -> bool {
        !self.is_local_stream_id(id)
    }

    // TODO: According to 8.1.2.1, if a headers blockis received with regular headers before pseudo headers
    // is malformed (stream error PROTOCOL_ERROR)


    /// Performs cleanup on a stream which is gracefully closing with both endpoints having sent a frame
    /// with an END_STREAM flag.
    pub fn finish_stream(&self, connection_state: &mut ConnectionState, stream_id: StreamId) {
        println!("CLOSING STREAM");

        // TODO: Verify that there are no cyclic references to Arc<StreamState> (otherwise the stream state may never get freed)
        connection_state.streams.remove(&stream_id);

        // If we are in a graceful shutdown mode, trigger a full shutdown once we have
        // completed all streams.
        //
        // TODO: What should we do about promised streams?
        //
        // NOTE: If connection_state.error is not None, then it's likely an OK error, because we
        // close the connection immediately on other types of errors.
        if connection_state.error.is_some() && connection_state.streams.is_empty() {
            println!("CLOSING CONNECTION");
            let _ = self.connection_event_channel.0.try_send(ConnectionEvent::Closing { error: None });
            return;
        }

        if self.is_local_stream_id(stream_id) {
            connection_state.local_stream_count -= 1;
            if !connection_state.pending_requests.is_empty() {
                // After removing a local stream, try to send any remaining queued requests. 
                let _ = self.connection_event_channel.0.try_send(ConnectionEvent::SendRequest);
            }
        } else {
            connection_state.remote_stream_count -= 1;
        }
    }


    pub fn new_stream(
        &self, connection_state: &ConnectionState, stream_id: StreamId
    ) -> (Stream, IncomingStreamBody, OutgoingStreamBody) {
        // NOTE: These channels only act as a boolean flag of whether or not something has changed so we should
        // only need to ever have at most 1 message in each of them.
        let (read_available_notifier, read_available_receiver) = channel::bounded(1);
        let (write_available_notifier, write_available_receiver) = channel::bounded(1);
        
        let stream = Stream {
            read_available_notifier,
            write_available_notifier,
            incoming_response_handler: None,
            outgoing_response_handler: None,
            processing_tasks: vec![],
            sent_end_of_stream: false,
            state: Arc::new(Mutex::new(StreamState {
                // weight: 16, // Default weight
                // dependency: 0,

                error: None,
                
                local_window: connection_state.local_settings[SettingId::INITIAL_WINDOW_SIZE] as WindowSize,
                remote_window: connection_state.remote_settings[SettingId::INITIAL_WINDOW_SIZE] as WindowSize,

                received_buffer: vec![],
                received_trailers: None,
                received_end_of_stream: false,
                received_expected_bytes: None,
                received_total_bytes: 0,

                sending_buffer: vec![],
                sending_trailers: None,
                sending_at_end: false
            }))
        };

        let incoming_body = IncomingStreamBody {
            stream_id,
            stream_state: stream.state.clone(),
            connection_event_sender: self.connection_event_channel.0.clone(),
            read_available_receiver,
            expected_length: None
        };

        let outgoing_body = OutgoingStreamBody {
            stream_id,     
            stream_state: stream.state.clone(),
            connection_event_sender: self.connection_event_channel.0.clone(),
            write_available_receiver
        };

        (stream, incoming_body, outgoing_body)
    }

    pub async fn request_handler_driver(self: Arc<ConnectionShared>, stream_id: StreamId, request: Request) {
        let request_handler = self.request_handler.as_ref().unwrap();

        let response = request_handler.handle_request(request).await;

        let _ = self.connection_event_channel.0.send(ConnectionEvent::SendResponse {
            stream_id: stream_id,
            response
        }).await;

        // TODO: Consider starting the processing task for reading the outgoing body here.
        // This will require us to validate the stream is still open, but this may help with latency.
    }
}