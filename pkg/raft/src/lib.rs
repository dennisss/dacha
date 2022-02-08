#![feature(proc_macro_hygiene, decl_macro, generators, async_closure)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate crypto;
extern crate google;
extern crate http;
extern crate nix;
extern crate protobuf;
extern crate sstable;
#[macro_use]
extern crate failure;
extern crate rpc_util;
#[macro_use]
extern crate macros;

// TODO: Eventually make this private
pub mod proto;

pub mod atomic;
mod consensus;
mod leader_service;
pub mod log;
pub mod node;
pub mod routing;
pub mod server;
mod sync;

pub use consensus::module::{NotLeaderError, ReadIndex};
pub use leader_service::LeaderServiceWrapper;
pub use log::log::Log;
pub use log::segmented_log::SegmentedLog;
pub use node::{Node, NodeOptions};
pub use proto::ident::LogIndex;
pub use proto::init::{BootstrapRequest, BootstrapResponse, ServerInitStub};
pub use routing::discovery_client::DiscoveryClient;
pub use routing::discovery_server::DiscoveryServer;
pub use routing::multicast::DiscoveryMulticast;
pub use routing::route_channel::RouteChannelFactory;
pub use routing::route_store::{RouteStore, RouteStoreGuard};
pub use server::server::{PendingExecution, PendingExecutionResult, Server, ServerInitialState};
pub use server::state_machine::{StateMachine, StateMachineSnapshot};
