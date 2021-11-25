#[macro_use]
mod macros;
pub mod alert;
pub mod application_stream;
mod cipher;
pub mod client;
mod constants;
pub mod extensions;
mod extensions_util;
pub mod handshake;
pub mod handshake_summary;
pub mod key_schedule;
mod key_schedule_helper;
pub mod options;
mod parsing;
pub mod record;
mod record_stream;
pub mod server;
pub mod transcript;

// Big-endian network order

// https://tools.ietf.org/html/rfc8446

// TODO: Validate that extensions are only send with the message types that they
// are allowed to appear on.

// TLS 1.3 ClientHellos are identified as having
//       a legacy_version of 0x0303 and a supported_versions extension
//       present with 0x0304 as the highest version indicated therein.

// Client hello must at least have supported_versions extension to be TLS 1.3
