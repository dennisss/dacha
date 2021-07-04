#![feature(
    proc_macro_hygiene,
    decl_macro,
    type_alias_enum_variants,
    generators,
    async_closure
)]

#[macro_use]
extern crate common;
extern crate byteorder;
extern crate crypto;
extern crate http;
extern crate rand;
extern crate protobuf;
extern crate google;

#[macro_use]
extern crate macros;

// TODO: Eventually make this private
pub mod proto;

pub mod atomic;
pub mod sync;

pub mod protos; // TODO: Eventually make this private again

pub mod log; // XXX: Likewise should be private
mod state;
//pub mod snapshot; // XXX: May eventually reoccur as a file that holds the
// algorithm for managing whether or not we should trigger snapshots
mod config_state;
pub mod consensus;
pub mod constraint;

//pub mod record_io;

// Higher level complete implementation dealing with actual networking issues
pub mod routing;
pub mod rpc;
// XXX: Should only really be required by the server itself
pub mod discovery;
pub mod server;
pub mod state_machine;

pub mod memory_log;
pub mod node;
pub mod simple_log;
