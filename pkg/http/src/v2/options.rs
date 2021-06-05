use std::time::Duration;

use crate::proto::v2::*;
use crate::v2::settings::SettingsContainer;

#[derive(Clone)]
pub struct ConnectionOptions {
    /// HTTP2 protocol defined settings. These will be communicated to the remote
    /// endpoint.
    ///
    /// INTERNAL WARNING: In the ConnectionShared code, these values shouldn't be accessed
    /// directly. Instead only the separate 'local_settings' field which contains the last
    /// acknowledged value of this should be used. 
    pub protocol_settings: SettingsContainer,

    /// Maximum number of bytes per stream that we will allow to be enqueued for sending to the
    /// remote server.
    ///
    /// The actual max used will be the min of this value of the remote flow control window
    /// size. We maintain this as a separate setting to ensure that a misbehaving remote endpoint
    /// can't force us to use large amounts of memory while queuing data. 
    pub max_sending_buffer_size: usize,

    /// Maximum size of the dynamic header index in bytes used to encode headers that are sent out.
    /// The actual size of the table will be regulated using HPACK dynamic size updates to be
    /// min(max_local_encoder_table_size, remote_settings[])
    pub max_local_encoder_table_size: usize,

    /// Amount of time after which we'll close the connection if we don't
    /// receive an acknowledment to our
    pub settings_ack_timeout: Duration,

    // TODO: Limit maximum number of incoming and outgoing pushes

    // TODO: Limit the number of streams stored in the priority tree.

    /// Maximum number of locally initialized streams
    /// The actual number used will be:
    /// 'min(max_outgoing_stream, remote_settings.MAX_CONCURRENT_STREAMS)'
    pub max_outgoing_streams: usize
}

impl std::default::Default for ConnectionOptions {
    fn default() -> Self {
        // Using the default values, except adding reasonable values for the
        // initially infinite values.
        let mut protocol_settings = SettingsContainer::default();
        protocol_settings.set(SettingId::MAX_CONCURRENT_STREAMS, 100).unwrap();
        protocol_settings.set(SettingId::MAX_HEADER_LIST_SIZE, 256*1024).unwrap(); // 256KB

        ConnectionOptions {
            protocol_settings,
            max_sending_buffer_size: 64*1024,  // 64 KB
            max_local_encoder_table_size: 8192,
            settings_ack_timeout: Duration::from_secs(10),
            max_outgoing_streams: 100
        }
    }
}