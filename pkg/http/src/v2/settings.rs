
/*
    Codes between 0 and 0xEFFF are reviewed while codes between 0xF000 to 0xFFFF are for experimental use.
*/

use std::{collections::HashMap, ops::Index};

use common::errors::*;
use parsing::ascii::AsciiString;

use crate::{headers::connection::parse_connection, proto::v2::*};
use crate::header::{Headers, Header};
use crate::headers::connection::ConnectionOption;
use crate::v2::types::*;
use crate::status_code::*;

const INFINITE: u32 = u32::MAX;

const SETTINGS_HEADER: &'static str = "HTTP2-Settings";

const MIN_ALLOWED_FRAME_SIZE: u32 = 1 << 14;
const MAX_ALLOWED_FRAME_SIZE: u32 = (1 << 24) - 1; 

/// Container of HTTP2 settings.
#[derive(Clone)]
pub struct SettingsContainer {
    data: HashMap<SettingId, u32>
}

impl SettingsContainer {

    /// Serializes the settings in this container into the payload of a SETTINGS frame.
    ///
    /// Only values which differ from the last_settings will be included.
    /// NOTE: We assume that the set of keys in each is the same.
    pub fn serialize_payload(&self, last_settings: &SettingsContainer, out: &mut Vec<u8>) {
        // Making a SettingsFramePayload
        for (id, value) in self.data.iter() {
            if let Some(old_value) = last_settings.data.get(&id) {
                if old_value == value {
                    continue;
                }
            }

            SettingsParameter { id: *id, value: *value }.serialize(out).unwrap();
        }
    }

    pub fn append_to_request(&self, headers: &mut Headers, connection_options: &mut Vec<ConnectionOption>) {
        let mut payload = vec![];
        self.serialize_payload(&Self::default(), &mut payload);

        let value = common::base64::encode_config(&payload, common::base64::URL_SAFE_NO_PAD);

        headers.raw_headers.push(Header {
            name: AsciiString::from(SETTINGS_HEADER).unwrap(),
            value: value.into()
        });

        connection_options.push(ConnectionOption::Unknown(AsciiString::from(SETTINGS_HEADER).unwrap()));
    }

    // TODO: Only return ProtocolErrorV1's.
    pub fn read_from_request(headers: &Headers) -> Result<Self> {
        let header = headers.find_one(SETTINGS_HEADER)
            .map_err(|_| Error::from(ProtocolErrorV1 {
                code: BAD_REQUEST, message: "Expected exactly one HTTP2-Settings header" }))?;

        let connection_options = parse_connection(headers)?;
        let mut found_option = false;
        for option in connection_options {
            if option == &SETTINGS_HEADER {
                found_option = true;
                break;
            }
        }

        if !found_option {
            return Err(ProtocolErrorV1 {
                code: BAD_REQUEST,
                message: "HTTP2-Settings not present in Connection options"
            }.into());
        }

        let payload = common::base64::decode_config(
            header.value.as_bytes(), common::base64::URL_SAFE_NO_PAD)
            .map_err(|_| Error::from(ProtocolErrorV1 {
                code: BAD_REQUEST,
                message: "HTTP2-Settings header can't be parsed as url safe base64"
            }))?;
        
        let frame = SettingsFramePayload::parse_complete(&payload)?;

        let mut out = Self::default();
        for param in frame.parameters {
            out.set(param.id, param.value)?;
        }

        Ok(out)
    }

    /// NOTE: This will validate that the setting is in the allowed range of values.
    /// Unknown settings will be ignored.
    ///
    /// TODO: Also check against usize for values which are sensitive 
    ///
    /// Returns the old value of the setting if any.
    pub fn set(&mut self, id: SettingId, value: u32) -> Result<Option<u32>> {
        match id {
            SettingId::HEADER_TABLE_SIZE => {},
            SettingId::ENABLE_PUSH => {
                if value != 0 && value != 1 {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "ENABLE_PUSH setting can only be 0 or 1",
                        local: true
                    }.into());
                }
            },
            SettingId::MAX_CONCURRENT_STREAMS => {},
            SettingId::INITIAL_WINDOW_SIZE => {
                if value > (WindowSize::MAX as u32) {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::FLOW_CONTROL_ERROR,
                        message: "INITIAL_WINDOW_SIZE value too large",
                        local: true
                    }.into());
                }
            }
            SettingId::MAX_FRAME_SIZE => {
                if value < MIN_ALLOWED_FRAME_SIZE || value > MAX_ALLOWED_FRAME_SIZE {
                    return Err(ProtocolErrorV2 {
                        code: ErrorCode::PROTOCOL_ERROR,
                        message: "MAX_FRAME_SIZE outside of allowed range",
                        local: true
                    }.into());
                }
            }
            SettingId::MAX_HEADER_LIST_SIZE => {},
            SettingId::Unknown(_) => {
                // Ignore
                return Ok(None);
            }
        }

        Ok(self.data.insert(id, value))
    }
}

impl Index<SettingId> for SettingsContainer {
    type Output = u32;

    fn index(&self, id: SettingId) -> &Self::Output {
        &self.data[&id]
    }
}

impl Default for SettingsContainer {
    fn default() -> Self {
        // Default values based on RFC 7540: Section 11.3
        let mut data = HashMap::new();
        data.insert(SettingId::HEADER_TABLE_SIZE, 4096);
        data.insert(SettingId::ENABLE_PUSH, 1);
        data.insert(SettingId::MAX_CONCURRENT_STREAMS, INFINITE);
        data.insert(SettingId::INITIAL_WINDOW_SIZE, 65535);
        data.insert(SettingId::MAX_FRAME_SIZE, 16384);
        data.insert(SettingId::MAX_HEADER_LIST_SIZE, INFINITE);

        Self { data }
    }
}