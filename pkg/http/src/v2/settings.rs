
/*
    Codes between 0 and 0xEFFF are reviewed while codes between 0xF000 to 0xFFFF are for experimental use.
*/

use std::{collections::HashMap, ops::Index};

use crate::proto::v2::*;

const INFINITE: u32 = u32::MAX;

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

            SettingsParameter { id: *id, value: *value }.serialize(out);
        }
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