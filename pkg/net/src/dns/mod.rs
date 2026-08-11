// Basic binary format defined in https://datatracker.ietf.org/doc/html/rfc1035.
// EDNS defined in https://datatracker.ietf.org/doc/html/rfc6891
// mDNS defined in https://datatracker.ietf.org/doc/html/rfc6762
// DNS-SD defined in https://datatracker.ietf.org/doc/html/rfc6763

mod proto {
    include!(concat!(env!("OUT_DIR"), "/src/dns/proto.rs"));
}
mod client;
mod message;
mod message_builder;
mod message_cell;
mod name;
mod server;
mod constants;

pub use client::*;
pub use server::*;
pub use message::*;
pub use message_builder::*;
pub use name::*;
pub use proto::{Class, OpCode, RecordType, ResponseCode, SRVDataHeader};
pub use constants::*;