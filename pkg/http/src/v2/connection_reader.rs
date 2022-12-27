use std::fmt::Write;
use std::sync::Arc;

use common::errors::*;
use common::io::Readable;
use executor::child_task::ChildTask;

use crate::hpack;
use crate::proto::v2::*;
use crate::v2::connection_shared::*;
use crate::v2::connection_state::*;
use crate::v2::frame_utils;
use crate::v2::stream::Stream;
use crate::v2::types::*;

/*
TODO:

Similarly, an endpoint
that receives any frames after receiving a frame with the
END_STREAM flag set MUST treat that as a connection error
(Section 5.4.1) of type STREAM_CLOSED, unless the frame is
permitted as described below.

^ We need to gurantee this on both the sneder and receiver end.
*/

/*
TODO:
The first use of a new stream identifier implicitly closes all
streams in the "idle" state that might have been initiated by that
peer with a lower-valued stream identifier.

This should count in a number of places such as RST_STREAM or PRIORITY
*/

// TODO: One major weakness of all this code right now is that it may deadlock
// if we forget to send the correct events around at the right time.

enum ReceivedHeadersType {
    PushPromise {
        promised_stream_id: StreamId,
    },
    RegularHeaders {
        end_stream: bool,
        priority: Option<PriorityFramePayload>,
    },
}

struct ReceivedHeaders {
    /// Id of the stream on which this data was received.
    stream_id: StreamId,

    data: Vec<u8>,

    typ: ReceivedHeadersType,
}

pub struct ConnectionReader {
    shared: Arc<ConnectionShared>,
}

impl ConnectionReader {
    pub fn new(shared: Arc<ConnectionShared>) -> Self {
        Self { shared }
    }

    /// Runs the thread that is the exlusive reader of incoming data from the
    /// raw connection.
    ///
    /// Rather than returning a Result<()>, when the read thread fails, it will
    /// notify the ConnectionWriter be sending ConnectionEvent::Goaway or
    /// ConnectionEvent::Closing events and terminate.
    pub async fn run(self, reader: Box<dyn Readable>, skip_preface_head: bool) {
        let result = self.run_inner(reader, skip_preface_head).await;

        // Mark that we will never receive any more stream ids.
        // This will be sent to the remote endpoint in any GOAWAYs that we send.
        {
            let mut connection_state = self.shared.state.lock().await;
            connection_state.upper_received_stream_id = connection_state.last_received_stream_id;

            connection_state
                .set_shutting_down(ShuttingDownState::Complete)
                .await;
        }

        match result {
            Ok(()) => {
                // If run_inner returned Ok(), so it should have sent its own Closing message
                // with the real error. So the below send() line isn't necessary
                // and is mainly a guard in the case that nothing was sent.
                //
                // If you see this error returned to you, then there is a bug in run_inner().
                let _ = self
                    .shared
                    .connection_event_sender
                    .send(ConnectionEvent::Closing {
                        send_goaway: None,
                        close_with: Some(Err(err_msg("Reader thread ended without a reason."))),
                    })
                    .await;
            }
            Err(e) => {
                // TODO: Don't always supress these.
                let mut is_reader_ended_error = false;
                if let Some(io_error) = e.downcast_ref::<std::io::Error>() {
                    if io_error.kind() == std::io::ErrorKind::ConnectionAborted
                        || io_error.kind() == std::io::ErrorKind::UnexpectedEof
                    {
                        is_reader_ended_error = true;
                    }
                }

                // If the underlying read stream if closed, then we will simply ask the writer
                // to stop right away. We could alternatively ask the writer to
                // send a Goaway, but most likely if the reader end is closed,
                // the writing end is closed as well. If there are outstanding
                // streams on the connection, they will
                if is_reader_ended_error {
                    let _ = self
                        .shared
                        .connection_event_sender
                        .send(ConnectionEvent::Closing {
                            send_goaway: None,
                            close_with: Some(Ok(())),
                        })
                        .await;
                    return;
                }

                if let Some(proto_error) = e.downcast_ref::<ProtocolErrorV2>() {
                    if proto_error.local {
                        // We locally identified an issue with the packets sent to us from the
                        // remote endpoint. In this case, we assume that the
                        // proto_error.code != INTERNAL.
                        let _ = self
                            .shared
                            .connection_event_sender
                            .send(ConnectionEvent::Closing {
                                send_goaway: Some(proto_error.clone()),
                                close_with: None,
                            })
                            .await;
                    } else {
                        // TODO: When would this case be triggered.

                        // We received a GOAWAY from the remote endpoint so there is no need to
                        // sent a GOAWAY back, but we should consider this a local mistake/error.
                        // NOTE: If we are a server, then whoever reads the error from run() should
                        // probably filter out INTERNAL errors as we don't really care about
                        // client-side errors.
                        let _ = self
                            .shared
                            .connection_event_sender
                            .send(ConnectionEvent::Closing {
                                send_goaway: None,
                                close_with: Some(Err(e)),
                            })
                            .await;
                    }

                    return;
                }

                // In all other cases, we enountered an uncategorized internal error.
                let _ = self
                    .shared
                    .connection_event_sender
                    .send(ConnectionEvent::Closing {
                        send_goaway: Some(ProtocolErrorV2 {
                            code: ErrorCode::INTERNAL_ERROR,
                            message: "Unknown internal error occured",
                            local: true,
                        }),
                        close_with: Some(Err(e)),
                    })
                    .await;
            }
        }
    }

    // TODO: According to RFC 7540 Section 4.1, undefined flags should be left as
    // zeros.

    // NOTE: Will return an Ok(()) if and only if the
    async fn run_inner(
        &self,
        mut reader: Box<dyn Readable>,
        seen_preface_head: bool,
    ) -> Result<()> {
        // Server endpoints need to read the preface sent by the client.
        if self.shared.is_server {
            let preface_str = if seen_preface_head {
                CONNECTION_PREFACE_BODY
            } else {
                CONNECTION_PREFACE
            };

            let mut preface = [0u8; CONNECTION_PREFACE.len()];
            reader
                .read_exact(&mut preface[0..preface_str.len()])
                .await?;
            if &preface[0..preface_str.len()] != preface_str {
                // We saw an invalid connection preface. Most likely this isn't an actual HTTP2
                // client, so it is safest to stop talking to them right away.
                let _ = self
                    .shared
                    .connection_event_sender
                    .send(ConnectionEvent::Closing {
                        send_goaway: None,
                        close_with: Some(Ok(())),
                    })
                    .await;
                return Ok(());
            }
        }

        // TODO: Receiving any packet on a stream with a smaller number than any stream
        // id ever seen should casue an error.

        // Used to decode remotely created headers received on the connection.
        // NOTE: This is shared across all streams on the connection.
        let mut remote_header_decoder;

        let mut max_frame_size;

        // Loading the above two variables from local settings.
        // NOTE: Because settings only change when they are acknowledged on this thread,
        // these are straight forward to keep in sync.
        {
            let state = self.shared.state.lock().await;
            remote_header_decoder =
                hpack::Decoder::new(state.local_settings[SettingId::HEADER_TABLE_SIZE] as usize);
            max_frame_size = state.local_settings[SettingId::MAX_FRAME_SIZE];
        }

        // Whether or not we've received a non-ACK SETTINGS frame from the other
        // endpoint yet. (we expect the first frame to the other endpoint to be
        // a non-ACK SETTINGS frame or an error).
        let mut received_remote_settings = false;

        // The current partially completed headers block if any. This will be set with
        // sequence of (HEADERS|PUSH_PROMISE) CONTINUATION* frames without
        // END_HEADERS set.
        //
        // When this is not None, we only allow CONTINUATION frames from the same stream
        // to be received and no other frames on other streams.
        let mut pending_header: Option<ReceivedHeaders> = None;

        let mut frame_header_buf = [0u8; FrameHeader::size_of()];
        loop {
            // Read the frame header
            reader.read_exact(&mut frame_header_buf).await?;

            let frame_header = FrameHeader::parse_complete(&frame_header_buf)?;

            // Enforce that the first frame is SETTINGS
            if !received_remote_settings
                && frame_header.typ != FrameType::SETTINGS
                && frame_header.typ != FrameType::GOAWAY
            {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::PROTOCOL_ERROR,
                    message: "Expected first received frame to be a SETTINGS frame",
                    local: true,
                }
                .into());
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
                    } else {
                        // is_local_stream_id
                        frame_header.stream_id > state.last_received_stream_id
                    }
                };

                if let Some(h) = &pending_header {
                    if h.stream_id == frame_header.stream_id {
                        idle = false;
                    }
                }

                if idle
                    && frame_header.typ != FrameType::HEADERS
                    && frame_header.typ != FrameType::PRIORITY
                {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "Received unallowed frame type for idle stream",
                        local: true,
                    }
                    .into());
                }
            }

            // Validate frame size based on procedure in RFC 7540: Section 4.2.
            if frame_header.length > max_frame_size {
                let can_alter_state = frame_header.typ == FrameType::SETTINGS
                    || frame_header.typ == FrameType::HEADERS
                    || frame_header.typ == FrameType::PUSH_PROMISE
                    || frame_header.typ == FrameType::CONTINUATION
                    || frame_header.stream_id == 0;

                if can_alter_state {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::FRAME_SIZE_ERROR,
                        message: "Received state altering frame larger than max frame size",
                        local: true,
                    }
                    .into());
                } else {
                    let mut connection_state = self.shared.state.lock().await;

                    self.shared
                        .finish_stream(
                            &mut connection_state,
                            frame_header.stream_id,
                            Some(ProtocolErrorV2 {
                                code: ErrorCode::FRAME_SIZE_ERROR,
                                message: "Received frame larger than max frame size",
                                local: true,
                            }),
                        )
                        .await;
                }

                // Skip over this frame's payload by just reading into a waste buffer until we
                // reach the end of the packet.
                let mut num_remaining = frame_header.length as usize;
                while num_remaining > 0 {
                    let mut buf = [0u8; 512];

                    let max_to_read = std::cmp::min(num_remaining, buf.len());
                    let n = reader.read(&mut buf[0..max_to_read]).await?;
                    num_remaining -= n;

                    if n == 0 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            format!("Hit end of stream before reading entire frame"),
                        )
                        .into());
                    }
                }

                continue;
            }

            // Enforce header block frames not interleaving with others.
            // This error is defined in RFC 7540: Section 6.10
            if let Some(received_header) = &pending_header {
                if frame_header.stream_id != received_header.stream_id
                    || frame_header.typ != FrameType::CONTINUATION
                {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "Non-continuation frames interleaved in header block",
                        local: true,
                    }
                    .into());
                }
            }

            // TODO: Read this on demand as we identify what it's needed for so that we can
            // just copy it into the final buffer all at once.
            let mut payload = vec![];
            // TODO: Should validate earlier that MAX_FRAME_SIZE is <= usize::max
            payload.resize(frame_header.length as usize, 0);
            reader.read_exact(&mut payload).await?;

            match frame_header.typ {
                FrameType::DATA => {
                    // TODO: If we receive DATA on a higher stream id, should we record it in
                    // last_received_stream_id to ensure that we can't receive a
                    // HEADERS later on that stream. ^ Basically ensure all
                    // RST_STREAM errors are converted into a permanent rejection of that stream id?

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
                    // Check if remotely closed. Even if closed, we still need it to count towards
                    // flow control (so we may want to tell the remote endpoint that more data is
                    // available)

                    let mut connection_state = self.shared.state.lock().await;
                    if connection_state.local_connection_window
                        < (frame_header.length as WindowSize)
                    {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FLOW_CONTROL_ERROR,
                            message: "Exceeded connection level window",
                            local: true,
                        }
                        .into());
                    }

                    // Update the local connection level window. This occurs even if the frame
                    // fails later down to ensure that it stays in sync with the remote endpoint.
                    connection_state.local_connection_window += frame_header.length as WindowSize;

                    let stream = match self
                        .find_received_stream(frame_header.stream_id, &mut connection_state)?
                    {
                        StreamEntry::Open(s) => s,
                        StreamEntry::New => {
                            // TODO: Normalize this error.
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::FLOW_CONTROL_ERROR,
                                message: "Recieved unallowed frame for idle stream",
                                local: true,
                            }
                            .into());
                        }
                        StreamEntry::Closed => {
                            // According to Section 6.1, we must send a STREAM_CLOSED if receiving
                            // a DATA frame on a non-"open" or "half-closed (local)" stream.
                            //
                            // This handles "closed"/"idle" cases. Other states will be checked in
                            // stream.receive_data().
                            //
                            // TODO: If we recently sent a RST_STREAM on this id since the last ping
                            // was acknowledged, don't both sending a RST_STREAM and just ignore the
                            // packets.
                            println!("SEND STREAM CLOSED");
                            let _ = self
                                .shared
                                .connection_event_sender
                                .send(ConnectionEvent::ResetStream {
                                    stream_id: frame_header.stream_id,
                                    error: ProtocolErrorV2 {
                                        code: ErrorCode::STREAM_CLOSED,
                                        message: "Received data on a closed stream",
                                        local: true,
                                    },
                                })
                                .await;

                            if frame_header.length != 0 {
                                let _ = self
                                    .shared
                                    .connection_event_sender
                                    .send(ConnectionEvent::StreamRead {
                                        stream_id: 0,
                                        count: (frame_header.length as usize),
                                    })
                                    .await;
                            }

                            continue;
                        }
                    };

                    let mut stream_state = stream.state.lock().await;

                    let extra_flow_controlled_bytes =
                        (frame_header.length as usize) - data_frame.data.len();

                    // TODO: Must refactor this and everything else to include padding in all the
                    // flow control calculations.
                    stream.receive_data(
                        &data_frame.data,
                        extra_flow_controlled_bytes,
                        data_flags.end_stream,
                        &mut stream_state,
                    );
                    // TODO: Have a smarter way to determine if the frame was rejected (e.g. based
                    // on the return value of receive_data).
                    if stream_state.error.is_some() || stream_state.reader_closed {
                        // Data frame was rejected.
                        // We can allow the other endpoint to send more.
                        if frame_header.length != 0 {
                            let _ = self
                                .shared
                                .connection_event_sender
                                .send(ConnectionEvent::StreamRead {
                                    stream_id: 0,
                                    count: (frame_header.length as usize),
                                })
                                .await;
                        }
                    } else {
                        // We discard the all the payload except the inner data, so we can given
                        // back flow control quota for any padding in the frame.
                        if extra_flow_controlled_bytes != 0 {
                            let _ = self
                                .shared
                                .connection_event_sender
                                .send(ConnectionEvent::StreamRead {
                                    stream_id: 0,
                                    count: extra_flow_controlled_bytes,
                                })
                                .await;
                        }
                    }

                    let stream_closed = stream.is_closed(&stream_state);
                    drop(stream_state);
                    drop(stream);

                    if stream_closed {
                        self.shared
                            .finish_stream(&mut connection_state, frame_header.stream_id, None)
                            .await;
                    }
                }
                FrameType::HEADERS => {
                    // NOTE: We don't check the stream_id until the full block is received in
                    // self.receive_headers().

                    let headers_flags = HeadersFrameFlags::parse_complete(&[frame_header.flags])?;
                    let headers_frame =
                        HeadersFramePayload::parse_complete(&payload, &headers_flags)?;
                    frame_utils::check_padding(&headers_frame.padding)?;

                    let received_headers = ReceivedHeaders {
                        data: headers_frame.header_block_fragment,
                        stream_id: frame_header.stream_id,
                        typ: ReceivedHeadersType::RegularHeaders {
                            end_stream: headers_flags.end_stream,
                            priority: headers_frame.priority,
                        },
                    };

                    if headers_flags.end_headers {
                        self.receive_headers(received_headers, &mut remote_header_decoder)
                            .await?;
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::PUSH_PROMISE => {
                    let push_promise_flags =
                        PushPromiseFrameFlags::parse_complete(&[frame_header.flags])?;
                    let push_promise_frame =
                        PushPromiseFramePayload::parse_complete(&payload, &push_promise_flags)?;
                    frame_utils::check_padding(&push_promise_frame.padding)?;

                    let received_headers = ReceivedHeaders {
                        data: push_promise_frame.header_block_fragment,
                        stream_id: frame_header.stream_id,
                        typ: ReceivedHeadersType::PushPromise {
                            promised_stream_id: push_promise_frame.promised_stream_id,
                        },
                    };

                    if push_promise_flags.end_headers {
                        self.receive_headers(received_headers, &mut remote_header_decoder)
                            .await?;
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::PRIORITY => {
                    if frame_header.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received PRIORITY frame on connection control stream",
                            local: true,
                        }
                        .into());
                    }

                    if (frame_header.length as usize) != PriorityFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received PRIORITY frame of wrong length",
                            local: true,
                        }
                        .into());
                    }

                    let priority_frame = PriorityFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;
                    self.set_priority(
                        frame_header.stream_id,
                        &priority_frame,
                        &mut connection_state,
                    )
                    .await?;
                }
                FrameType::RST_STREAM => {
                    if frame_header.stream_id == 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received RST_STREAM frame on connection control stream",
                            local: true,
                        }
                        .into());
                    }

                    if (frame_header.length as usize) != RstStreamFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received RST_STREAM frame of wrong length",
                            local: true,
                        }
                        .into());
                    }

                    let rst_stream_frame = RstStreamFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;

                    if (self.shared.is_local_stream_id(frame_header.stream_id)
                        && frame_header.stream_id > connection_state.last_sent_stream_id)
                        || (self.shared.is_remote_stream_id(frame_header.stream_id)
                            && frame_header.stream_id > connection_state.last_received_stream_id)
                    {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received RST_STREAM for idle stream",
                            local: true,
                        }
                        .into());
                    }

                    self.shared
                        .finish_stream(
                            &mut connection_state,
                            frame_header.stream_id,
                            Some(ProtocolErrorV2 {
                                code: rst_stream_frame.error_code,
                                message: "Recieved RST_STREAM from remote endpoint",
                                local: false,
                            }),
                        )
                        .await;
                }
                FrameType::SETTINGS => {
                    if frame_header.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received SETTINGS frame on non-connection control stream",
                            local: true,
                        }
                        .into());
                    }

                    if (frame_header.length % 6) != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received SETTINGS frame of wrong length",
                            local: true,
                        }
                        .into());
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
                                local: true,
                            }
                            .into());
                        }

                        if let Some(waiter) = connection_state.local_settings_ack_waiter.take() {
                            // Stop waiting
                            drop(waiter);
                        } else {
                            return Err(ProtocolErrorV2 {
                                code: ErrorCode::PROTOCOL_ERROR,
                                message:
                                    "Received SETTINGS ACK while no settings where pending ACK",
                                local: true,
                            }
                            .into());
                        }

                        // TODO: Apply any other state changes that are needed.
                        connection_state.local_settings =
                            connection_state.local_pending_settings.clone();

                        // TODO: Group together all of these variables which need to be synced to
                        // the settings.
                        remote_header_decoder.set_protocol_max_size(
                            connection_state.local_settings[SettingId::HEADER_TABLE_SIZE] as usize,
                        );
                        max_frame_size = connection_state.local_settings[SettingId::MAX_FRAME_SIZE];

                        // TODO: Adjust flow control window.
                    } else {
                        received_remote_settings = true;

                        let mut header_table_size = None;

                        // Apply the settings.
                        for param in settings_frame.parameters {
                            let old_value = connection_state
                                .remote_settings
                                .set(param.id, param.value)?;

                            if param.id == SettingId::HEADER_TABLE_SIZE {
                                // NOTE: This will be applied on the writer thread as it needs to
                                // ACK'ed in lock step
                                // with any usages of the header encoder.
                                header_table_size = Some(param.value);
                            } else if param.id == SettingId::INITIAL_WINDOW_SIZE {
                                // NOTE: Changes to this parameter DO NOT update the connection
                                // window.

                                // NOTE: As we validate that this parameter is always < 2^32,
                                // this should never overflow.
                                let window_diff = (param.value as WindowSize)
                                    - (old_value.unwrap_or(0) as WindowSize);

                                for (stream_id, stream) in &connection_state.streams {
                                    if stream.sending_end_flushed {
                                        continue;
                                    }

                                    let mut stream_state = stream.state.lock().await;

                                    stream_state.remote_window = stream_state.remote_window.checked_add(
                                        window_diff).ok_or_else(|| Error::from(ProtocolErrorV2 {
                                            code: ErrorCode::FLOW_CONTROL_ERROR,
                                            message: "Change in INITIAL_WINDOW_SIZE cause a window to overflow",
                                            local: true
                                        }))?;

                                    // The window size change may have make it possible for more
                                    // data to be send.
                                    let _ = stream.write_available_notifier.try_send(());
                                }

                                // Force a re-check of whether or not more data can be sent.
                                // TODO: Prevent updating this parameter many times in the same
                                // receive settings block.
                                let _ = self
                                    .shared
                                    .connection_event_sender
                                    .send(ConnectionEvent::StreamWrite { stream_id: 0 })
                                    .await;
                            }
                        }

                        let _ = self
                            .shared
                            .connection_event_sender
                            .send(ConnectionEvent::AcknowledgeSettings { header_table_size })
                            .await;
                    }
                }
                FrameType::PING => {
                    if frame_header.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received PING message on non-connection control stream",
                            local: true,
                        }
                        .into());
                    }

                    if (frame_header.length as usize) != PingFramePayload::size_of() {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received PING message of wrong length",
                            local: true,
                        }
                        .into());
                    }

                    let ping_flags = PingFrameFlags::parse_complete(&[frame_header.flags])?;
                    let ping_frame = PingFramePayload::parse_complete(&payload)?;

                    if ping_flags.ack {
                        // TODO
                    } else {
                        // TODO: Block if too many pings in a short period of time.
                        self.shared
                            .connection_event_sender
                            .send(ConnectionEvent::Ping { ping_frame })
                            .await
                            .map_err(|_| err_msg("Writer thread closed"))?;
                    }
                }
                // TOOD: Return an error if we ever receive multiple GOAWAY packets with the later
                // one having a larger last_stream_id than a previous one.
                FrameType::GOAWAY => {
                    if frame_header.stream_id != 0 {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received GOAWAY frame on non-connection control stream",
                            local: true,
                        }
                        .into());
                    }

                    // TODO: When a server gracefully shuts down, follow the guidance in Section 6.8

                    /*
                    TODO: Is this a mandatory part of every implementation:
                    After sending a GOAWAY frame, the sender can discard frames for
                    streams initiated by the receiver with identifiers higher than the
                    identified last stream.
                    */

                    let goaway_frame = GoawayFramePayload::parse_complete(&payload)?;

                    if !self.shared.is_local_stream_id(goaway_frame.last_stream_id) {
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "GOAWAY received with id of non-local stream",
                            local: true,
                        }
                        .into());
                    }

                    // TODO: Limit the size of the opaque data

                    let mut connection_state = self.shared.state.lock().await;

                    // TODO: We need to be much more consistent about always setting this.
                    // TODO: We need to uphold the gurantee that while this is None, an incoming
                    // request is guranteed to be processed.
                    // TODO: Verify that once this is set, we won't generate any new streams.

                    // TODO: Let's now only do this if we have a non-NO_ERROR frame.
                    // connection_state.error = Some(ProtocolErrorV2 {
                    //     code: goaway_frame.error_code,
                    //     message: "Remote GOAWAY received", // TODO: Read the opaque message from
                    // the remote packet.     local: true
                    // });

                    // TODO: Check that it is monotonically lower than before.
                    connection_state.upper_sent_stream_id = goaway_frame.last_stream_id;

                    let refused_error = ProtocolErrorV2 {
                        code: ErrorCode::REFUSED_STREAM,
                        message: "Connection shutting down",
                        local: false,
                    };

                    let mut stream_ids_to_finish = vec![];
                    for (stream_id, stream) in &connection_state.streams {
                        // TODO: Only applies to locally initialized streams?
                        if self.shared.is_local_stream_id(*stream_id)
                            && *stream_id > goaway_frame.last_stream_id
                        {
                            // Reset the stream with a 'retryable' error.
                            // Main challenge is deadling with locks.
                            stream_ids_to_finish.push(*stream_id);
                        }
                    }

                    for stream_id in stream_ids_to_finish {
                        self.shared
                            .finish_stream(
                                &mut connection_state,
                                stream_id,
                                Some(refused_error.clone()),
                            )
                            .await;
                    }

                    // This code is only relevant on the client side.
                    // As soon as we receive a remote GOAWAY, we can cancel all unsent requests.
                    while let Some(req) = connection_state.pending_requests.pop_front() {
                        req.response_handler
                            .handle_response(Err(refused_error.clone().into()))
                            .await;
                    }

                    if goaway_frame.error_code == ErrorCode::NO_ERROR {
                        // Graceful shutdown.
                        // TODO: Check this implementation.

                        if !connection_state.shutting_down.is_some() {
                            connection_state
                                .set_shutting_down(ShuttingDownState::GracefulRemote)
                                .await;
                        }

                        // Check with the writer thread if it wants to stop the connection now.
                        let _ = self
                            .shared
                            .connection_event_sender
                            .send(ConnectionEvent::Closing {
                                send_goaway: None,
                                close_with: None,
                            })
                            .await;

                        // TODO: Set connection_state.shuttind_down and maybe
                        // send a CLOSED notification as
                        // we may want to stop now.

                        // TODO: Even if we receive an error, we can still
                        // perform this right?
                    } else {
                        // Need to reset all the streams!

                        // Need to return an error but shouldn't ask the writer thread to repeat it.

                        // Via the CLOSING, we should propagate the error.

                        connection_state
                            .set_shutting_down(ShuttingDownState::Complete)
                            .await;
                        let _ = self
                            .shared
                            .connection_event_sender
                            .send(ConnectionEvent::Closing {
                                send_goaway: None,
                                close_with: Some(Err(ProtocolErrorV2 {
                                    code: goaway_frame.error_code,
                                    message: "Remote GOAWAY received",
                                    // TODO: Support returning the raw message
                                    // message: Self::debug_data_to_string(&goaway_frame.
                                    // additional_debug_data),
                                    local: false,
                                }
                                .into())),
                            })
                            .await;
                        return Ok(());
                    }
                }
                FrameType::WINDOW_UPDATE => {
                    if (frame_header.length as usize) != WindowUpdateFramePayload::size_of() {
                        // Connection error: FRAME_SIZE_ERROR
                        return Err(ProtocolErrorV2 {
                            code: ErrorCode::FRAME_SIZE_ERROR,
                            message: "Received WINDOW_UPDATE message of wrong length",
                            local: true,
                        }
                        .into());
                    }

                    // TODO: Should we block these if received on an idle stream.

                    let window_update_frame = WindowUpdateFramePayload::parse_complete(&payload)?;

                    let mut connection_state = self.shared.state.lock().await;

                    if window_update_frame.window_size_increment == 0 {
                        let error = ProtocolErrorV2 {
                            code: ErrorCode::PROTOCOL_ERROR,
                            message: "Received WINDOW_UPDATE with invalid 0 increment",
                            local: true,
                        };

                        if frame_header.stream_id == 0 {
                            return Err(error.into());
                        }

                        // TODO: Send the RST_STREAM even if the stream is unknown?

                        self.shared
                            .finish_stream(
                                &mut connection_state,
                                frame_header.stream_id,
                                Some(error),
                            )
                            .await;

                        continue;
                    }

                    if frame_header.stream_id == 0 {
                        connection_state.remote_connection_window = connection_state
                            .remote_connection_window
                            .checked_add(window_update_frame.window_size_increment as WindowSize)
                            .ok_or_else(|| ProtocolErrorV2 {
                                code: ErrorCode::FLOW_CONTROL_ERROR,
                                message: "Overflow in connection flow control window size",
                                local: true,
                            })?;
                    } else if let Some(stream) =
                        connection_state.streams.get(&frame_header.stream_id)
                    {
                        let mut stream_state = stream.state.lock().await;

                        // TODO: Make this just a stream error?
                        match stream_state
                            .remote_window
                            .checked_add(window_update_frame.window_size_increment as WindowSize)
                        {
                            Some(v) => {
                                stream.set_remote_window(&mut stream_state, v);
                            }
                            None => {
                                stream_state.error = Some(ProtocolErrorV2 {
                                    code: ErrorCode::FLOW_CONTROL_ERROR,
                                    message: "Overflow in stream flow control window size",
                                    local: true,
                                });

                                drop(stream_state);
                                self.shared
                                    .finish_stream(
                                        &mut connection_state,
                                        frame_header.stream_id,
                                        None,
                                    )
                                    .await;
                            }
                        };
                    }

                    // Check again if there is any data available for writing now that the window
                    // has been updated.
                    let _ = self
                        .shared
                        .connection_event_sender
                        .send(ConnectionEvent::StreamWrite {
                            stream_id: frame_header.stream_id,
                        })
                        .await;
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

                    let continuation_flags =
                        ContinuationFrameFlags::parse_complete(&[frame_header.flags])?;

                    // NOTE: The entire payload is a header chunk.
                    // TODO: Enforce a max size to the combined header data.
                    received_headers.data.extend_from_slice(&payload);

                    if continuation_flags.end_headers {
                        self.receive_headers(received_headers, &mut remote_header_decoder)
                            .await?;
                    } else {
                        pending_header = Some(received_headers);
                    }
                }
                FrameType::Unknown(_) => {
                    // According to RFC 7540 Section 4.1, unknown frame types
                    // should be discarded.
                }
            }
        }
    }

    fn debug_data_to_string(data: &[u8]) -> String {
        let mut s = String::new();
        for b in data {
            if *b != 0 && b.is_ascii() {
                s.push(*b as char);
            } else {
                write!(s, "\\x{:02x}", b).unwrap(); // Should never fail.
            }
        }

        s
    }

    async fn receive_headers(
        &self,
        received_headers: ReceivedHeaders,
        remote_header_decoder: &mut hpack::Decoder,
    ) -> Result<()> {
        // First deserialize all the headers so that they definately get applied to the
        // production state. TODO: Preserve the original error message and log
        // internally?
        let headers = remote_header_decoder
            .parse_all(&received_headers.data)
            .map_err(|_| ProtocolErrorV2 {
                code: ErrorCode::COMPRESSION_ERROR,
                message: "Failure while decoding receivers header fragment",
                local: true,
            })?;

        let mut connection_state = self.shared.state.lock().await;

        match received_headers.typ {
            ReceivedHeadersType::RegularHeaders {
                end_stream,
                priority,
            } => {
                // TODO: Need to implement usage of 'priority'

                if self.shared.is_server {
                    self.receive_headers_server(
                        received_headers.stream_id,
                        headers,
                        end_stream,
                        &mut connection_state,
                    )
                    .await?;
                } else {
                    self.receive_headers_client(
                        received_headers.stream_id,
                        headers,
                        end_stream,
                        &mut connection_state,
                    )
                    .await?;
                    // TODO: Maybe update priority
                }

                // TODO: Only do this if the headers methods succeeded?
                if let Some(priority) = priority {
                    self.set_priority(received_headers.stream_id, &priority, &mut connection_state)
                        .await?;
                }
            }
            ReceivedHeadersType::PushPromise { promised_stream_id } => {
                if self.shared.is_server {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "Client should not be sending PUSH_PROMISE frames",
                        local: true,
                    }
                    .into());
                }

                // TODO: Reserved streams don't count towards the MAX_CONCURRNET_STREAM limit.

                // TODO: Implement prioritization using the correct parent stream.

                return Err(err_msg("Push promise not yet implemented"));
            }
        }

        Ok(())
    }

    /// Implementation on receiving a block of headers on a server from a
    /// client.
    async fn receive_headers_server(
        &self,
        stream_id: StreamId,
        headers: Vec<hpack::HeaderField>,
        end_stream: bool,
        connection_state: &mut ConnectionState,
    ) -> Result<()> {
        match self.find_received_stream(stream_id, connection_state)? {
            // Client sent new request headers.
            StreamEntry::New => {
                if stream_id > connection_state.upper_received_stream_id {
                    // When shutting down, don't accept any new streams.
                    // TODO: Actually we should check if the stream_id is > the last_stream_id in
                    // our GOAWAY message.
                    let _ = self
                        .shared
                        .connection_event_sender
                        .send(ConnectionEvent::ResetStream {
                            stream_id: stream_id,
                            error: ProtocolErrorV2 {
                                code: ErrorCode::REFUSED_STREAM,
                                message: "Received new request after shutdown",
                                local: true,
                            },
                        })
                        .await;
                    return Ok(());
                }

                if connection_state.remote_stream_count
                    >= connection_state.local_settings[SettingId::MAX_CONCURRENT_STREAMS] as usize
                {
                    // Send a REFUSED_STREAM stream error (or as described in 5.1.2, PROTOCOL_ERROR
                    // is also allowed if we don't want it to be retryable)..
                    // TODO: Should we disallow using this stream id in the future?

                    let _ = self
                        .shared
                        .connection_event_sender
                        .send(ConnectionEvent::ResetStream {
                            stream_id: stream_id,
                            error: ProtocolErrorV2 {
                                code: ErrorCode::REFUSED_STREAM,
                                message: "Exceeded MAX_CONCURRENT_STREAMS",
                                local: true,
                            },
                        })
                        .await;
                    return Ok(());
                }

                // When receiving headers on server,
                // - end stream means the remote stream is done.

                // TODO: Ensure that all assignments to this are guarded by the check on
                // upper_received_stream_id. TODO: Only do this if the stream is
                // successfully started?
                connection_state.last_received_stream_id = stream_id;
                connection_state.remote_stream_count += 1;

                // Make new a new stream

                let (mut stream, incoming_body, outgoing_body) =
                    self.shared.new_stream(&connection_state, stream_id);

                let mut stream_state = stream.state.lock().await;

                if let Some(request) =
                    stream.receive_request(headers, end_stream, incoming_body, &mut stream_state)
                {
                    stream.outgoing_response_handler = Some((request.head.method, outgoing_body));

                    stream.processing_tasks.push(ChildTask::spawn(
                        self.shared
                            .clone()
                            .request_handler_driver(stream_id, request),
                    ));
                }

                let stream_closed = stream.is_closed(&stream_state);
                drop(stream_state);

                connection_state.streams.insert(stream_id, stream);

                if stream_closed {
                    self.shared
                        .finish_stream(connection_state, stream_id, None)
                        .await;
                }
            }
            // Receiving the client's trailers for an existing request.
            StreamEntry::Open(stream) => {
                let mut stream_state = stream.state.lock().await;

                stream.receive_trailers(headers, end_stream, &mut stream_state);

                if stream.is_closed(&stream_state) {
                    drop(stream_state);
                    self.shared
                        .finish_stream(connection_state, stream_id, None)
                        .await;
                }
            }
            StreamEntry::Closed => {
                let _ = self
                    .shared
                    .connection_event_sender
                    .send(ConnectionEvent::ResetStream {
                        stream_id: stream_id,
                        error: ProtocolErrorV2 {
                            code: ErrorCode::STREAM_CLOSED,
                            message: "Received HEADERS on closed stream",
                            local: true,
                        },
                    })
                    .await;
            }
        }

        Ok(())
    }

    async fn receive_headers_client(
        &self,
        stream_id: StreamId,
        headers: Vec<hpack::HeaderField>,
        end_stream: bool,
        connection_state: &mut ConnectionState,
    ) -> Result<()> {
        match self.find_received_stream(stream_id, connection_state)? {
            StreamEntry::New => {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::PROTOCOL_ERROR,
                    message: "Client received HEADERS on remote idle stream without PUSH_PROMISE",
                    local: true,
                }
                .into());
            }
            // Either receiving a response head or trailers from the server.
            StreamEntry::Open(stream) => {
                let mut stream_state = stream.state.lock().await;

                if let Some((request_method, response_handler, incoming_body)) =
                    stream.incoming_response_handler.take()
                {
                    if let Some(response) = stream.receive_response(
                        request_method,
                        headers,
                        end_stream,
                        incoming_body,
                        &mut stream_state,
                    ) {
                        response_handler.handle_response(Ok(response)).await;
                    } else {
                        // TODO: Use the stream error.
                        response_handler
                            .handle_response(Err(err_msg("Failed")))
                            .await;
                    }
                } else {
                    // Otherwise we just received trailers.
                    stream.receive_trailers(headers, end_stream, &mut *stream_state);
                }

                let stream_closed = stream.is_closed(&stream_state);
                drop(stream_state);

                if stream_closed {
                    self.shared
                        .finish_stream(connection_state, stream_id, None)
                        .await;
                }
            }
            StreamEntry::Closed => {
                let _ = self
                    .shared
                    .connection_event_sender
                    .send(ConnectionEvent::ResetStream {
                        stream_id: stream_id,
                        error: ProtocolErrorV2 {
                            code: ErrorCode::STREAM_CLOSED,
                            message: "Received HEADERS on closed stream",
                            local: true,
                        },
                    })
                    .await;
            }
        }

        Ok(())
    }

    /// Find the stream associated with a stream_id that we received from the
    /// other endpoint.
    ///
    /// This performs a few validation checks:
    /// 1. Idle streams with a local stream id are invalid.
    /// 2. If the stream_id == 0, we will error out.
    fn find_received_stream<'a>(
        &self,
        stream_id: StreamId,
        connection_state: &'a mut ConnectionState,
    ) -> Result<StreamEntry<'a>> {
        if stream_id == 0 {
            return Err(ProtocolErrorV2 {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Unexpected frame on connection control stream",
                local: true,
            }
            .into());
        }

        if let Some(stream) = connection_state.streams.get_mut(&stream_id) {
            return Ok(StreamEntry::Open(stream));
        }

        if self.shared.is_local_stream_id(stream_id) {
            if stream_id > connection_state.last_sent_stream_id {
                return Err(ProtocolErrorV2 {
                    code: ErrorCode::PROTOCOL_ERROR,
                    message: "Received frame on idle stream with local stream id",
                    local: true,
                }
                .into());
            }

            Ok(StreamEntry::Closed)
        } else {
            // is_remote_stream_id
            if stream_id > connection_state.last_received_stream_id {
                return Ok(StreamEntry::New);
            }

            Ok(StreamEntry::Closed)
        }
    }

    /// NOTE: This assumes that we've verified that we are not usign stream_id 0
    async fn set_priority(
        &self,
        stream_id: StreamId,
        frame: &PriorityFramePayload,
        connection_state: &mut ConnectionState,
    ) -> Result<()> {
        // NOTE: See https://github.com/httpwg/http2-spec/pull/813. PRIORITY frames
        // can't implicitly close lower numbered streams.

        // TODO: This is problematic. IF
        if stream_id == frame.stream_dependency {
            // Stream Error PROTOCOL_ERROR
            self.shared
                .finish_stream(
                    connection_state,
                    stream_id,
                    Some(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "Stream can't depend on itself",
                        local: true,
                    }),
                )
                .await;
        }

        // TODO: Implement prioritization logic here.

        Ok(())
    }
}

enum StreamEntry<'a> {
    /// The looked up stream is an idle stream with a remote stream id.
    New,

    /// The looked up stream is known to have been initialized before and is
    /// still being read/written.
    Open(&'a mut Stream),

    /// The looked up stream is closed.
    Closed,
}
