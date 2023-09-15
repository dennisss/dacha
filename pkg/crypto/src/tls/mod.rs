#[macro_use]
mod macros;
pub mod alert;
pub mod application_stream;
mod cipher;
mod cipher_suite;
mod cipher_tls12;
pub mod client;
mod constants;
pub mod extensions;
mod extensions_util;
pub mod handshake;
mod handshake_executor;
pub mod handshake_summary;
pub mod key_schedule;
mod key_schedule_helper;
mod key_schedule_tls12;
pub mod options;
mod parsing;
pub mod record;
mod record_stream;
pub mod server;
mod signatures;
pub mod transcript;

pub use client::Client;
pub use handshake_summary::HandshakeSummary;
pub use options::*;
pub use server::Server;

// Big-endian network order

// https://tools.ietf.org/html/rfc8446

// TODO: Validate that extensions are only send with the message types that they
// are allowed to appear on.
