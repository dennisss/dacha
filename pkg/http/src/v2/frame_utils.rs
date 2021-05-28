// Helpers for creating protocol frames.

use common::errors::*;

use crate::v2::types::*;
use crate::proto::v2::*;

pub fn new_window_update_frame(stream_id: StreamId, increment: usize) -> Vec<u8> {
    let mut frame = vec![];
    FrameHeader {
        typ: FrameType::WINDOW_UPDATE,
        length: WindowUpdateFramePayload::size_of() as u32,
        flags: 0,
        reserved: 0,
        stream_id
    }.serialize(&mut frame).unwrap();

    WindowUpdateFramePayload {
        reserved: 0,
        window_size_increment: increment as u32,
    }.serialize(&mut frame).unwrap();

    frame
}

pub fn new_data_frame(stream_id: StreamId, data: Vec<u8>, end_stream: bool) -> Vec<u8> {
    let mut frame = vec![];
    FrameHeader {
        typ: FrameType::DATA,
        flags: DataFrameFlags {
            padded: false,
            end_stream,
            reserved1: 0,
            reserved2: 0
        }.to_u8().unwrap(),
        length: data.len() as u32,
        reserved: 0,
        stream_id
    }.serialize(&mut frame).unwrap();

    frame.extend_from_slice(&data);

    frame
}

pub fn new_ping_frame(opaque_data: [u8; 8], ack: bool) -> Vec<u8> {
    let mut frame = vec![];
    FrameHeader {
        typ: FrameType::PING,
        length: PingFramePayload::size_of() as u32,
        flags: PingFrameFlags {
            ack,
            reserved1234567: 0,
        }.to_u8().unwrap(),
        reserved: 0,
        stream_id: 0
    }.serialize(&mut frame).unwrap();

    PingFramePayload {
        opaque_data
    }.serialize(&mut frame).unwrap();

    frame
}

pub fn new_settings_ack_frame() -> Vec<u8> {
    let mut frame = vec![];
    FrameHeader {
        typ: FrameType::SETTINGS,
        length: 0,
        flags: SettingsFrameFlags {
            ack: true,
            reserved1234567: 0
        }.to_u8().unwrap(),
        reserved: 0,
        stream_id: 0
    }.serialize(&mut frame).unwrap();

    frame
}

pub fn new_rst_stream_frame(stream_id: StreamId, error: ProtocolErrorV2) -> Vec<u8> {
    let mut frame = vec![];
    FrameHeader {
        typ: FrameType::RST_STREAM,
        length: RstStreamFramePayload::size_of() as u32,
        flags: 0,
        reserved: 0,
        stream_id
    }.serialize(&mut frame).unwrap();

    RstStreamFramePayload {
        error_code: error.code
    }.serialize(&mut frame).unwrap();

    frame
}

pub fn new_goaway_frame(last_stream_id: StreamId, error: ProtocolErrorV2) -> Vec<u8> {
    let mut payload = vec![];
    GoawayFramePayload {
        reserved: 0,
        last_stream_id,
        error_code: error.code,
        additional_debug_data: error.message.as_bytes().to_vec()
    }.serialize(&mut payload).unwrap();

    let mut frame = vec![];
    FrameHeader {
        typ: FrameType::GOAWAY,
        length: payload.len() as u32,
        flags: 0,
        reserved: 0,
        stream_id: 0
    }.serialize(&mut frame).unwrap();

    frame.extend_from_slice(&payload);

    frame
}

pub fn check_padding(padding: &[u8]) -> Result<()> {
    for byte in padding {
        if *byte != 0 {
            return Err(ProtocolErrorV2 {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Received non-zero padding in DATA frame",
                local: true
            }.into());
        }
    }

    Ok(())
}
