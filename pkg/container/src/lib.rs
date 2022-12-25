#![feature(async_closure)]

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
extern crate google;
extern crate rpc;
extern crate sstable;
#[macro_use]
extern crate async_std;
#[macro_use]
extern crate datastore;
extern crate net;
extern crate rpc_util;
extern crate usb;
#[macro_use]
extern crate regexp_macros;
extern crate automata;
extern crate radix;

mod capabilities;
pub mod manager;
pub mod meta;
pub mod node;
mod proto;
mod runtime;
pub mod service;

pub use manager::main::main as manager_main;
pub use node::main::main as node_main;
pub use proto::blob::*;
pub use proto::config::*;
pub use proto::job::*;
pub use proto::log::*;
pub use proto::manager::*;
pub use proto::meta::*;
pub use proto::node::*;
pub use proto::node_service::*;
pub use proto::worker::*;
pub use proto::worker_event::*;
pub use runtime::ContainerRuntime;
pub use service::resolver::ServiceResolver;
