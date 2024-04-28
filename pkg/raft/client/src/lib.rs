#![feature(proc_macro_hygiene, decl_macro, async_closure)]

#[macro_use]
extern crate common;
extern crate raft_proto;

// TODO: Eventually make this private
pub use raft_proto::raft as proto;

mod routing;

// TODO: Consider moving this entirely back to the 'raft' crate.
pub mod server;

pub mod utils;

pub use routing::discovery_client::{DiscoveryClient, DiscoveryClientOptions};
pub use routing::discovery_server::DiscoveryServer;
pub use routing::multicast::DiscoveryMulticast;
pub use routing::route_channel::RouteChannelFactory;
pub use routing::route_store::{RouteStore, RouteStoreGuard};
