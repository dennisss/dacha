#![feature(core_intrinsics, async_await, trait_alias)]

#[macro_use] extern crate common;
#[macro_use] extern crate parsing;
extern crate bytes;
extern crate libc;

mod reader;
mod common_parser;
pub mod uri;
pub mod uri_parser;
mod dns;
pub mod status_code;
pub mod body;
pub mod spec;
pub mod message;
mod message_parser;
pub mod header_parser;
pub mod header;
pub mod chunked;
pub mod transfer_encoding;
pub mod client;
pub mod server;
