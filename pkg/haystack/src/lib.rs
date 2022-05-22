#![feature(
    async_await,
    futures_api,
    proc_macro_hygiene,
    decl_macro,
    type_alias_enum_variants,
    generators
)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate common;
extern crate byteorder;
extern crate chrono;
extern crate crypto;
extern crate dotenv;
extern crate fs2;
extern crate http;
extern crate mime_sniffer;
extern crate protobuf;
#[macro_use]
extern crate macros;
extern crate protobuf_json;
#[macro_use]
extern crate failure;

macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
}

mod background_thread;
pub mod cache;
pub mod client;
pub mod directory;
mod http_utils;
mod paths;
mod proto;
pub mod store;
pub mod types;

pub use proto::config::*;
