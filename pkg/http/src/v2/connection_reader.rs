use std::sync::Arc;

use common::errors::*;
use common::io::Readable;
use common::task::ChildTask;

use crate::v2::types::*;
use crate::v2::connection_state::*;
use crate::hpack;
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
    /// Rather than returning a Result<()>, when the read thread fails, it will notify the
    /// ConnectionWriter be sending ConnectionEvent::Goaway or ConnectionEvent::Closing events
    /// and terminate.
    pub async fn run(self, reader: Box<dyn Readable>, skip_preface_head: bool) {
        let result = self.run_inner(reader, skip_preface_head).await;
        
        match result {
            Ok(()) => {
                // TODO: Will this ever happen?
                let _ = self.shared.connection_event_sender.send(ConnectionEvent::Closing { error: Some(ProtocolErrorV2 {
                    code: ErrorCode::PROTOCOL_ERROR,
                    message: "Reader thread ended",
                    local: false
                }) }).await;
            }
            Err(e) => {
                // TODO: Improve reporting of these errors up the call chain.
                println!("HTTP2 READ THREAD FAILED: {:?}", e);

                let proto_error = if let Some(e) = e.downcast_ref::<ProtocolErrorV2>() {
                    // We don't need to send a GOAWAY from remotely generated errors.
                    if !e.local {
                        let _ = self.shared.connection_event_sender.send(ConnectionEvent::Closing {
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
                let _ = self.shared.connection_event_sender.send(ConnectionEvent::Goaway {
                    error: proto_error,
                    last_stream_id
                }).await;
            }
        }
    }

    // TODO: According to RFC 7540 Section 4.1, undefined flags should be left as zeros.

    // NOTE: Will return an Ok(()) if and only if the 
    async fn run_inner(&self, mut reader: Box<dyn Readable>,
                       seen_preface_head: bool) -> Result<()> {
        // Server endpoints need to read the preface sent by the client.
        if self.shared.is_server {
            let preface_str = if seen_preface_head { CONNECTION_PREFACE_BODY } else { CONNECTION_PREFACE };

            let mut preface = [0u8; CONNECTION_PREFACE.len()];
            reader.read_exact(&mut preface[0..preface_str.len()]).await?;
            if &preface[0..preface_str.len()] != preface_str {
                return Err(err_msg("Incorrect preface received"));
            }
        }

        // TODO: Receiving any packet on a stream with a smaller number than any stream id ever seen
        // should casue an error.

        // TODO: Ensure that the first frame received is a Settings (non-ack)

        // Used to decode remotely created headers received on the connection.
        // NOTE: This is shared across all streams on the connection.
        let mut remote_header_decoder;

        let mut max_frame_size;

        // Loading the above two variables from local settings.
        // NOTE: Because settings only change when they are acknowledged on this thread,
        // these are straight forward to keep in sync.
        {
            let state = self.shared.state.lock().await;
            remote_header_decoder = hpack::Decoder::new(
                state.local_settings[SettingId::HEADER_TABLE_SIZE] as usize);
            max_frame_size = state.local_settings[SettingId::MAX_FRAME_SIZE];
        }



        // Whether or not we've received a non-ACK SETTINGS frame from the other endpoint yet.
        // (we expect the first frame to the other endpoint to be a non-ACK SETTINGS frame or
        // an error).
        let mut received_remote_settings = false;

        // The current partially completed headers block if any. This will be set with sequence
        // of (HEADERS|PUSH_PROMISE) CONTINUATION* frames without END_HEADERS set.
        //
        // When this is not None, we only allow CONTINUATION frames from the same stream to be
        // received and no other frames on other streams. 
        let mut pending_header: Option<ReceivedHeaders> = None;

        let mut frame_header_buf = [0u8; FrameHeader::size_of()];
        loop {
            // Read the frame header
            if let Err(e) = reader.read_exact(&mut frame_header_buf).await {
                if let Some(io_error) = e.downcast_ref::<std::io::Error>() {
                    if io_error.kind() == std::io::ErrorKind::ConnectionAborted {
                        let connection_state = self.shared.state.lock().await;
                        if connection_state.streams.is_empty() {
                            return Ok(());
                        }
                    }
                }

                return Err(e);
            }

            let frame_header = FrameHeader::parse_complete(&frame_header_buf)?;

            println!("READ FRAME {:?}", frame_header.typ);

            // Enforce that the first frame is SETTINGS
            if !received_remote_settings &&
                frame_header.typ != FrameType::SETTINGS &&
                frame_header.typ != FrameType::GOAWAY {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::PROTOCOL_ERROR,
                    message: "Expected first received frame to be a SETTINGS frame",
                    local: true
                }.into());
            }

            /*
            idle:
                HEADERS | PRIORITY
            reserved (local)
                RST_STREAM | PRIORITY | WINDOW_UPDATE
            reserved (remote)
                HEADERS | RST_STREAM | PRIORITY

            open
                ?
            half-closed (local)
                (any)

            */

            // Idle state check.
            // Only 
            {
                let state = self.shared.state.lock().await;
                
                let mut idle = {
                    if frame_header.stream_id == 0 {
                        false
                    } else if self.shared.is_local_stream_id(frame_header.stream_id) {
                        frame_header.stream_id > state.last_sent_stream_id
                    } else { // is_local_stream_id
                        frame_header.stream_id > state.last_received_stream_id
                    }
                };

                if let Some(h) = &pending_header {
                    if h.stream_id == frame_header.stream_id {
                        idle = false;
                    }
                }

                if idle && frame_header.typ != FrameType::HEADERS && frame_header.typ != FrameType::PRIORITY {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "Received unallowed frame type for idle stream",
                        local: true
                    }.into());
                }
            }

            // Validate frame size based on procedure in RFC 7540: Section 4.2.
            if frame_header.length > max_frame_size {
                let can_alter_state =
                    frame_header.typ == FrameType::SETTINGS ||
                    frame_header.typ == FrameType::HEADERS ||
                    frame_header.typ == FrameType::PUSH_PROMISE ||
                    frame_header.typ == FrameType::CONTINUATION ||
                    frame_header.stream_id == 0;
                
                if can_alter_state {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received state altering frame larger than max frame size",
                        local: true
                    }.into());
                } else {
                    let mut connection_state = self.shared.state.lock().await;

                    self.shared.finish_stream(&mut connection_state, frame_header.stream_id, Some(ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received frame larger than max frame size",
                        local: true
                    })).await;
                }

                // Skip over this frame's payload by just reading into a waste buffer until we reach
                // the end of the packet.
                let mut num_remaining = frame_header.length as usize;
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

            // Enforce header block frames not interleaving with others.
            // This error is defined in RFC 7540: Section 6.10
            if let Some(received_header) = &pending_header {
                if frame_header.stream_id != received_header.stream_id ||
                   frame_header.typ != FrameType::CONTINUATION {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "Non-continuation frames interleaved in header block",
                        local: true
                    }.into());    
                }
            }

            // TODO: Read this on demand as we identify what it's needed for so that we can just copy it into the final
            // buffer all at once.
            let mut payload = vec![];
            // TODO: Should validate earlier that MAX_FRAME_SIZE is <= usize::max
            payload.resize(frame_header.length as usize, 0);
            reader.read_exact(&mut payload).await?;

            match frame_header.typ {
                FrameType::DATA => {
                    if frame_header.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "DATA frame received on the connection control stream.",
                            local: true
                        }.into());
                    }

                    // TODO: If we receive DATA on a higher stream id, should we record it in last_received_stream_id to
                    // ensure that we can't receive a HEADERS later on that stream.
                    // ^ Basically ensure all RST_STREAM errors are converted into a permanent rejection of that stream id?

                    let data_flags = DataFrameFlags::parse_complete(&[frame_header.flags])?;
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
                    if connection_state.local_connection_window < (frame_header.length as WindowSize) {
                        // TODO: Should we still 
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FLOW_CONTROL_ERROR,
                            message: "Exceeded connection level window",
                            local: true
                        }.into());
                    }

                    // Update the local connection level window. This occurs even if the frame
                    // fails later down to ensure that it stays in sync with the remote endpoint.
                    connection_state.local_connection_window += frame_header.length as WindowSize;

                    let stream = match connection_state.streams.get(&frame_header.stream_id) {
                        Some(s) => s,
                        None => {
                            // TODO: In this case I still need to update the replenish the
                            // connection flow control window

                            // According to Section 6.1, we must send a STREAM_CLOSED if receiving
                            // a DATA frame on a non-"open" or "half-closed (local)" stream.
                            //
                            // This handles "closed"/"idle" cases. Other states will be checked in
                            // stream.receive_data().
                            println!("SEND STREAM CLOSED");
                            let _ = self.shared.connection_event_sender.send(ConnectionEvent::ResetStream {
                                stream_id: frame_header.stream_id,
                                error: ProtocolErrorV2 {
                                    code: ErrorCode::STREAM_CLOSED,
                                    message: "Received data on a closed stream",
                                    local: true
                                }
                            }).await;

                            if frame_header.length != 0 {
                                let _ = self.shared.connection_event_sender.send(ConnectionEvent::StreamRead {
                                    stream_id: 0,
                                    count: (frame_header.length as usize)
                                }).await;
                            }

                            continue;
                        } 
                    };

                    let mut stream_state = stream.state.lock().await;

                    let extra_flow_controlled_bytes = (frame_header.length as usize) - data_frame.data.len();

                    // TODO: Must refactor this and everything else to include padding in all the
                    // flow control calculations.
                    stream.receive_data(
                        &data_frame.data, extra_flow_controlled_bytes, 
                        data_flags.end_stream, &mut stream_state);
                    if stream_state.error.is_some() {
                        // Data frame was rejected. 
                        // We can allow the other endpoint to send more.
                        if frame_header.length != 0 {
                            let _ = self.shared.connection_event_sender.send(ConnectionEvent::StreamRead {
                                stream_id: 0,
                                count: (frame_header.length as usize)
                            }).await;
                        }
                    } else {
                        // We discard the all the payload except the inner data, so we can given
                        // back flow control quota for any padding in the frame.
                        if extra_flow_controlled_bytes != 0 {
                            let _ = self.shared.connection_event_sender.send(ConnectionEvent::StreamRead {
                                stream_id: 0,
                                count: extra_flow_controlled_bytes
                            }).await;
                        }
                    }

                    let stream_closed = stream.is_closed(&stream_state);
                    drop(stream_state);
                    drop(stream);

                    if stream_closed {
                        self.shared.finish_stream(&mut connection_state, frame_header.stream_id, None).await;
                    }
                }
                FrameType::HEADERS => {
                    let headers_flags = HeadersFrameFlags::parse_complete(&[frame_header.flags])?;
                    let headers_frame = HeadersFramePayload::parse_complete(
                        &payload, &headers_flags)?;
                    frame_utils::check_padding(&headers_frame.padding)?;
 
                    // TODO: Check early which stream id is used?

                    let received_headers = ReceivedHeaders {
                        data: headers_frame.header_block_fragment,
                        stream_id: frame_header.stream_id,
                        typ: ReceivedHeadersType::RegularHeaders {
                            end_stream: headers_flags.end_stream,
                            priority: headers_frame.priority
                        }
                    };

                    if headers_flags.end_headers {
                        self.receive_headers(received_headers, &mut remote_header_decoder).await?;
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::PRIORITY => {

                }
                FrameType::RST_STREAM => {
                    if frame_header.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received RST_STREAM frame on connection control stream",
                            local: true
                        }.into());
                    }

                    if (frame_header.length as usize) != RstStreamFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received RST_STREAM frame of wrong length",
                            local: true
                        }.into());
                    }

                    let rst_stream_frame = RstStreamFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;
                    
                    if (self.shared.is_local_stream_id(frame_header.stream_id) &&
                        frame_header.stream_id > connection_state.last_sent_stream_id) ||
                       (self.shared.is_remote_stream_id(frame_header.stream_id) &&
                        frame_header.stream_id > connection_state.last_received_stream_id) {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received RST_STREAM for idle stream",
                            local: true
                        }.into());
                    }

                    self.shared.finish_stream(&mut connection_state, frame_header.stream_id, Some(ProtocolErrorV2 {
                        code: rst_stream_frame.error_code,
                        message: "Recieved RST_STREAM",
                        local: false
                    })).await;
                }
                FrameType::SETTINGS => {
                    if frame_header.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received SETTINGS frame on non-connection control stream",
                            local: true
                        }.into());
                    }

                    if (frame_header.length % 6) != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received SETTINGS frame of wrong length",
                            local: true
                        }.into());
                    }

                    let settings_flags = SettingsFrameFlags::parse_complete(&[frame_header.flags])?;
                    // TODO: This seems to fial?
                    let settings_frame = SettingsFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;

                    if settings_flags.ack {
                        if frame_header.length != 0 {
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

                        // TODO: Group together all of these variables which need to be synced to the settings.
                        remote_header_decoder.set_protocol_max_size(
                            connection_state.local_settings[SettingId::HEADER_TABLE_SIZE] as usize);
                        max_frame_size = connection_state.local_settings[SettingId::MAX_FRAME_SIZE];

                    } else {
                        received_remote_settings = true;

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

                                // NOTE: As we validate that this parameter is always < 2^32,
                                // this should never overflow.
                                let window_diff = (param.value as WindowSize) - (old_value.unwrap_or(0) as WindowSize);

                                for (stream_id, stream) in &connection_state.streams {
                                    let mut stream_state = stream.state.lock().await;

                                    if stream_state.sending_end && stream_state.sending_buffer.is_empty() {
                                        continue;
                                    }

                                    stream_state.remote_window = stream_state.remote_window.checked_add(
                                        window_diff).ok_or_else(|| Error::from(ProtocolErrorV2 {
                                            code: ErrorCode::FLOW_CONTROL_ERROR,
                                            message: "Change in INITIAL_WINDOW_SIZE cause a window to overflow",
                                            local: true
                                        }))?;

                                    // The window size change may have make it possible for more data to be send.
                                    let _ = stream.write_available_notifier.try_send(());
                                }

                                // Force a re-check of whether or not more data can be sent.
                                self.shared.connection_event_sender.send(ConnectionEvent::StreamWrite { stream_id: 0 }).await;
                            }
                        }

                        self.shared.connection_event_sender.send(ConnectionEvent::AcknowledgeSettings { header_table_size }).await;
                    }
                }
                FrameType::PUSH_PROMISE => {

                }
                FrameType::PING => {
                    if frame_header.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received PING message on non-connection control stream",
                            local: true
                        }.into());
                    }

                    if (frame_header.length as usize) != PingFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received PING message of wrong length",
                            local: true
                        }.into());
                    }

                    let ping_flags = PingFrameFlags::parse_complete(&[frame_header.flags])?;
                    let ping_frame = PingFramePayload::parse_complete(&payload)?;

                    if ping_flags.ack {
                        // TODO
                    } else {
                        // TODO: Block if too many pings in a short period of time.
                        self.shared.connection_event_sender.send(ConnectionEvent::Ping { ping_frame }).await
                            .map_err(|_| err_msg("Writer thread closed"))?;
                    }

                }
                FrameType::GOAWAY => {
                    if frame_header.stream_id != 0 {
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

                    // Basically what I need from the stream state is

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

                    } else {

                        // Need to reset all the streams!

                        // Need to return an error but shouldn't ask the writer thread to repeat it.

                        return Ok(());
                    }
                    // 
                    // Send a notification to the other side that we need to GOAWAY

                }
                FrameType::WINDOW_UPDATE => {
                    if (frame_header.length as usize) != WindowUpdateFramePayload::size_of() {
                        // Connection error: FRAME_SIZE_ERROR
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received WINDOW_UPDATE message of wrong length",
                            local: true
                        }.into());
                    }

                    // TODO: Should we block these if received on an idle stream.

                    let window_update_frame = WindowUpdateFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;

                    if window_update_frame.window_size_increment == 0 {
                        let error = ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received WINDOW_UPDATE with invalid 0 increment",
                            local: true
                        };

                        if frame_header.stream_id == 0 {
                            return Err(error.into());
                        }

                        // TODO: Send the RST_STREAM even if the stream is unknown?

                        self.shared.finish_stream(
                            &mut connection_state, frame_header.stream_id,
                            Some(error)).await;

                        continue;
                    }

                    if frame_header.stream_id == 0 {
                        connection_state.remote_connection_window = connection_state.remote_connection_window.checked_add(window_update_frame.window_size_increment as WindowSize).ok_or_else(|| ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Overflow in connection flow control window size",
                            local: true
                        })?;
                    } else if let Some(stream) = connection_state.streams.get(&frame_header.stream_id) {
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
                    let mut received_headers = match pending_header.take() {
                        Some(v) => v,
                        None => {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message: "Expected a HEADERS or PUSH_PROMISE to precede a CONTINUATION frame",
                                local: true
                            }.into());
                        }
                    };
                    
                    let continuation_flags = ContinuationFrameFlags::parse_complete(&[frame_header.flags])?;

                    // NOTE: The entire payload is a header chunk.
                    // TODO: Enforce a max size to the combined header data.
                    received_headers.data.extend_from_slice(&payload);

                    if continuation_flags.end_headers {
                        self.receive_headers(received_headers, &mut remote_header_decoder).await?;
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

    async fn receive_headers(&self, received_headers: ReceivedHeaders, remote_header_decoder: &mut hpack::Decoder) -> Result<()> {
        // TODO: Check all this logic against the RFC. Right now it's mostly implemented based
        // on common sense.

        // TODO: Make sure that the stream id is non-zero

        // First deserialize all the headers so that they definately get applied to the production state.
        // TODO: Preserve the original error message and log internally?
        let headers = remote_header_decoder.parse_all(&received_headers.data)
            .map_err(|_| ProtocolErrorV2 {
                code: ErrorCode::COMPRESSION_ERROR,
                message: "Failure while decoding receivers header fragment",
                local: true
            })?;

        let mut connection_state = self.shared.state.lock().await;

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
                        // XXX: receive_trailers()
                        let mut stream_state = stream.state.lock().await;

                        stream.receive_trailers(headers, end_stream, &mut stream_state);

                        if stream.is_closed(&stream_state) {
                            println!("TRAILERS TRIGGERED SHUTDOWN");
                            drop(stream_state);
                            self.shared.finish_stream(&mut connection_state, received_headers.stream_id, None).await;
                        }
                
                        return Ok(());

                    }

                    if connection_state.remote_stream_count >= connection_state.local_settings[SettingId::MAX_CONCURRENT_STREAMS] as usize {
                        // Send a REFUSED_STREAM stream error (or as described in 5.1.2, PROTOCOL_ERROR is also allowed if we don't want it to be retryable)..
                        // TODO: Should we disallow using this stream id in the future?

                        self.shared.connection_event_sender.send(ConnectionEvent::ResetStream {
                            stream_id: received_headers.stream_id,
                            error: ProtocolErrorV2 {
                                code: ErrorCode::REFUSED_STREAM,
                                message: "Exceeded MAX_CONCURRENT_STREAMS",
                                local: true,
                            }
                        }).await;
                        return Ok(());
                    }

                    // When receiving headers on server,
                    // - end stream means the remote stream is done.

                    // TODO: Only do this if the stream is successfully started?
                    connection_state.last_received_stream_id = received_headers.stream_id;
                    connection_state.remote_stream_count += 1;

                    // Make new a new stream

                    let (mut stream, incoming_body, outgoing_body) = self.shared.new_stream(
                        &connection_state,  received_headers.stream_id);

                    let mut stream_state = stream.state.lock().await;
                    
                    if let Some(request) = stream.receive_request(headers, end_stream, incoming_body, &mut stream_state) {
                        stream.outgoing_response_handler = Some(outgoing_body);

                        stream.processing_tasks.push(ChildTask::spawn(self.shared.clone().request_handler_driver(
                            received_headers.stream_id, request)));
                    }
                    
                    let stream_closed = stream.is_closed(&stream_state);
                    drop(stream_state);

                    connection_state.streams.insert(received_headers.stream_id, stream);
                    
                    if stream_closed {
                        self.shared.finish_stream(&mut connection_state, received_headers.stream_id, None).await;
                    }
                } else {
                    // TODO: A client can receive trailers on both a 
                    if !self.shared.is_local_stream_id(received_headers.stream_id) ||
                       received_headers.stream_id < connection_state.last_sent_stream_id {
                        // Error
                    }

                    // TODO: Don't accept new streams when shutting down.

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

                    let mut stream_state = stream.state.lock().await;

                    if let Some((request_method, response_handler, incoming_body)) =
                        stream.incoming_response_handler.take() {

                        if let Some(response) = stream.receive_response(
                            request_method, headers, end_stream, incoming_body, &mut stream_state) {
                            response_handler.handle_response(Ok(response)).await;
                        } else {
                            // TODO: Use the stream error.
                            response_handler.handle_response(Err(err_msg("Failed"))).await;
                        }

                    } else {
                        // Otherwise we just received trailers.
                        stream.receive_trailers(headers, end_stream, &mut *stream_state);
                    }

                    let stream_closed = stream.is_closed(&stream_state);
                    drop(stream_state);

                    if stream_closed {
                        self.shared.finish_stream(&mut connection_state, received_headers.stream_id, None).await;
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