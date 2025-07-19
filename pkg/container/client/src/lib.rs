#![feature(async_closure)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate db_txn_client;
#[macro_use]
extern crate regexp_macros;

pub mod acl;
pub mod credentials;
pub mod env;
pub mod id;
pub mod meta;
pub mod server;
pub mod service;
pub mod throttler;

pub use container_proto::cluster::*;
pub use server::*;
pub use service::resolver::ServiceResolver;
pub use meta::client::ClusterMetaClient;