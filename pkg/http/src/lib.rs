#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate parsing;

pub mod body;
pub mod chunked;
mod chunked_syntax;
pub mod client;
mod common_syntax;
mod dns;
pub mod header;
pub mod header_syntax;
pub mod message;
mod message_syntax;
mod reader;
pub mod server;
pub mod spec;
pub mod status_code;
pub mod encoding;
mod encoding_syntax;
pub mod uri;
pub mod uri_syntax;
pub mod method;
pub mod request;
pub mod response;
mod upgrade;
mod upgrade_syntax;
mod v2;

pub use crate::server::Server;
pub use crate::client::Client;