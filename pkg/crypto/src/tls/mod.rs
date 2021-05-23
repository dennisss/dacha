#[macro_use]
mod macros;
pub mod alert;
pub mod client;
pub mod extensions;
pub mod handshake;
pub mod key_schedule;
pub mod options;
mod parsing;
pub mod record;
pub mod transcript;
mod record_stream;
mod cipher;
pub mod application_stream;

// Big-endian network order

// https://tools.ietf.org/html/rfc8446

// TODO: Validate that extensions are only send with the message types that they
// are allowed to appear on.

// TLS 1.3 ClientHellos are identified as having
//       a legacy_version of 0x0303 and a supported_versions extension
//       present with 0x0304 as the highest version indicated therein.

// Client hello must at least have supported_versions extension to be TLS 1.3
