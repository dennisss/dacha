#![feature(
    proc_macro_hygiene,
    decl_macro,
    type_alias_enum_variants,
    generators,
    async_closure
)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate common;
extern crate byteorder;
extern crate bytes;
extern crate crypto;
extern crate http;
extern crate rand;
extern crate rmp_serde as rmps;
extern crate serde;

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
pub mod server_protos;
pub mod state_machine;

pub mod memory_log;
pub mod node;
pub mod simple_log;
