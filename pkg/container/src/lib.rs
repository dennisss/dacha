#![feature(async_closure, drain_filter)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate crypto;
extern crate libc;
extern crate nix;
extern crate protobuf;
#[macro_use]
extern crate macros;
extern crate compression;
extern crate rpc;
extern crate sstable;
#[macro_use]
extern crate datastore;
extern crate net;
extern crate rpc_util;
extern crate usb;
#[macro_use]
extern crate regexp_macros;
extern crate automata;
extern crate base_radix;

pub mod init;
pub mod manager;
pub mod meta;
pub mod node;
use container_proto::cluster as proto;
mod runtime;
pub mod service;
mod setup_socket;

pub use manager::main::main as manager_main;
pub use node::main::main as node_main;
pub use proto::*;
pub use runtime::ContainerRuntime;
pub use service::resolver::ServiceResolver;
