#![feature(async_await, futures_api, proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate diesel;

#[macro_use] extern crate common;
extern crate dotenv;
extern crate rand;
extern crate byteorder;
extern crate arrayref;
extern crate bytes;
extern crate base64;
extern crate fs2;
extern crate mime_sniffer;
extern crate ipnetwork;
extern crate chrono;
extern crate ctrlc;
extern crate http;
extern crate crypto;
extern crate protobuf;
#[macro_use] extern crate macros;
extern crate protobuf_json;
#[macro_use] extern crate failure;

macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
}

mod proto;
mod http_utils;
pub mod types;
mod background_thread;
mod paths;
pub mod store;
pub mod directory;
pub mod cache;
pub mod client;

pub use proto::config::*;
