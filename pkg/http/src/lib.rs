#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;

extern crate parsing;

pub mod body;
pub mod chunked;
mod chunked_syntax;
pub mod client;
mod common_parser;
mod dns;
pub mod header;
pub mod header_parser;
pub mod message;
mod message_parser;
mod reader;
pub mod server;
pub mod spec;
pub mod status_code;
pub mod transfer_encoding;
mod transfer_encoding_syntax;
pub mod uri;
pub mod uri_parser;
pub mod method;
pub mod request;
pub mod response;
mod upgrade;
mod upgrade_syntax;