#![feature(async_closure)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate db_txn_client;

pub mod acl;
pub mod credentials;
pub mod env;
pub mod id;
pub mod meta;
pub mod server;
pub mod service;

pub use container_proto::cluster::*;
pub use server::*;
pub use service::resolver::ServiceResolver;
