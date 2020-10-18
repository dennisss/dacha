#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;
extern crate libc;

pub mod body;
pub mod chunked;
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
pub mod uri;
pub mod uri_parser;
