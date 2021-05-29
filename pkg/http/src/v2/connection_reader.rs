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
use crate::v2::connection_shared::*;

enum ReceivedHeadersType {
    PushPromise {
        promised_stream_id: StreamId,
    },
    RegularHeaders {
        end_stream: bool,
        priority: Option<PriorityFramePayload>
    }
}

struct ReceivedHeaders {
    /// Id of the stream on which this data was received.
    stream_id: StreamId,

    data: Vec<u8>,

    typ: ReceivedHeadersType
}

pub struct ConnectionReader {
    shared: Arc<ConnectionShared>
}

impl ConnectionReader {
    pub fn new(shared: Arc<ConnectionShared>) -> Self {
        Self { shared }
    }

    /// Runs the thread that is the exlusive reader of incoming data from the raw connection.
    ///
    /// Internal Error handling:
    /// - If a connection error occurs, this function will return immediately with a non-ok result.
    ///   The caller should communicate this to the 
    ///
    /// External Error Handling:
    /// - The caller should cancel this future when it wants to 
    pub async fn run(self, reader: Box<dyn Readable>, skip_preface_head: bool) {
        let result = self.run_inner(reader, skip_preface_head).await;
        
        match result {
            Ok(()) => {
                // TODO: Will this ever happen?
                let _ = self.shared.connection_event_channel.0.send(ConnectionEvent::Closing { error: None }).await;
            }
            Err(e) => {
                // TODO: Improve reporting of these errors up the call chain.
                println!("HTTP2 READ THREAD FAILED: {:?}", e);

                let proto_error = if let Some(e) = e.downcast_ref::<ProtocolErrorV2>() {
                    // We don't need to send a GOAWAY from remotely generated errors.
                    if !e.local {
                        let _ = self.shared.connection_event_channel.0.send(ConnectionEvent::Closing {
                            error: Some(e.clone())
                        }).await;
                        return;
                    }

                    e.clone()
                } else {
                    ProtocolErrorV2 {
                        code: ErrorCode::INTERNAL_ERROR,
                        message: "Unknown internal error occured",
                        local: true
                    }
                };

                let last_stream_id = {
                    let connection_state = self.shared.state.lock().await;
                    connection_state.last_received_stream_id
                };

                // TODO: Who should be responislbe for marking the connection_state.error?
                let _ = self.shared.connection_event_channel.0.send(ConnectionEvent::Goaway {
                    error: proto_error,
                    last_stream_id
                }).await;
            }
        }
    }

    // NOTE: Will return an Ok(()) if and only if the 
    async fn run_inner(&self, mut reader: Box<dyn Readable>,
                       seen_preface_head: bool) -> Result<()> {
        if self.shared.is_server {
            let preface_str = if seen_preface_head { CONNECTION_PREFACE_BODY } else { CONNECTION_PREFACE };

            let mut preface = [0u8; CONNECTION_PREFACE.len()];
            reader.read_exact(&mut preface[0..preface_str.len()]).await?;
            if &preface[0..preface_str.len()] != preface_str {
                return Err(err_msg("Incorrect preface received"));
            }
        }

        // TODO: Don't allow starting any new streams until we've gotten the remote settings 

        // If the read thread fails, we should tell the write thread to complain about an error.
        // Likewise we need to be able to send other types of events to the write thread.

        // TODO: Receiving any packet on a stream with a smaller number than any stream id ever seen
        // should casue an error.

        // Promised request:
        // (promised_stream_id)

        let mut pending_header: Option<ReceivedHeaders> = None;

        let mut frame_header_buf = [0u8; FrameHeader::size_of()];
        loop {
            reader.read_exact(&mut frame_header_buf).await?;

            let header_frame = FrameHeader::parse_complete(&frame_header_buf)?;

            println!("Got frame: {:?}", header_frame.typ);

            let max_frame_size = {
                let state = self.shared.state.lock().await;
                state.local_settings[SettingId::MAX_FRAME_SIZE]
            };

            // TODO: first frame must always be the Settings frame 
            

            // Error handling based on RFC 7540: Section 4.2
            if header_frame.length > max_frame_size {
                let can_alter_state =
                    header_frame.typ == FrameType::SETTINGS ||
                    header_frame.typ == FrameType::HEADERS ||
                    header_frame.typ == FrameType::PUSH_PROMISE ||
                    header_frame.typ == FrameType::CONTINUATION ||
                    header_frame.stream_id == 0;
                
                if can_alter_state {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received state altering frame larger than max frame size",
                        local: true
                    }.into());
                } else {
                    self.shared.send_reset_stream(header_frame.stream_id, ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received frame larger than max frame size",
                        local: true
                    }).await?;
                }

                // Skip over this frame's payload by just reading into a waste buffer until we reach
                // the end of the packet.
                let mut num_remaining = header_frame.length as usize;
                while num_remaining > 0 {
                    let mut buf = [0u8; 512];

                    let max_to_read = std::cmp::min(num_remaining, buf.len());
                    let n = reader.read(&mut buf[0..max_to_read]).await?;
                    num_remaining -= n;

                    if n == 0 {
                        return Ok(());
                    }
                }

                continue;
            }

            if let Some(received_header) = &pending_header {
                if header_frame.stream_id == received_header.stream_id ||
                   header_frame.typ != FrameType::CONTINUATION {
                    // TODO: Verify that this is the right error code.
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "",
                        local: true
                    }.into());    
                }
            }

            // TODO: Read this on demand as we identify what it's needed for so that we can just copy it into the final
            // buffer all at once.
            let mut payload = vec![];
            // TODO: Should validate earlier that MAX_FRAME_SIZE is <= usize::max
            payload.resize(header_frame.length as usize, 0);
            reader.read_exact(&mut payload).await?;

            match header_frame.typ {
                FrameType::DATA => {
                    if header_frame.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "DATA frame received on the connection control stream.",
                            local: true
                        }.into());
                    } 

                    let data_flags = DataFrameFlags::parse_complete(&[header_frame.flags])?;
                    let data_frame = DataFramePayload::parse_complete(&payload, &data_flags)?;
                    frame_utils::check_padding(&data_frame.padding)?;

                    /*
                    If a DATA frame is received
                    whose stream is not in "open" or "half-closed (local)" state, the
                    recipient MUST respond with a stream error (Section 5.4.2) of type
                    STREAM_CLOSED.
                    */

                    // Verify stream exists (not still applies to flow control)
                    // Check if remotely closed. Even if closed, we still need it to count towards flow control (so we may want to tell the remote endpoint that more data is available)


                    let mut connection_state = self.shared.state.lock().await;
                    if connection_state.local_connection_window < (header_frame.length as WindowSize) {
                        // TODO: Should we still 
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FLOW_CONTROL_ERROR,
                            message: "Exceeded connection level window",
                            local: true
                        }.into());
                    }

                    connection_state.local_connection_window += header_frame.length as WindowSize;

                    // if payload.len() > 

                    // Push data into receiving buffer / update 


                    let stream = match connection_state.streams.get(&header_frame.stream_id) {
                        Some(s) => s,
                        None => {
                            // Send a STREAM_CLOSED 
                            continue;
                        } 
                    };

                    let mut stream_state = stream.state.lock().await;

                    stream_state.received_total_bytes += data_frame.data.len();
                    if data_flags.end_stream {
                        if let Some(expected_size) = stream_state.received_expected_bytes.clone() {
                            if expected_size != stream_state.received_total_bytes {
                                // Error. Mismatch between content-length and data framing.
                            }
                        }
                    }

                    if stream_state.received_end_of_stream {
                        // Send a STREAM_CLOSED error (should be sent even if the stream is closed??)
                    }

                    if stream_state.local_window < (header_frame.length as WindowSize) {
                        // Send a RST_STREAM
                    }
                    stream_state.local_window += header_frame.length as WindowSize;


                    stream.receive_data(&data_frame.data, data_flags.end_stream, &mut stream_state);

                    let stream_closed = stream_state.is_closed();
                    drop(stream_state);
                    drop(stream);

                    if stream_closed {
                        self.shared.finish_stream(&mut connection_state, header_frame.stream_id);
                    }
                }
                FrameType::HEADERS => {
                    let headers_flags = HeadersFrameFlags::parse_complete(&[header_frame.flags])?;
                    let headers_frame = HeadersFramePayload::parse_complete(
                        &payload, &headers_flags)?;
                    frame_utils::check_padding(&headers_frame.padding)?;
 
                    // TODO: Check early which stream id is used?

                    let received_headers = ReceivedHeaders {
                        data: headers_frame.header_block_fragment,
                        stream_id: header_frame.stream_id,
                        typ: ReceivedHeadersType::RegularHeaders {
                            end_stream: headers_flags.end_stream,
                            priority: headers_frame.priority
                        }
                    };

                    if headers_flags.end_headers {
                        self.receive_headers(received_headers).await?;
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::PRIORITY => {

                }
                FrameType::RST_STREAM => {
                    if header_frame.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received RST_STREAM frame on connection control stream",
                            local: true
                        }.into());
                    }

                    if (header_frame.length as usize) != RstStreamFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received RST_STREAM frame of wrong length",
                            local: true
                        }.into());
                    }

                    {
                        let connection_state = self.shared.state.lock().await;
                        if (self.shared.is_local_stream_id(header_frame.stream_id) && header_frame.stream_id > connection_state.last_sent_stream_id) ||
                            (self.shared.is_remote_stream_id(header_frame.stream_id) && header_frame.stream_id > connection_state.last_received_stream_id) {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received RST_STREAM for idle stream",
                                local: true
                            }.into());
                        }
                    }

                    let rst_stream_frame = RstStreamFramePayload::parse_complete(&payload)?;

                    self.shared.recv_reset_stream(header_frame.stream_id, ProtocolErrorV2 {
                        code: rst_stream_frame.error_code,
                        message: "",
                        local: true
                    }).await;
                }
                FrameType::SETTINGS => {
                    if header_frame.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received SETTINGS frame on non-connection control stream",
                            local: true
                        }.into());
                    }

                    if header_frame.length % 6 != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received SETTINGS frame of wrong length",
                            local: true
                        }.into());
                    }

                    let settings_flags = SettingsFrameFlags::parse_complete(&[header_frame.flags])?;
                    let settings_frame = SettingsFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;

                    if settings_flags.ack {
                        if header_frame.length != 0 {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::FRAME_SIZE_ERROR,
                                message: "Received SETTINGS ACK with non-zero length",
                                local: true
                            }.into());
                        }

                        if let Some(waiter) = connection_state.local_settings_ack_waiter.take() {
                            // Stop waiting
                            drop(waiter);
                        } else {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received SETTINGS ACK while no settings where pending ACK",
                                local: true
                            }.into());
                        }

                        // TODO: Apply any other state changes that are needed.
                        connection_state.local_settings = connection_state.local_pending_settings.clone();
                    } else {

                        let mut header_table_size = None;

                        // Apply the settings.
                        for param in settings_frame.parameters {
                            let old_value = connection_state.remote_settings.set(param.id, param.value)?;

                            if param.id == SettingId::HEADER_TABLE_SIZE {
                                // NOTE: This will be applied on the writer thread as it needs to ACK'ed in lock step
                                // with any usages of the header encoder.
                                header_table_size = Some(param.value);
                            } else if param.id == SettingId::INITIAL_WINDOW_SIZE {
                                // NOTE: Changes to this parameter DO NOT update the 

                                let window_diff = (param.value - old_value.unwrap_or(0)) as WindowSize;

                                for (stream_id, stream) in &connection_state.streams {
                                    let mut stream_state = stream.state.lock().await;

                                    if stream_state.sending_at_end && stream_state.sending_buffer.is_empty() {
                                        continue;
                                    }

                                    stream_state.remote_window = stream_state.remote_window.checked_add(
                                        window_diff).ok_or_else(|| Error::from(ProtocolErrorV2 {
                                            code: ErrorCode::FLOW_CONTROL_ERROR,
                                            message: "Change in INITIAL_WINDOW_SIZE cause a window to overflow",
                                            local: true
                                        }))?;

                                    // The window size change may have make it possible for more data to be send.
                                    stream.write_available_notifier.try_send(());
                                }

                                // Force a re-check of whether or not more data can be sent.
                                self.shared.connection_event_channel.0.send(ConnectionEvent::StreamWrite { stream_id: 0 }).await;
                            }
                        }

                        self.shared.connection_event_channel.0.send(ConnectionEvent::AcknowledgeSettings { header_table_size }).await;
                    }
                }
                FrameType::PUSH_PROMISE => {

                }
                FrameType::PING => {
                    if header_frame.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received PING message on non-connection control stream",
                            local: true
                        }.into());
                    }

                    if (header_frame.length as usize) != PingFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received PING message of wrong length",
                            local: true
                        }.into());
                    }

                    let ping_flags = PingFrameFlags::parse_complete(&[header_frame.flags])?;
                    let ping_frame = PingFramePayload::parse_complete(&payload)?;

                    if ping_flags.ack {

                    } else {
                        // TODO: Block if too many pings in a short period of time.
                        self.shared.connection_event_channel.0.send(ConnectionEvent::Ping { ping_frame }).await
                            .map_err(|_| err_msg("Writer thread closed"))?;
                    }

                }
                FrameType::GOAWAY => {
                    if header_frame.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received GOAWAY frame on non-connection control stream",
                            local: true
                        }.into());
                    }

                    // TODO: When a server gracefully shuts down, follow the guidance in Section 6.8

                    /* 
                    TODO: Is this a mandatory part of every implementation:
                    After sending a GOAWAY frame, the sender can discard frames for
                    streams initiated by the receiver with identifiers higher than the
                    identified last stream. 
                    */

                    let goaway_frame = GoawayFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;

                    // TODO: We need to be much more consistent about always setting this.
                    // TODO: We need to uphold the gurantee that while this is None, an incoming request is guranteed to be
                    // processed.
                    // TODO: Verify that once this is set, we won't generate any new streams.
                    connection_state.error = Some(ProtocolErrorV2 {
                        code: goaway_frame.error_code,
                        message: "Remote GOAWAY received", // TODO: Read the opaque message from the remote packet.
                        local: true
                    });

                    // All streams will ids >= last_stream_id we should report as retryable.
                    // For all other streams:
                    //   If there is a forceful failure, mark them as fully closed.

                    // If we have no error, 


                    if goaway_frame.error_code == ErrorCode::NO_ERROR {
                        // Graceful shutdown.

                        for (stream_id, stream) in &connection_state.streams {
                            // TODO: Only applies to locally initialized streams?
                            if self.shared.is_local_stream_id(*stream_id) && *stream_id > goaway_frame.last_stream_id {
                                // Reset the stream with a 'retryable' error.
                                // Main challenge is deadling with locks.
                            }
                        }

                        while let Some(req) = connection_state.pending_requests.pop_front() {
                            req.response_handler.handle_response(Err(ProtocolErrorV2 {
                                code: ErrorCode::REFUSED_STREAM,
                                message: "Connection shutting down",
                                local: false,
                            }.into())).await;
                        }

                        // All 'pending_requests' should 

                    } else {

                        // Need to reset all the streams!

                        // Need to return an error but shouldn't ask the writer thread to repeat it.

                        return Ok(());
                    }
                    // 
                    // Send a notification to the other side that we need to GOAWAY

                }
                FrameType::WINDOW_UPDATE => {
                    if (header_frame.length as usize) != WindowUpdateFramePayload::size_of() {
                        // Connection error: FRAME_SIZE_ERROR
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received WINDOW_UPDATE message of wrong length",
                            local: true
                        }.into());
                    }

                    // TODO: Should we block these if received on an idle frame.

                    let window_update_frame = WindowUpdateFramePayload::parse_complete(&payload)?;
                    if window_update_frame.window_size_increment == 0 {
                        if header_frame.stream_id == 0 {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Received WINDOW_UPDATE with invalid 0 increment",
                                local: true
                            }.into());
                        }

                        // TODO: Send this even if the stream is unknown?
                        self.shared.send_reset_stream(header_frame.stream_id, ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received WINDOW_UPDATE with invalid 0 increment",
                            local: true
                        }).await?;
                        continue;
                    }

                    let mut connection_state = self.shared.state.lock().await;
                    if header_frame.stream_id == 0 {
                        connection_state.remote_connection_window = connection_state.remote_connection_window.checked_add(window_update_frame.window_size_increment as WindowSize).ok_or_else(|| ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Overflow in connection flow control window size",
                            local: true
                        })?;
                    } else if let Some(stream) = connection_state.streams.get(&header_frame.stream_id) {
                        let mut stream_state = stream.state.lock().await;
                        
                        // TODO: Make this just a stream error? 
                        stream_state.remote_window = stream_state.remote_window.checked_add(window_update_frame.window_size_increment as WindowSize).ok_or_else(|| ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Overflow in stream flow control window size",
                            local: true
                        })?;
                    }
                }
                FrameType::CONTINUATION => {
                    let mut received_headers = pending_header.take().unwrap();
                    
                    let continuation_flags = ContinuationFrameFlags::parse_complete(&[header_frame.flags])?;

                    // NOTE: The entire payload is a header chunk.
                    // TODO: Enforce a max size to the combined header data.
                    received_headers.data.extend_from_slice(&payload);

                    if continuation_flags.end_headers {
                        // Process it now.
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::Unknown(_) => {
                    // According to RFC 7540 Section 4.1, unknown frame types should be discarded.
                }
            }
        }
    }

    async fn receive_headers(&self, received_headers: ReceivedHeaders) -> Result<()> {
        // TODO: Check all this logic against the RFC. Right now it's mostly implemented based
        // on common sense.

        // TODO: Make sure that the stream id is non-zero

        let mut connection_state = self.shared.state.lock().await;

        // First deserialize all the headers so that they definately get applied to the production state.
        let headers = connection_state.remote_header_decoder.parse_all(&received_headers.data)?;

        match received_headers.typ {
            ReceivedHeadersType::RegularHeaders { end_stream, priority } => {
                // TODO: Need to implement usage of 'end_stream'

                if self.shared.is_server {
                    if !self.shared.is_remote_stream_id(received_headers.stream_id) ||
                       received_headers.stream_id < connection_state.last_received_stream_id {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Headers block received on non-new remotely initialized stream",
                            local: true
                        }.into());
                    }

                    if connection_state.error.is_some() {
                        // When shutting down, don't accept any new streams.
                        // TODO: Trigger a reset stream, although I guess that isn't needed.
                        return Ok(());
                    }

                    // If receiving headers on an existing stream, this is trailers.
                    // TODO: This could also happen if we performed a 'push promise' and we are receiving data.
                    if let Some(stream) = connection_state.streams.get(&received_headers.stream_id) {
                        if !end_stream {
                            // TODO: Make this a stream error?
                            return Err(err_msg("Received trailers for request without closing stream"));
                        }

                        let mut state = stream.state.lock().await;
                        if state.received_end_of_stream {
                            // TODO: Make this a stream error?
                            return Err(err_msg("Already received end of stream"));
                        }

                        state.received_trailers = Some(process_trailers(headers)?);
                        stream.receive_data(&[], end_stream, &mut state);

                        if state.is_closed() {
                            drop(state);
                            self.shared.finish_stream(&mut connection_state, received_headers.stream_id);
                        }

                        return Ok(());
                    }

                    if connection_state.remote_stream_count >= connection_state.local_settings[SettingId::MAX_CONCURRENT_STREAMS] as usize {
                        // Send a REFUSED_STREAM stream error (or as described in 5.1.2, PROTOCOL_ERROR is also allowed if we don't want it to be retryable)..
                    }

                    // When receiving headers on server,
                    // - end stream means the remote stream is done.

                    // TODO: Only do this if the stream is successfully started?
                    connection_state.last_received_stream_id = received_headers.stream_id;
                    connection_state.remote_stream_count += 1;

                    // Make new a new stream

                    let (mut stream, incoming_body, outgoing_body) = self.shared.new_stream(
                        &connection_state,  received_headers.stream_id);

                    let head = process_request_head(headers)?;
                    let body = create_server_request_body(&head, incoming_body).await?;

                    let request = Request {
                        head,
                        body
                    };

                    // NOTE: We don't need to check if the stream is closed as the local end hasn't even
                    // been started yet.
                    stream.receive_data(&[], end_stream, &mut *stream.state.lock().await);

                    stream.outgoing_response_handler = Some(outgoing_body);

                    stream.processing_tasks.push(ChildTask::spawn(self.shared.clone().request_handler_driver(
                        received_headers.stream_id, request)));

                    connection_state.streams.insert(received_headers.stream_id, stream);

                    
                    // I guess we can start the sending task here (but I'd ideally prefer not to do that).
                    // If the body is empty, we should reflect that in the state.

                    // Need to formulate the Request object, then create the remote stream object and get going on getting the response.

                    // Start the new request
                } else {
                    if !self.shared.is_local_stream_id(received_headers.stream_id) ||
                       received_headers.stream_id < connection_state.last_sent_stream_id {
                        // Error
                    }

                    let stream = match connection_state.streams.get_mut(&received_headers.stream_id) {
                        Some(s) => s,
                        None => {
                            // Most likely we closed the stream, so we can just ignore the headers.
                            return Ok(());
                        } 
                    };
                    // NOTE: Because the stream is still present in the 'streams' map, we know right here
                    // that it isn't closed yet.

                    // TODO: Maybe update priority

                    if let Some((request_method, response_handler, incoming_body)) =
                        stream.incoming_response_handler.take() {

                        // NOTE: 

                        let head = process_response_head(headers)?;
                        let body = create_client_response_body(request_method, &head, incoming_body).await?;

                        let response = Response {
                            head,
                            body
                        };

                        response_handler.handle_response(Ok(response)).await;

                    } else {
                        // Otherwise we just received trailers.
                        
                        let mut stream_state = stream.state.lock().await;

                        // TODO: Check this also for the response headers case?
                        if stream_state.received_end_of_stream {
                            // TODO: Send a stream error.
                        }
                        
                        if !end_stream {
                            // TODO: Pick a bigger type of error
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Expected trailers to end the stream",
                                local: true
                            }.into());
                        }

                        stream_state.received_trailers = Some(process_trailers(headers)?);
                    }

                    let stream_closed;
                    {
                        let mut stream_state = stream.state.lock().await;
                        
                        // TODO: This must always run after we set the trailers.
                        stream.receive_data(&[], end_stream, &mut stream_state);

                        stream_closed = stream_state.is_closed();
                    }

                    if stream_closed {
                        self.shared.finish_stream(&mut connection_state, received_headers.stream_id);
                    }
                }
            }
            ReceivedHeadersType::PushPromise { promised_stream_id } => {
                // TODO: Reserved streams don't count towards the MAX_CONCURRNET_STREAM limit.

                return Err(err_msg("Push promise not yet implemented"));
            }
        }

        Ok(())
    }

}