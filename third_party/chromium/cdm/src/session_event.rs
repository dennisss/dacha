use crate::bindings::KeyStatus;
use crate::MessageType;

#[derive(Clone, Debug)]
pub enum SessionEvent {
    SessionMessage {
        session_id: String,
        message_type: MessageType,
        message: Vec<u8>,
    },

    /// The time at which keys in the session will expire has changed.
    ExpirationChange {
        session_id: String,

        /// Time in seconds since epoch at which the expiration happens.
        /// (0 means there is no expiration).
        new_expiry_time: f64,
    },

    SessionKeysChange {
        session_id: String,
        has_additional_usable_key: bool,
        keys: Vec<KeyInfo>,
    },

    SessionClosed {
        session_id: String,
    },
}

#[derive(Clone, Debug)]
pub struct KeyInfo {
    pub key_id: Vec<u8>,
    pub status: KeyStatus,
    pub system_code: u32,
}
