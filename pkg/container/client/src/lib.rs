#![feature(async_closure)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
#[macro_use]
extern crate datastore_meta_client;

pub mod meta;
pub mod service;

use container_proto::cluster as proto;
pub use proto::*;
pub use service::resolver::ServiceResolver;
